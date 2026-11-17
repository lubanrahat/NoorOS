//! Host-side launcher for NoorOS.
//!
//! This binary never contains any operating system code. It builds the disk
//! images (through `build.rs`), then starts QEMU. On Apple Silicon the host is
//! aarch64, so the x86_64 guest is fully emulated by QEMU's TCG engine.

use std::env;
use std::process::{Command, exit};

const HELP: &str = "\
NoorOS 0.1.0 - A Light Beyond Computing

Usage:
  cargo run --release -- [bios|uefi] [options]
  nooros [bios|uefi] [options]

Options:
  --headless            Do not open a QEMU window (for screenshots/CI)
  --serial <path>       Write serial output to a file instead of the terminal
  --monitor <path>      Expose the QEMU monitor on a UNIX socket
  --allow-reboot        Let a guest reset restart QEMU instead of exiting
  --qemu-arg <arg>      Pass a single extra argument to QEMU (repeatable)
  -h, --help            Show this help

Examples:
  cargo run --release              # BIOS boot, QEMU window, serial on stdout
  cargo run --release -- uefi      # UEFI boot (downloads OVMF on first run)
  cargo run --release -- bios --headless --serial serial.log
";

fn main() {
    let bios_path = env!("BIOS_PATH");
    let uefi_path = env!("UEFI_PATH");

    let args: Vec<String> = env::args().skip(1).collect();
    let mut mode = "bios".to_string();
    let mut headless = false;
    let mut serial: Option<String> = None;
    let mut monitor: Option<String> = None;
    let mut allow_reboot = false;
    let mut extra: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "bios" | "uefi" => mode = args[i].clone(),
            "--headless" => headless = true,
            "--allow-reboot" => allow_reboot = true,
            "--serial" => {
                i += 1;
                serial = Some(args.get(i).expect("--serial needs a path").clone());
            }
            "--monitor" => {
                i += 1;
                monitor = Some(args.get(i).expect("--monitor needs a socket path").clone());
            }
            "--qemu-arg" => {
                i += 1;
                extra.push(args.get(i).expect("--qemu-arg needs a value").clone());
            }
            "-h" | "--help" => {
                print!("{HELP}");
                exit(0);
            }
            other => {
                eprintln!("unknown argument: {other}\n\n{HELP}");
                exit(2);
            }
        }
        i += 1;
    }

    let mut cmd = Command::new("qemu-system-x86_64");
    // x86_64 guest, emulated on the Apple Silicon host.
    cmd.args(["-machine", "pc"]);
    cmd.args(["-m", "512M"]);
    cmd.args(["-smp", "1"]);
    cmd.args(["-vga", "std"]);
    if !allow_reboot {
        cmd.args(["-no-reboot"]);
    }
    // Lets the kernel power the emulator off (ACPI shutdown also works).
    cmd.args(["-device", "isa-debug-exit,iobase=0xf4,iosize=0x04"]);

    if headless {
        cmd.args(["-display", "none"]);
    }

    match &serial {
        Some(path) => {
            cmd.arg("-serial").arg(format!("file:{path}"));
        }
        None => {
            cmd.args(["-serial", "stdio"]);
        }
    }

    match &monitor {
        Some(path) => {
            cmd.arg("-monitor")
                .arg(format!("unix:{path},server,nowait"));
        }
        None => {
            cmd.args(["-monitor", "none"]);
        }
    }

    for arg in &extra {
        cmd.arg(arg);
    }

    if mode == "uefi" {
        // QEMU's bundled OVMF firmware is not guaranteed to exist. Fetch a
        // prebuilt one on first use so `cargo run -- uefi` works everywhere.
        let ovmf_dir = env::current_dir().unwrap().join("target/ovmf");
        match fetch_ovmf(&ovmf_dir) {
            Ok((code, vars)) => {
                cmd.arg("-drive").arg(format!(
                    "if=pflash,format=raw,unit=0,readonly=on,file={}",
                    code.display()
                ));
                cmd.arg("-drive").arg(format!(
                    "if=pflash,format=raw,unit=1,snapshot=on,file={}",
                    vars.display()
                ));
                cmd.arg("-drive")
                    .arg(format!("format=raw,file={uefi_path}"));
            }
            Err(err) => {
                eprintln!("warning: {err}");
                eprintln!("falling back to BIOS boot");
                cmd.arg("-drive")
                    .arg(format!("format=raw,file={bios_path}"));
            }
        }
    } else {
        cmd.arg("-drive")
            .arg(format!("format=raw,file={bios_path}"));
    }

    eprintln!(
        "NoorOS: booting {mode} image in QEMU (guest: x86_64, host: {})",
        env::consts::ARCH
    );
    let status = cmd.status().expect("failed to start qemu-system-x86_64");
    match status.code() {
        Some(0) => {}
        code => {
            eprintln!("NoorOS: QEMU exited with {code:?}");
        }
    }
}

/// Minimal OVMF downloader, used only for `-- uefi` boots.
fn fetch_ovmf(dir: &std::path::Path) -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    let code = dir.join("OVMF_CODE.fd");
    let vars = dir.join("OVMF_VARS.fd");
    if code.exists() && vars.exists() {
        return Ok((code, vars));
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;

    // The prebuilt released by the rust-osdev project. Pinned for repeatability.
    let tag = "edk2-stable202411-r1";
    let url = format!(
        "https://github.com/rust-osdev/ovmf-prebuilt/releases/download/{tag}/{tag}-bin.tar.xz"
    );
    eprintln!("NoorOS: downloading OVMF firmware (first UEFI boot only)...");
    let archive = dir.join("ovmf.tar.xz");
    let status = Command::new("curl")
        .args(["-sSfL", "-o"])
        .arg(&archive)
        .arg(&url)
        .status()
        .map_err(|e| format!("curl failed: {e}"))?;
    if !status.success() {
        return Err("could not download OVMF firmware".into());
    }
    let status = Command::new("tar")
        .args(["-xJf"])
        .arg(&archive)
        .arg("-C")
        .arg(dir)
        .status()
        .map_err(|e| format!("tar failed: {e}"))?;
    if !status.success() {
        return Err("could not extract OVMF firmware".into());
    }
    let _ = std::fs::remove_file(&archive);

    // The archive contains <tag>-bin/<arch>/{code,vars}.fd; QEMU wants the
    // x64 pair under stable names in the cache directory.
    let extracted = dir.join(format!("{tag}-bin"));
    let x64 = extracted.join("x64");
    for (src, dst) in [("code.fd", &code), ("vars.fd", &vars)] {
        std::fs::rename(x64.join(src), dst)
            .map_err(|e| format!("cannot install OVMF {src}: {e}"))?;
    }
    let _ = std::fs::remove_dir_all(&extracted);

    if code.exists() && vars.exists() {
        Ok((code, vars))
    } else {
        Err("OVMF archive layout was not what NoorOS expects".into())
    }
}
