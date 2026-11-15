//! Build script for the host side of NoorOS.
//!
//! Cargo compiles `kernel/` for `x86_64-unknown-none` (see the artifact
//! dependency in `Cargo.toml`) and exposes the resulting ELF through
//! `CARGO_BIN_FILE_KERNEL_kernel`. This script combines that ELF with the
//! `bootloader` crate to produce bootable BIOS and UEFI disk images.

use std::path::PathBuf;

/// Cargo only notices new or removed entries when a directory itself is
/// registered, so every file inside is registered individually. Without this,
/// editing an existing kernel source file would reuse a stale disk image.
fn watch_tree(dir: &std::path::Path) {
    println!("cargo:rerun-if-changed={}", dir.display());
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            watch_tree(&path);
        } else {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}

fn main() {
    watch_tree(std::path::Path::new("kernel/src"));
    println!("cargo:rerun-if-changed=kernel/Cargo.toml");

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR not set"));
    let kernel = PathBuf::from(
        std::env::var_os("CARGO_BIN_FILE_KERNEL_kernel").expect("kernel artifact not found"),
    );

    // Ask the bootloader for a desktop-sized framebuffer. The bootloader will
    // fall back to the closest mode the firmware supports. We never assume that
    // the request succeeded: the kernel reads the real mode from BootInfo.
    let mut config = bootloader::BootConfig::default();
    config.frame_buffer.minimum_framebuffer_width = Some(1280);
    config.frame_buffer.minimum_framebuffer_height = Some(800);
    // Keep the firmware hand-off screen clean; the kernel paints its own.
    config.frame_buffer_logging = false;
    config.serial_logging = true;

    let bios_path = out_dir.join("nooros-bios.img");
    bootloader::BiosBoot::new(&kernel)
        .set_boot_config(&config)
        .create_disk_image(&bios_path)
        .expect("failed to create BIOS disk image");

    let uefi_path = out_dir.join("nooros-uefi.img");
    bootloader::UefiBoot::new(&kernel)
        .set_boot_config(&config)
        .create_disk_image(&uefi_path)
        .expect("failed to create UEFI disk image");

    println!("cargo:rustc-env=BIOS_PATH={}", bios_path.display());
    println!("cargo:rustc-env=UEFI_PATH={}", uefi_path.display());
}
