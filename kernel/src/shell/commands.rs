//! The Noor Shell command interpreter.
//!
//! All output is produced from values the kernel really has: the bootloader
//! memory map, the heap allocator counters, CPUID, the PIT tick counter and the
//! CMOS real-time clock. Nothing here is simulated or randomised.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::drivers::{cpu, framebuffer, keyboard, mouse, rtc, timer};
use crate::{fs, memory, tasking, util};
use x86_64::VirtAddr;

use super::parser::parse;

pub const VERSION: &str = "0.1.0";
pub const KERNEL_NAME: &str = "Noor Kernel";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Shutdown,
    Reboot,
}

pub struct Output {
    pub lines: Vec<String>,
    pub clear: bool,
    pub action: Action,
    /// True when the command was not recognised (rendered in the error colour).
    pub error: bool,
}

impl Output {
    fn text(lines: Vec<String>) -> Self {
        Self {
            lines,
            clear: false,
            action: Action::None,
            error: false,
        }
    }

    fn message(line: &str) -> Self {
        Self::text(vec![String::from(line)])
    }

    fn error(line: &str) -> Self {
        Self {
            error: true,
            ..Self::text(vec![String::from(line)])
        }
    }
}

pub fn execute(input: &str) -> Output {
    let trimmed = input.trim();
    if let Some((left, target)) = split_redirection(trimmed) {
        return redirect(left, target);
    }

    let command = parse(trimmed);
    if command.is_empty() {
        return Output::text(Vec::new());
    }

    if command.matches("help") {
        help()
    } else if command.matches("clear") {
        Output {
            clear: true,
            ..Output::text(Vec::new())
        }
    } else if command.matches("about") {
        about()
    } else if command.matches("info") {
        info()
    } else if command.matches("memory") {
        memory_info()
    } else if command.matches("version") {
        version()
    } else if command.matches("echo") {
        Output::message(unquote(command.args))
    } else if command.matches("uptime") {
        uptime()
    } else if command.matches("ls") {
        ls(command.args)
    } else if command.matches("cat") {
        cat(command.args)
    } else if command.matches("touch") {
        touch(command.args)
    } else if command.matches("rm") {
        rm(command.args)
    } else if command.matches("mkdir") {
        mkdir(command.args)
    } else if command.matches("ps") {
        ps()
    } else if command.matches("shutdown") {
        shutdown()
    } else if command.matches("reboot") {
        reboot()
    } else {
        Output::error("Command not found. Type 'help' for available commands.")
    }
}

/// Splits `echo "text" > /file` at the first `>` outside double quotes.
fn split_redirection(input: &str) -> Option<(&str, &str)> {
    let mut quoted = false;
    for (index, ch) in input.char_indices() {
        match ch {
            '"' => quoted = !quoted,
            '>' if !quoted => return Some((input[..index].trim(), input[index + 1..].trim())),
            _ => {}
        }
    }
    None
}

fn redirect(left: &str, target: &str) -> Output {
    if target.is_empty() {
        return Output::error("usage: echo \"text\" > <file>");
    }
    if target.starts_with('>') {
        return Output::error("Appending (>>) is not supported; use > to replace a file.");
    }
    let command = parse(left);
    if !command.matches("echo") {
        return Output::error("Only 'echo' supports > redirection.");
    }
    let text = unquote(command.args);
    match fs::write_file(target, text.as_bytes()) {
        Ok(bytes) => Output::message(&format!("Wrote {bytes} bytes to {target}")),
        Err(error) => Output::error(&format!("{target}: {}", error.message())),
    }
}

/// Removes one pair of surrounding double quotes from an echo argument.
fn unquote(text: &str) -> &str {
    let text = text.trim();
    if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') {
        &text[1..text.len() - 1]
    } else {
        text
    }
}

fn help() -> Output {
    let rows = [
        ("help", "commands", "clear", "clear screen"),
        ("about", "about", "info", "system info"),
        ("memory", "memory", "version", "version"),
        ("echo", "print text", "uptime", "uptime/clock"),
        ("ls", "list files", "cat", "print file"),
        ("touch", "create file", "rm", "delete file"),
        ("mkdir", "new dir", "ps", "processes"),
        ("shutdown", "power off", "reboot", "restart"),
    ];
    let mut lines = vec![String::from("NOOR TERMINAL"), String::new()];
    for (name, description, name2, description2) in rows {
        lines.push(format!(
            "  {name:<9} {description:<12} {name2:<9} {description2}"
        ));
    }
    lines.push(String::new());
    lines.push(String::from("Files: echo \"text\" > /file  writes a file."));
    Output::text(lines)
}

fn about() -> Output {
    Output::text(vec![
        String::from("NOOROS 0.1.0"),
        String::from("A Light Beyond Computing."),
        String::new(),
        String::from("NoorOS is a minimal graphical educational OS"),
        String::from("built in Rust to demonstrate operating system"),
        String::from("concepts: virtual memory, preemptive scheduling,"),
        String::from("an in-memory file system and framebuffer rendering."),
        String::new(),
        format!("Kernel:       {KERNEL_NAME}"),
        String::from("Architecture: x86_64"),
        String::from("Language:     Rust (no_std, no_main)"),
        String::from("Project type: Educational operating system"),
    ])
}

fn version() -> Output {
    let scheduler = tasking::stats();
    let filesystem = fs::stats();
    Output::text(vec![
        format!("NoorOS {VERSION}"),
        format!("{KERNEL_NAME} {VERSION}"),
        String::from("Target: x86_64-unknown-none"),
        String::from("Language: Rust (no_std, no_main)"),
        format!(
            "Heap: {} mapped on demand",
            util::bytes(memory::heap_capacity())
        ),
        format!(
            "Scheduler: round-robin, {} tasks, {} ms quantum",
            scheduler.tasks, scheduler.quantum_ms
        ),
        format!(
            "MemFS: {} files, {} directories",
            filesystem.files, filesystem.directories
        ),
    ])
}

fn info() -> Output {
    let mut lines = Vec::new();
    lines.push(String::from("SYSTEM INFORMATION"));
    lines.push(String::new());
    lines.push(format!("  {:<15} NoorOS {VERSION}", "Operating system"));
    lines.push(format!("  {:<15} {KERNEL_NAME} (monolithic)", "Kernel"));
    lines.push(format!("  {:<15} x86_64", "Architecture"));

    let cpu = cpu::info();
    lines.push(format!(
        "  {:<15} {}",
        "CPU brand",
        truncate(&cpu.brand, 30)
    ));
    lines.push(format!("  {:<15} {}", "CPU vendor", cpu.vendor));
    lines.push(format!(
        "  {:<15} {} (CPUID)",
        "Logical CPUs", cpu.logical_cpus
    ));
    lines.push(format!(
        "  {:<15} {}",
        "Hypervisor",
        if cpu.hypervisor {
            "yes (CPUID)"
        } else {
            "no (CPUID)"
        }
    ));

    if let Some(fb) = framebuffer::info() {
        lines.push(format!(
            "  {:<15} {}x{}, {} bpp, {:?}",
            "Framebuffer", fb.width, fb.height, fb.bytes_per_pixel, fb.pixel_format
        ));
    }
    if let Some(map) = memory::map() {
        lines.push(format!(
            "  {:<15} {} of {}",
            "Memory usable",
            util::bytes(map.usable_bytes),
            util::bytes(map.total_bytes)
        ));
        lines.push(format!("  {:<15} {}", "Memory regions", map.regions.len()));
    }
    let heap = memory::heap_stats();
    lines.push(format!(
        "  {:<15} {}, {} used",
        "Kernel heap",
        util::bytes(heap.size),
        util::bytes(heap.used)
    ));
    lines.push(format!(
        "  {:<15} {}",
        "Uptime",
        util::uptime(timer::uptime_ms())
    ));
    if let Some(now) = rtc::now() {
        lines.push(format!(
            "  {:<15} {} {}",
            "RTC clock",
            now.date(),
            now.clock_hm()
        ));
    }
    let frames = memory::frame_stats();
    lines.push(format!(
        "  {:<15} {} free / {} managed frames",
        "Physical frames", frames.free, frames.usable
    ));
    lines.push(format!(
        "  {:<15} active (bitmap frames + page tables)",
        "Virtual memory"
    ));
    let scheduler = tasking::stats();
    lines.push(format!(
        "  {:<15} round-robin, {} tasks, {} ms",
        "Scheduler", scheduler.tasks, scheduler.quantum_ms
    ));
    let filesystem = fs::stats();
    lines.push(format!(
        "  {:<15} {} files, {} dirs, {}",
        "MemFS",
        filesystem.files,
        filesystem.directories,
        util::bytes(filesystem.bytes)
    ));
    lines.push(format!(
        "  {:<15} {} timer, {} kbd, {} mouse",
        "Interrupts",
        timer::interrupt_count(),
        keyboard::interrupt_count(),
        mouse::interrupt_count()
    ));
    Output::text(lines)
}

fn memory_info() -> Output {
    let Some(map) = memory::map() else {
        return Output::message("Memory map is not available.");
    };
    let frames = memory::frame_stats();
    let heap = memory::heap_stats();

    // Keep this within the terminal's 18-row viewport. The full region table
    // lives in the Monitor application; this view answers the memory questions
    // that matter interactively: how much RAM, how many frames, and whether a
    // virtual address really translates through the active page tables.
    let mut lines = Vec::new();
    lines.push(String::from("PHYSICAL MEMORY"));
    lines.push(format!(
        "  Total RAM  {} ({} regions)",
        util::bytes(map.total_bytes),
        map.regions.len()
    ));
    lines.push(format!("  Usable     {}", util::bytes(map.usable_bytes)));
    lines.push(format!("  Reserved   {}", util::bytes(map.reserved_bytes)));
    lines.push(String::new());
    lines.push(String::from("FRAME ALLOCATOR (4 KiB frames)"));
    lines.push(format!(
        "  Managed    {} frames ({})",
        frames.usable,
        util::bytes(frames.usable * memory::FRAME_SIZE)
    ));
    lines.push(format!(
        "  Used       {} frames ({})",
        frames.used,
        util::bytes(frames.used * memory::FRAME_SIZE)
    ));
    lines.push(format!(
        "  Free       {} frames ({})",
        frames.free,
        util::bytes(frames.free * memory::FRAME_SIZE)
    ));
    lines.push(String::new());
    lines.push(String::from("KERNEL HEAP (mapped on demand)"));
    lines.push(format!(
        "  Mapped     {} at {:#018X}",
        util::bytes(heap.mapped),
        heap.base
    ));
    lines.push(format!(
        "  In use     {} of {} usable",
        util::bytes(heap.used),
        util::bytes(heap.size)
    ));
    lines.push(String::from("ADDRESS TRANSLATION"));
    lines.push(translation_line("heap", memory::heap_base()));
    lines.push(translation_line(
        "kernel",
        VirtAddr::new(VERSION.as_ptr() as u64),
    ));
    let probe = VirtAddr::new(0x0000_2000_0000_0000);
    let probe_result = match memory::translate_addr(probe) {
        Some(physical) => format!("{:#018X} (mapped)", physical.as_u64()),
        None => String::from("None (unmapped)"),
    };
    lines.push(format!(
        "  {:<6} {:#018X} -> {}",
        "probe",
        probe.as_u64(),
        probe_result
    ));
    Output::text(lines)
}

fn translation_line(label: &str, address: VirtAddr) -> String {
    match memory::translate_addr(address) {
        Some(physical) => format!(
            "  {:<6} {:#018X} -> {:#018X}",
            label,
            address.as_u64(),
            physical.as_u64()
        ),
        None => format!("  {:<6} {:#018X} -> None", label, address.as_u64()),
    }
}

fn ls(path: &str) -> Output {
    let path = if path.trim().is_empty() {
        "/"
    } else {
        path.trim()
    };
    match fs::list(path) {
        Ok(entries) => {
            let mut lines = vec![
                format!("PATH {path}"),
                format!("  {:<20} {:<5} {:>8}", "NAME", "TYPE", "SIZE"),
            ];
            if entries.is_empty() {
                lines.push(String::from("  (empty directory)"));
            }
            for entry in entries.iter().take(40) {
                let kind = match entry.file_type {
                    fs::FileType::Directory => "dir",
                    fs::FileType::RegularFile => "file",
                };
                let size = match entry.file_type {
                    fs::FileType::Directory => String::from("-"),
                    fs::FileType::RegularFile => util::bytes(entry.size),
                };
                lines.push(format!(
                    "  {:<20} {:<5} {:>8}",
                    truncate(&entry.name, 20),
                    kind,
                    size
                ));
            }
            if entries.len() > 40 {
                lines.push(format!("  ... {} more entries", entries.len() - 40));
            }
            Output::text(lines)
        }
        Err(fs::FsError::NotADirectory) => match fs::stat(path) {
            Ok(entry) => Output::text(vec![format!(
                "{path}  file  {}  inode {}",
                util::bytes(entry.size),
                entry.inode
            )]),
            Err(error) => Output::error(&format!("{path}: {}", error.message())),
        },
        Err(error) => Output::error(&format!("{path}: {}", error.message())),
    }
}

fn cat(path: &str) -> Output {
    if path.trim().is_empty() {
        return Output::error("usage: cat <file>");
    }
    let path = path.trim();
    match fs::read_file(path) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            let total = text.lines().count();
            let mut lines: Vec<String> = text.lines().take(120).map(String::from).collect();
            if lines.is_empty() {
                lines.push(String::from("(empty file)"));
            } else if total > lines.len() {
                lines.push(format!("... {} more lines", total - lines.len()));
            }
            Output::text(lines)
        }
        Err(error) => Output::error(&format!("{path}: {}", error.message())),
    }
}

fn touch(path: &str) -> Output {
    if path.trim().is_empty() {
        return Output::error("usage: touch <file>");
    }
    let path = path.trim();
    match fs::create_file(path) {
        Ok(inode) => Output::message(&format!("Created {path} (inode {inode})")),
        Err(fs::FsError::AlreadyExists) => Output::message(&format!("Already exists: {path}")),
        Err(error) => Output::error(&format!("{path}: {}", error.message())),
    }
}

fn rm(path: &str) -> Output {
    if path.trim().is_empty() {
        return Output::error("usage: rm <file>");
    }
    let path = path.trim();
    match fs::delete(path) {
        Ok(()) => Output::message(&format!("Removed {path}")),
        Err(error) => Output::error(&format!("{path}: {}", error.message())),
    }
}

fn mkdir(path: &str) -> Output {
    if path.trim().is_empty() {
        return Output::error("usage: mkdir <directory>");
    }
    let path = path.trim();
    match fs::create_dir(path) {
        Ok(inode) => Output::message(&format!("Created directory {path} (inode {inode})")),
        Err(error) => Output::error(&format!("{path}: {}", error.message())),
    }
}

fn ps() -> Output {
    let tasks = tasking::tasks();
    let scheduler = tasking::stats();
    let demo = tasking::demo::stats();
    let mut lines = vec![
        format!(
            "TASKS  round-robin @ {} Hz  -  {} switches",
            timer::TICK_HZ,
            scheduler.switches
        ),
        format!("  {:<4} {:<10} {:<9} {:>8}", "TID", "NAME", "STATE", "CPU"),
    ];
    for task in &tasks {
        lines.push(format!(
            "  {:<4} {:<10} {:<9} {:>6} ms",
            task.tid,
            task.name,
            task.state.name(),
            task.cpu_ticks * tasking::TICK_MS
        ));
    }
    lines.push(String::new());
    lines.push(format!("Uptime counter: {} ticks", demo.uptime_ticks));
    lines.push(format!("Primes found:   {}", demo.primes));
    lines.push(format!(
        "Quantum:        {} ms ({} ticks)",
        scheduler.quantum_ms,
        tasking::QUANTUM_TICKS
    ));
    Output::text(lines)
}

fn uptime() -> Output {
    let ticks = timer::ticks();
    let mut lines = vec![
        format!("Uptime: {}", util::uptime(timer::uptime_ms())),
        format!("PIT ticks: {ticks} at {} Hz", timer::TICK_HZ),
        format!("Timer interrupts: {}", timer::interrupt_count()),
    ];
    if let Some(now) = rtc::now() {
        lines.push(format!("RTC clock: {} {}", now.date(), now.clock()));
    }
    Output::text(lines)
}

fn shutdown() -> Output {
    Output {
        lines: vec![
            String::from("Shutting down NoorOS..."),
            String::from("Sending ACPI power-off command."),
        ],
        clear: false,
        action: Action::Shutdown,
        error: false,
    }
}

fn reboot() -> Output {
    Output {
        lines: vec![
            String::from("Rebooting NoorOS..."),
            String::from("Pulsing the 8042 reset line."),
        ],
        clear: false,
        action: Action::Reboot,
        error: false,
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(3)).collect();
    out.push_str("...");
    out
}
