//! The Noor Shell command interpreter.
//!
//! All output is produced from values the kernel really has: the bootloader
//! memory map, the heap allocator counters, CPUID, the PIT tick counter and the
//! CMOS real-time clock. Nothing here is simulated or randomised.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::drivers::{cpu, framebuffer, keyboard, rtc, timer};
use crate::memory;
use crate::util;

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
            "Heap: {} linked-list allocator in .bss",
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
    // Honest limitation: the kernel has no paging of its own.
    lines.push(format!(
        "  {:<15} not implemented (no paging)",
        "Virtual memory"
    ));
    lines.push(format!(
        "  {:<15} {} timer, {} keys",
        "Interrupts",
        timer::interrupt_count(),
        keyboard::interrupt_count()
    ));
    Output::text(lines)
}

fn memory_info() -> Output {
    let mut lines = Vec::new();
    let Some(map) = memory::map() else {
        return Output::message("Memory map is not available.");
    };
    // Keep this within the terminal's 18-row viewport: heading, table header,
    // one row per region and two footer lines. The full picture is in Monitor.
    lines.push(format!(
        "PHYSICAL MEMORY  -  {} total, {} usable",
        util::bytes(map.total_bytes),
        util::bytes(map.usable_bytes)
    ));
    lines.push(String::new());
    lines.push(format!(
        "  {:<12} {:<12} {:>7}  TYPE",
        "START", "END", "SIZE"
    ));
    for region in map.regions.iter().take(12) {
        lines.push(format!(
            "  {:012X} {:012X} {:>7}  {}",
            region.start,
            region.start + region.len,
            util::bytes(region.len),
            region.kind.detail()
        ));
    }
    if map.regions.len() > 12 {
        lines.push(format!("  ... {} more region(s)", map.regions.len() - 12));
    }
    lines.push(String::new());
    let heap = memory::heap_stats();
    lines.push(format!(
        "  Heap {} total, {} used - linked_list_allocator",
        util::bytes(heap.size),
        util::bytes(heap.used)
    ));
    lines.push(String::from("  Paging: not implemented in this version."));
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
