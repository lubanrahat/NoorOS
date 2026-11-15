//! Machine shutdown and reboot.
//!
//! `shutdown` first asks the chipset's ACPI power-management block to power the
//! machine off (QEMU's PIIX4 implements this at port 0x604), and falls back to
//! the `isa-debug-exit` device used by the NoorOS QEMU runner.

use x86_64::instructions::port::Port;

pub fn shutdown() -> ! {
    serial_println!("[power] ACPI power-off requested");
    // SAFETY: ACPI PM1a control register (QEMU PIIX4); writing SLP_TYP + SLP_EN.
    unsafe {
        Port::<u16>::new(0x604).write(0x2000);
        // Older QEMU/Bochs ACPI port.
        Port::<u16>::new(0xB004).write(0x2000);
        // isa-debug-exit (present only under our QEMU runner). 0x10 = success.
        Port::<u32>::new(0xF4).write(0x10);
    }
    crate::kernel::panic::halt()
}

pub fn reboot() -> ! {
    serial_println!("[power] reboot requested");
    // Pulse the 8042 reset line; this is the traditional 286+ reset method.
    unsafe {
        let mut status: Port<u8> = Port::new(0x64);
        let mut command: Port<u8> = Port::new(0x64);
        for _ in 0..100_000 {
            if status.read() & 0x02 == 0 {
                break;
            }
        }
        command.write(0xFE);
    }

    // Fallback: load an empty IDT and trigger a triple fault.
    let empty_idt = [0u8; 10];
    unsafe {
        core::arch::asm!("lidt [{}]", in(reg) empty_idt.as_ptr(), options(readonly, nostack, preserves_flags));
        core::arch::asm!("int3");
    }
    crate::kernel::panic::halt()
}
