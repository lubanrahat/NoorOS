//! Background tasks used to demonstrate preemptive scheduling.
//!
//! Both tasks only touch atomics. They never allocate, log or take a lock, so
//! they can be preempted at any instruction without risking a lock that the
//! interrupted task was holding. Their counters are visible in Noor Monitor
//! and in the terminal's `ps` command.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::drivers::timer;

/// PIT ticks observed by the uptime task (shared memory updated from a task).
pub static UPTIME_TICKS: AtomicU64 = AtomicU64::new(0);
/// Prime numbers found by the calculation task since boot.
pub static PRIMES_FOUND: AtomicU64 = AtomicU64::new(0);

/// Wakes every PIT tick and advances the shared uptime counter.
///
/// The task spins rather than sleeping so the scheduler has real work to
/// preempt; it still yields the CPU at the end of every quantum.
pub extern "C" fn uptime_task() {
    let mut last = timer::ticks();
    loop {
        let now = timer::ticks();
        if now != last {
            UPTIME_TICKS.fetch_add(now - last, Ordering::Relaxed);
            last = now;
        }
        core::hint::spin_loop();
    }
}

/// Continuously counts primes below 50,000, then starts again.
pub extern "C" fn prime_task() {
    let mut candidate = 2u64;
    loop {
        if is_prime(candidate) {
            PRIMES_FOUND.fetch_add(1, Ordering::Relaxed);
        }
        candidate += 1;
        if candidate > 50_000 {
            candidate = 2;
        }
    }
}

fn is_prime(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    if n % 2 == 0 {
        return n == 2;
    }
    let mut divisor = 3;
    while divisor * divisor <= n {
        if n.is_multiple_of(divisor) {
            return false;
        }
        divisor += 2;
    }
    true
}

#[derive(Clone, Copy)]
pub struct DemoStats {
    pub uptime_ticks: u64,
    pub primes: u64,
}

pub fn stats() -> DemoStats {
    DemoStats {
        uptime_ticks: UPTIME_TICKS.load(Ordering::Relaxed),
        primes: PRIMES_FOUND.load(Ordering::Relaxed),
    }
}
