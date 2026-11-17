//! Preemptive round-robin kernel tasks.
//!
//! NoorOS has one address space and one CPU, so a "task" is a kernel thread:
//! its own stack plus a saved interrupt frame. The PIT (IRQ0) preempts the
//! running task every [`QUANTUM_TICKS`] ticks and the scheduler switches to the
//! next ready task.
//!
//! # Switching stacks from an interrupt
//!
//! IRQ0 does **not** use the `x86-interrupt` ABI. `timer_irq_entry` (assembly
//! below) pushes every general-purpose register, calls [`timer_tick_from_irq`],
//! replaces RSP with the value it returns, and then pops the registers and
//! executes `iretq`. The saved frame is therefore exactly:
//!
//! ```text
//! r15 r14 r13 r12 r11 r10 r9 r8 rbp rdi rsi rdx rcx rbx rax rip cs rflags rsp ss
//! ```
//!
//! The five words after the general-purpose registers are the frame the CPU
//! itself pushes on interrupt entry; `iretq` pops all five. A new task starts
//! with a hand-built frame whose RIP is `task_bootstrap`, whose R12 holds the
//! task entry point, and whose RSP points at the frame itself (8 mod 16, so the
//! entry point sees the SysV ABI alignment). `task_bootstrap` calls that entry
//! and then [`task_exit`]. Interrupts are enabled again by the `iretq` (RFLAGS
//! has IF set), so a task runs preemptively from its first instruction.
//!
//! # Locking
//!
//! The scheduler lock must never be taken by a task with interrupts enabled:
//! IRQ0 runs with IF cleared and would otherwise deadlock on a lock held by the
//! task it interrupted. Every non-IRQ access goes through [`with_scheduler`],
//! which disables interrupts for the duration. The scheduler never allocates,
//! so it also never touches the heap lock. Demo tasks only use atomics; they
//! never take a lock that another task might hold when preempted.
//!
//! The `x86_64-unknown-none` target disables the red zone and uses soft-float,
//! so an interrupt can push a frame below RSP safely and no SSE/FPU state has
//! to be saved.

use alloc::alloc::{Layout, alloc_zeroed, dealloc};
use core::mem::size_of;
use core::ptr;
use core::sync::atomic::{AtomicU64, Ordering};

use spin::Mutex;
use x86_64::VirtAddr;
use x86_64::instructions::interrupts::without_interrupts;
use x86_64::instructions::segmentation::{CS, SS, Segment};

pub mod demo;

/// Maximum number of tasks, including the desktop task.
pub const MAX_TASKS: usize = 8;
/// Kernel stack size for a spawned task.
pub const STACK_SIZE: usize = 32 * 1024;
/// Ticks a task may run before the scheduler switches away (2 x 10 ms).
pub const QUANTUM_TICKS: u32 = 2;
/// Milliseconds represented by one PIT tick.
pub const TICK_MS: u64 = 1000 / crate::drivers::timer::TICK_HZ;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TaskState {
    Ready,
    Running,
    /// Reserved for tasks that wait on an event; the demo tasks never block.
    #[allow(dead_code)]
    Blocked,
    Exited,
}

impl TaskState {
    pub fn name(self) -> &'static str {
        match self {
            TaskState::Ready => "ready",
            TaskState::Running => "running",
            TaskState::Blocked => "blocked",
            TaskState::Exited => "exited",
        }
    }
}

/// The register frame saved by `timer_irq_entry`.
///
/// Field order is the order the registers appear on the stack after the pushes
/// in the assembly trampoline, followed by the five words the CPU pushes on
/// interrupt entry. `rsp` and `ss` are restored by `iretq`, so they must be
/// present in a hand-built frame for a new task.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SavedRegisters {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

/// One task control block.
pub struct Task {
    pub tid: u64,
    pub name: &'static str,
    pub state: TaskState,
    /// Stack pointer of the saved register frame.
    pub saved_rsp: usize,
    /// Diagnostic copy of the saved frame, refreshed on every tick.
    pub regs: SavedRegisters,
    /// PIT ticks charged to this task.
    pub cpu_ticks: u64,
    pub quantum_ticks: u32,
    pub quantum_left: u32,
    #[allow(dead_code)] // kept so a future scheduler can reclaim exited stacks
    stack_base: usize,
    #[allow(dead_code)]
    stack_size: usize,
}

impl Task {
    fn info(&self) -> TaskInfo {
        TaskInfo {
            tid: self.tid,
            name: self.name,
            state: self.state,
            cpu_ticks: self.cpu_ticks,
            quantum_left: self.quantum_left,
            quantum_ticks: self.quantum_ticks,
        }
    }
}

/// Copy of a task's scheduling state, safe to hold outside the scheduler lock.
#[derive(Clone, Copy)]
pub struct TaskInfo {
    pub tid: u64,
    pub name: &'static str,
    pub state: TaskState,
    pub cpu_ticks: u64,
    pub quantum_left: u32,
    pub quantum_ticks: u32,
}

#[derive(Clone, Copy)]
pub struct SchedulerStats {
    pub tasks: usize,
    pub switches: u64,
    pub quantum_ms: u64,
}

struct Scheduler {
    tasks: [Option<Task>; MAX_TASKS],
    current: usize,
    next_tid: u64,
    switches: u64,
    ready: bool,
}

impl Scheduler {
    const fn new() -> Self {
        Self {
            tasks: [const { None }; MAX_TASKS],
            current: 0,
            next_tid: 1,
            switches: 0,
            ready: false,
        }
    }

    fn pick_next(&self) -> usize {
        for offset in 1..=self.tasks.len() {
            let index = (self.current + offset) % self.tasks.len();
            if let Some(task) = &self.tasks[index]
                && matches!(task.state, TaskState::Ready | TaskState::Running)
            {
                return index;
            }
        }
        self.current
    }
}

static SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());
static KERNEL_CS: AtomicU64 = AtomicU64::new(0x08);
static KERNEL_SS: AtomicU64 = AtomicU64::new(0x10);

fn with_scheduler<R>(f: impl FnOnce(&mut Scheduler) -> R) -> R {
    without_interrupts(|| {
        let mut scheduler = SCHEDULER.lock();
        f(&mut scheduler)
    })
}

/// Creates the desktop task and spawns the two background demo tasks.
///
/// Must run before `interrupts::init()` enables IRQ0, and after the heap is
/// available (task stacks are allocated from it).
pub fn init() -> Result<(), &'static str> {
    KERNEL_CS.store(CS::get_reg().0 as u64, Ordering::Relaxed);
    KERNEL_SS.store(SS::get_reg().0 as u64, Ordering::Relaxed);

    with_scheduler(|scheduler| {
        if scheduler.ready {
            return;
        }
        scheduler.tasks[0] = Some(Task {
            tid: 0,
            name: "desktop",
            state: TaskState::Running,
            saved_rsp: 0,
            regs: SavedRegisters::default(),
            cpu_ticks: 0,
            quantum_ticks: QUANTUM_TICKS,
            quantum_left: QUANTUM_TICKS,
            stack_base: 0,
            stack_size: 0,
        });
        scheduler.current = 0;
        scheduler.next_tid = 1;
        scheduler.ready = true;
    });

    spawn("uptime", demo::uptime_task)?;
    spawn("primes", demo::prime_task)?;
    Ok(())
}

/// Allocates a kernel stack and adds a ready task to the round-robin queue.
pub fn spawn(name: &'static str, entry: extern "C" fn()) -> Result<u64, &'static str> {
    let layout =
        Layout::from_size_align(STACK_SIZE, 16).map_err(|_| "invalid kernel stack layout")?;
    // SAFETY: the layout has a non-zero size; the block is zeroed and only
    // released below if the task table is full.
    let base = unsafe { alloc_zeroed(layout) };
    if base.is_null() {
        return Err("cannot allocate a kernel stack");
    }

    let top = base as usize + STACK_SIZE;
    // The frame sits just below the top of the stack. `rsp` is set to
    // `frame_addr`, which is 8 mod 16; `task_bootstrap`'s `sub rsp, 8` then
    // aligns the stack so the called entry point sees the SysV ABI's 8 mod 16.
    let frame_addr = top - 8 - size_of::<SavedRegisters>();
    let frame = SavedRegisters {
        r12: entry as usize as u64,
        rip: task_bootstrap as *const () as usize as u64,
        cs: KERNEL_CS.load(Ordering::Relaxed),
        rflags: 0x202, // reserved bit 1 + interrupts enabled
        rsp: frame_addr as u64,
        ss: KERNEL_SS.load(Ordering::Relaxed),
        ..SavedRegisters::default()
    };
    // SAFETY: `frame_addr` is eight-byte aligned inside the zeroed stack.
    unsafe {
        ptr::write(frame_addr as *mut SavedRegisters, frame);
    }

    let tid = with_scheduler(|scheduler| {
        let slot = scheduler.tasks.iter().position(|task| task.is_none())?;
        let tid = scheduler.next_tid;
        scheduler.next_tid += 1;
        scheduler.tasks[slot] = Some(Task {
            tid,
            name,
            state: TaskState::Ready,
            saved_rsp: frame_addr,
            regs: frame,
            cpu_ticks: 0,
            quantum_ticks: QUANTUM_TICKS,
            quantum_left: QUANTUM_TICKS,
            stack_base: base as usize,
            stack_size: STACK_SIZE,
        });
        Some(tid)
    });

    match tid {
        Some(tid) => Ok(tid),
        None => {
            // SAFETY: the block was allocated with this layout and never used.
            unsafe { dealloc(base, layout) };
            Err("the task table is full")
        }
    }
}

/// Snapshot of every task, in slot order.
///
/// The copy happens with interrupts disabled, but the returned `Vec` is built
/// afterwards so the heap lock is never taken in an interrupt-sensitive path.
pub fn tasks() -> alloc::vec::Vec<TaskInfo> {
    let mut buffer: [Option<TaskInfo>; MAX_TASKS] = [None; MAX_TASKS];
    let mut count = 0;
    with_scheduler(|scheduler| {
        for task in scheduler.tasks.iter().flatten() {
            buffer[count] = Some(task.info());
            count += 1;
        }
    });
    buffer[..count].iter().flatten().copied().collect()
}

pub fn stats() -> SchedulerStats {
    with_scheduler(|scheduler| SchedulerStats {
        tasks: scheduler.tasks.iter().flatten().count(),
        switches: scheduler.switches,
        quantum_ms: QUANTUM_TICKS as u64 * TICK_MS,
    })
}

/// Marks the calling task as exited and parks it until the next tick.
///
/// The stack is intentionally not freed: freeing it would take the heap lock
/// from a task that may be preempted, and a later scheduler could reclaim it
/// with a dedicated reaper task.
#[unsafe(no_mangle)]
pub extern "C" fn task_exit() -> ! {
    with_scheduler(|scheduler| {
        let current = scheduler.current;
        if let Some(task) = scheduler.tasks[current].as_mut() {
            task.state = TaskState::Exited;
        }
    });
    loop {
        x86_64::instructions::hlt();
    }
}

/// Called by the assembly IRQ0 trampoline with the saved frame pointer.
#[unsafe(no_mangle)]
pub extern "C" fn timer_tick_from_irq(rsp: usize) -> usize {
    crate::drivers::timer::on_interrupt();
    crate::interrupts::acknowledge_timer();
    scheduler_tick(rsp)
}

/// Charges the tick, decrements the quantum and returns the next stack pointer.
fn scheduler_tick(rsp: usize) -> usize {
    let mut scheduler = SCHEDULER.lock();
    if !scheduler.ready {
        return rsp;
    }

    let current = scheduler.current;
    let mut switch = false;
    if let Some(task) = scheduler.tasks[current].as_mut() {
        task.cpu_ticks += 1;
        // SAFETY: `rsp` points at the frame pushed by `timer_irq_entry`, which
        // has exactly the layout of `SavedRegisters`.
        task.regs = unsafe { ptr::read(rsp as *const SavedRegisters) };
        task.saved_rsp = rsp;
        switch = match task.state {
            TaskState::Running => {
                task.quantum_left = task.quantum_left.saturating_sub(1);
                task.quantum_left == 0
            }
            // A task that blocked or exited must leave the CPU immediately.
            TaskState::Blocked | TaskState::Exited => true,
            TaskState::Ready => false,
        };
    }
    if !switch {
        return rsp;
    }

    let next = scheduler.pick_next();
    if next == current {
        // Nothing else is runnable; give the current task a fresh quantum.
        if let Some(task) = scheduler.tasks[current].as_mut() {
            task.quantum_left = task.quantum_ticks;
        }
        return rsp;
    }

    if let Some(task) = scheduler.tasks[current].as_mut()
        && task.state == TaskState::Running
    {
        task.state = TaskState::Ready;
    }
    scheduler.switches += 1;
    scheduler.current = next;
    let task = scheduler.tasks[next]
        .as_mut()
        .expect("a runnable task slot cannot be empty");
    task.state = TaskState::Running;
    task.quantum_left = task.quantum_ticks;
    task.saved_rsp
}

/// Address of the assembly IRQ0 entry, installed in the IDT.
pub fn timer_handler_addr() -> VirtAddr {
    VirtAddr::new(timer_irq_entry as *const () as usize as u64)
}

unsafe extern "C" {
    fn timer_irq_entry();
    fn task_bootstrap();
}

core::arch::global_asm!(
    r#"
    .section .text

    .global timer_irq_entry
    .type timer_irq_entry, @function
timer_irq_entry:
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15

    mov rdi, rsp
    and rsp, -16
    call timer_tick_from_irq
    mov rsp, rax

    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    iretq
    .size timer_irq_entry, . - timer_irq_entry

    .global task_bootstrap
    .type task_bootstrap, @function
task_bootstrap:
    sub rsp, 8
    call r12
    call task_exit
    ud2
    .size task_bootstrap, . - task_bootstrap
"#
);
