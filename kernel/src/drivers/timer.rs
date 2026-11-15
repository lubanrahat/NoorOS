//! Programmable Interval Timer (PIT) driver.
//!
//! Channel 0 is programmed to 100 Hz. The IRQ handler (see `interrupts`) calls
//! [`on_interrupt`]; the rest of the kernel reads a monotonic tick counter. The
//! tick count is the basis of `uptime` in Noor Terminal and Noor Monitor.

use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::instructions::port::Port;

/// PIT frequency in Hz.
pub const TICK_HZ: u64 = 100;
const PIT_BASE_FREQUENCY: u32 = 1_193_182;

static TICKS: AtomicU64 = AtomicU64::new(0);
static INTERRUPTS: AtomicU64 = AtomicU64::new(0);

/// Programs PIT channel 0 in rate-generator mode.
pub fn init() {
    let divisor = (PIT_BASE_FREQUENCY / TICK_HZ as u32) as u16;
    // SAFETY: standard PIT I/O ports; no memory is touched.
    unsafe {
        Port::<u8>::new(0x43).write(0x36); // channel 0, lobyte/hibyte, mode 3
        Port::<u8>::new(0x40).write((divisor & 0xff) as u8);
        Port::<u8>::new(0x40).write((divisor >> 8) as u8);
    }
}

/// Called from the timer interrupt handler.
pub fn on_interrupt() {
    TICKS.fetch_add(1, Ordering::Relaxed);
    INTERRUPTS.fetch_add(1, Ordering::Relaxed);
}

pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

pub fn uptime_ms() -> u64 {
    ticks() * 1000 / TICK_HZ
}

pub fn interrupt_count() -> u64 {
    INTERRUPTS.load(Ordering::Relaxed)
}

/// Busy-waits using `hlt`, which also lets QEMU idle. Interrupts must be on.
pub fn sleep_ms(ms: u64) {
    let target = ticks() + (ms * TICK_HZ / 1000).max(1);
    while ticks() < target {
        x86_64::instructions::hlt();
    }
}
