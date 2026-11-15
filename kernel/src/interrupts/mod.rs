//! Interrupt descriptor table, PIC remapping and IRQ handlers.
//!
//! NoorOS installs a table with CPU exception handlers plus IRQ0 (PIT) and
//! IRQ1 (PS/2 keyboard). All other vectors fall back to the CPU defaults; this
//! is a minimal educational setup, not a complete fault-tolerant kernel.

use pic8259::ChainedPics;
use spin::{Mutex, Once};
use x86_64::instructions::port::Port;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

use crate::drivers::{keyboard, timer};

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

// SAFETY: the offsets 32..48 do not collide with CPU exception vectors.
pub static PICS: Mutex<ChainedPics> =
    Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

static IDT: Once<InterruptDescriptorTable> = Once::new();

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
    Keyboard,
}

impl InterruptIndex {
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

fn read_cr2() -> u64 {
    let value: u64;
    // SAFETY: reading CR2 has no side effects.
    unsafe {
        core::arch::asm!("mov {}, cr2", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

fn build_idt() -> InterruptDescriptorTable {
    let mut idt = InterruptDescriptorTable::new();
    idt.breakpoint.set_handler_fn(breakpoint_handler);
    idt.double_fault.set_handler_fn(double_fault_handler);
    idt.page_fault.set_handler_fn(page_fault_handler);
    idt.general_protection_fault
        .set_handler_fn(general_protection_fault_handler);
    idt.invalid_opcode.set_handler_fn(invalid_opcode_handler);
    idt[InterruptIndex::Timer.as_u8()].set_handler_fn(timer_handler);
    idt[InterruptIndex::Keyboard.as_u8()].set_handler_fn(keyboard_handler);
    idt
}

/// Loads the IDT, remaps the PIC and enables interrupts.
pub fn init() {
    IDT.call_once(build_idt).load();
    unsafe {
        PICS.lock().initialize();
    }
    // Unmask IRQ0 (timer) and IRQ1 (keyboard) on the master PIC; slave masked.
    // SAFETY: standard 8259A mask registers.
    unsafe {
        Port::<u8>::new(0x21).write(0xFC);
        Port::<u8>::new(0xA1).write(0xFF);
    }
    x86_64::instructions::interrupts::enable();
}

fn end_of_interrupt(index: InterruptIndex) {
    // SAFETY: the PIC was initialised with the same vector offsets.
    unsafe {
        PICS.lock().notify_end_of_interrupt(index.as_u8());
    }
}

extern "x86-interrupt" fn timer_handler(_frame: InterruptStackFrame) {
    timer::on_interrupt();
    end_of_interrupt(InterruptIndex::Timer);
}

extern "x86-interrupt" fn keyboard_handler(_frame: InterruptStackFrame) {
    // SAFETY: port 0x60 is the PS/2 data port.
    let byte = unsafe { Port::<u8>::new(0x60).read() };
    keyboard::on_scancode(byte);
    end_of_interrupt(InterruptIndex::Keyboard);
}

extern "x86-interrupt" fn breakpoint_handler(frame: InterruptStackFrame) {
    serial_println!("[exception] BREAKPOINT at {:#?}", frame);
}

extern "x86-interrupt" fn invalid_opcode_handler(frame: InterruptStackFrame) {
    crate::kernel::fatal::exception(
        "INVALID OPCODE",
        "The CPU executed an invalid instruction.",
        &frame,
    );
}

extern "x86-interrupt" fn general_protection_fault_handler(
    frame: InterruptStackFrame,
    error_code: u64,
) {
    let message = alloc::format!("General protection fault (error {error_code:#X}).");
    crate::kernel::fatal::exception("GENERAL PROTECTION FAULT", &message, &frame);
}

extern "x86-interrupt" fn page_fault_handler(
    frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    let address = read_cr2();
    let message = alloc::format!(
        "Page fault at {address:#X}\n  error: {error_code:?}\n  present={} write={} user={}",
        !error_code.contains(PageFaultErrorCode::PROTECTION_VIOLATION),
        error_code.contains(PageFaultErrorCode::CAUSED_BY_WRITE),
        error_code.contains(PageFaultErrorCode::USER_MODE),
    );
    crate::kernel::fatal::exception("PAGE FAULT", &message, &frame);
}

extern "x86-interrupt" fn double_fault_handler(frame: InterruptStackFrame, error_code: u64) -> ! {
    let message = alloc::format!("Double fault (error {error_code:#X}).");
    crate::kernel::fatal::exception("DOUBLE FAULT", &message, &frame);
}
