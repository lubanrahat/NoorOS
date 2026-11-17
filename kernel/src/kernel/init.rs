//! Kernel initialisation sequence.
//!
//! Order matters:
//!   1. serial first, so every later failure is visible on the host,
//!   2. framebuffer next, so failures can also be shown to the user,
//!   3. frame allocator, active page tables and heap, then the memory-map
//!      snapshot (the snapshot allocates),
//!   4. keyboard/mouse devices, PIT and finally the IDT/PIC (interrupts on),
//!   5. desktop.

use bootloader_api::BootInfo;

use crate::boot::BootScreen;
use crate::desktop;
use crate::drivers::{cpu, framebuffer, keyboard, mouse, serial, timer};
use crate::interrupts;
use crate::{fs, memory, tasking, util};

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
    // The frame allocator and page tables must exist before anything allocates:
    // the boot screen itself is built from `String`s, so every earlier step has
    // to be allocation-free.
    if let Err(error) = memory::init(boot_info) {
        serial_println!("[fatal] memory initialisation failed: {error}");
        crate::kernel::panic::halt();
    }
    let frames = memory::frame_stats();
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
        "Virtual memory online",
        &alloc::format!("{} frames free", frames.free),
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

    // --- Tasks and filesystem ----------------------------------------------
    // Both allocate, so they run after the heap and memory-map snapshot. The
    // scheduler must exist before IRQ0 is unmasked below.
    if let Err(error) = tasking::init() {
        serial_println!("[fatal] scheduler initialisation failed: {error}");
        crate::kernel::panic::halt();
    }
    let scheduler = tasking::stats();
    serial_println!(
        "[kernel] scheduler: round-robin, {} tasks, {} ms quantum, {} Hz",
        scheduler.tasks,
        scheduler.quantum_ms,
        timer::TICK_HZ
    );
    boot.ok(
        "Scheduler online",
        &alloc::format!(
            "{} tasks - {} ms quantum",
            scheduler.tasks,
            scheduler.quantum_ms
        ),
    );

    if let Err(error) = fs::init() {
        serial_println!("[fatal] MemFS initialisation failed: {}", error.message());
        crate::kernel::panic::halt();
    }
    if let Err(error) = fs::self_test() {
        serial_println!("[fatal] {error}");
        crate::kernel::panic::halt();
    }
    let filesystem = fs::stats();
    serial_println!(
        "[kernel] MemFS: {} files, {} directories, {} in RAM",
        filesystem.files,
        filesystem.directories,
        util::bytes(filesystem.bytes)
    );
    boot.ok(
        "MemFS mounted",
        &alloc::format!(
            "{} files - {} dirs",
            filesystem.files,
            filesystem.directories
        ),
    );

    // --- Hardware ----------------------------------------------------------
    // The mouse is programmed before the IDT is loaded: its configuration
    // enables the auxiliary interrupt in the 8042, and IRQ12 must not become
    // reachable before the handler and the PIC masks are in place.
    keyboard::init();
    mouse::init(
        framebuffer_info.width as i32,
        framebuffer_info.height as i32,
    );
    timer::init();
    interrupts::init();
    boot.ok(
        "Interrupts initialized",
        "IDT loaded, PIC remapped, PIT at 100 Hz",
    );

    boot.ok(
        "Input subsystem initialized",
        if mouse::present() {
            "PS/2 keyboard IRQ1, mouse IRQ12"
        } else {
            "PS/2 keyboard IRQ1, no mouse found"
        },
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
    // allocates its own wallpaper cache. The heap would grow on demand anyway,
    // but freeing first avoids mapping pages that are no longer needed.
    drop(boot);
    serial_println!("[desktop] handing over to Noor Desktop");

    desktop::run(desktop)
}
