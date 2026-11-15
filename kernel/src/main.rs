//! Noor Kernel - the NoorOS kernel entry point.
//!
//! NoorOS is a `no_std`, `no_main` x86_64 kernel. The `bootloader` crate loads
//! this ELF, switches the CPU to long mode, sets up a stack and hands over a
//! `BootInfo` structure containing the framebuffer and physical memory map.

#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

#[macro_use]
mod macros;

mod applications;
mod boot;
mod desktop;
mod drivers;
mod graphics;
mod interrupts;
mod kernel;
mod memory;
mod shell;
mod util;

use bootloader_api::{BootInfo, BootloaderConfig, entry_point};

/// Bootloader configuration embedded in the kernel ELF.
///
/// The stack is larger than the 80 KiB default because the desktop renders
/// nested UI code; the framebuffer itself lives in bootloader-managed memory.
pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.kernel_stack_size = 128 * 1024;
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    // Firmware and the bootloader may leave maskable interrupts enabled, with
    // the 8259 PIC still on its default vectors. Until our own IDT exists that
    // means IRQ0 would be delivered as vector 8 (double fault), so keep the CPU
    // quiet until `interrupts::init` installs the table and remaps the PIC.
    x86_64::instructions::interrupts::disable();
    kernel::init::start(boot_info)
}
