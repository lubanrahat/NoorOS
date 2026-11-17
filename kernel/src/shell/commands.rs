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
use crate::memory;
use crate::util;
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
    let command = parse(input);
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
        Output::message(command.args)
    } else if command.matches("uptime") {
        uptime()
    } else if command.matches("shutdown") {
        shutdown()
    } else if command.matches("reboot") {
        reboot()
    } else {
        Output::error("Command not found. Type 'help' for available commands.")
    }
}

fn help() -> Output {
    let mut lines = vec![
        String::from("NOOR TERMINAL"),
        String::new(),
        String::from("Available Commands:"),
        String::new(),
    ];
    let commands: [(&str, &str); 9] = [
        ("help", "Show available commands"),
        ("clear", "Clear terminal"),
        ("about", "About NoorOS"),
        ("info", "System information"),
        ("memory", "Memory information"),
        ("version", "Kernel version"),
        ("echo", "Print text"),
        ("uptime", "System uptime"),
        ("shutdown", "Power off the machine (ACPI)"),
    ];
    for (name, description) in commands {
        lines.push(format!("  {name:<10} {description}"));
    }
    lines.push(format!("  {:<10} {}", "reboot", "Restart the machine"));
    lines.push(String::new());
    lines.push(String::from(
        "Noor Shell v0.1.0 - text after echo is printed.",
    ));
    Output::text(lines)
}

fn about() -> Output {
    Output::text(vec![
        String::from("NOOROS 0.1.0"),
        String::from("A Light Beyond Computing."),
        String::new(),
        String::from("NoorOS is a minimal graphical educational OS"),
        String::from("built in Rust to demonstrate operating system"),
        String::from("concepts: kernel development, memory management"),
        String::from("and framebuffer-based desktop rendering."),
        String::new(),
        format!("Kernel:       {KERNEL_NAME}"),
        String::from("Architecture: x86_64"),
        String::from("Language:     Rust (no_std, no_main)"),
        String::from("Project type: Educational operating system"),
    ])
}

fn version() -> Output {
    Output::text(vec![
        format!("NoorOS {VERSION}"),
        format!("{KERNEL_NAME} {VERSION}"),
        String::from("Target: x86_64-unknown-none"),
        String::from("Language: Rust (no_std, no_main)"),
        format!(
            "Heap: {} mapped on demand",
            util::bytes(memory::heap_capacity())
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
