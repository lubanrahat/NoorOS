//! Kernel initialisation sequence.
//!
//! Order matters:
//!   1. serial first, so every later failure is visible on the host,
//!   2. framebuffer next, so failures can also be shown to the user,
//!   3. heap, then the memory-map snapshot (the snapshot allocates),
//!   4. keyboard parser, PIT and finally the IDT/PIC (interrupts on),
//!   5. desktop.

use bootloader_api::BootInfo;

use crate::boot::BootScreen;
use crate::desktop;
use crate::drivers::{cpu, framebuffer, keyboard, serial, timer};
use crate::interrupts;
use crate::{memory, util};

pub fn start(boot_info: &'static mut BootInfo) -> ! {
    serial::init();
    serial_println!();
    serial_println!("==============================================");
    serial_println!(" NoorOS 0.1.0 - Noor Kernel");
    serial_println!(" A Light Beyond Computing.");
    serial_println!("==============================================");
    serial_println!(
        "[boot] entered kernel; bootloader_api {}.{}.{}, kernel ELF at {:#X} ({} bytes)",
        boot_info.api_version.version_major(),
        boot_info.api_version.version_minor(),
        boot_info.api_version.version_patch(),
        boot_info.kernel_addr,
        boot_info.kernel_len
    );

    // --- Framebuffer -------------------------------------------------------
    let Some(framebuffer_info) = boot_info.framebuffer.as_mut().map(|fb| {
        let info = fb.info();
        framebuffer::init(fb.buffer_mut(), info);
        info
    }) else {
        serial_println!("[fatal] the bootloader did not provide a framebuffer");
        crate::kernel::panic::halt();
    };

    serial_println!(
        "[boot] framebuffer {}x{}, stride {}, {} bpp, {:?}",
        framebuffer_info.width,
        framebuffer_info.height,
        framebuffer_info.stride,
        framebuffer_info.bytes_per_pixel,
        framebuffer_info.pixel_format
    );

    // --- Memory ------------------------------------------------------------
    // The heap must exist before anything allocates: the boot screen itself is
    // built from `String`s, so every earlier step has to be allocation-free.
    memory::init_heap();
    let heap = memory::heap_stats();
    serial_println!(
        "[boot] framebuffer size: {}",
        util::bytes(framebuffer_info.byte_len as u64)
    );

    let note = alloc::format!(
        "{}x{} framebuffer",
        framebuffer_info.width,
        framebuffer_info.height
    );

    let mut boot = BootScreen::new(note);
    boot.ok(
        "Framebuffer initialized",
        &alloc::format!(
            "{}x{} - {} bpp - {:?}",
            framebuffer_info.width,
            framebuffer_info.height,
            framebuffer_info.bytes_per_pixel,
            framebuffer_info.pixel_format
        ),
    );
    boot.ok(
        "Kernel heap initialized",
        &alloc::format!("{} linked-list heap in .bss", util::bytes(heap.size)),
    );

    let map = memory::init_map(&boot_info.memory_regions);
    boot.ok(
        "Memory map loaded",
        &alloc::format!(
            "{} regions - {} usable",
            map.regions.len(),
            util::bytes(map.usable_bytes)
        ),
    );

    // --- Hardware ----------------------------------------------------------
    keyboard::init();
    timer::init();
    interrupts::init();
    boot.ok(
        "Interrupts initialized",
        "IDT loaded, PIC remapped, PIT at 100 Hz",
    );

    boot.ok(
        "Input subsystem initialized",
        &alloc::format!("PS/2 keyboard on IRQ1 - {} keys", keyboard::keystrokes()),
    );

    let cpu = cpu::info();
    boot.ok(
        "CPU identified",
        &alloc::format!("{} - family {} model {}", cpu.vendor, cpu.family, cpu.model),
    );

    // --- Desktop -----------------------------------------------------------
    let desktop = desktop::Desktop::new(framebuffer_info);
    boot.ok("Desktop environment initialized", "Noor Desktop 0.1.0");
    boot.starting_desktop();
    // Release the boot screen's off-screen background before the desktop
    // allocates its own wallpaper cache (the kernel heap is only 8 MiB).
    drop(boot);
    serial_println!("[desktop] handing over to Noor Desktop");

    desktop::run(desktop)
}
