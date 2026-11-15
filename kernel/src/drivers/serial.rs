//! Serial output on COM1 (16550 UART).
//!
//! This is the kernel's debugging channel. QEMU exposes it on stdio or to a file
//! (see the runner's `--serial` option).

use core::fmt;
use spin::Mutex;
use uart_16550::backend::PioBackend;
use uart_16550::{Config, Uart16550Tty};

static SERIAL: Mutex<Option<Uart16550Tty<PioBackend>>> = Mutex::new(None);

/// Initialises COM1 at 115200 baud, 8N1.
pub fn init() {
    // SAFETY: 0x3F8 is the standard COM1 base port on x86 PCs.
    match unsafe { Uart16550Tty::new_port(0x3F8, Config::default()) } {
        Ok(tty) => {
            *SERIAL.lock() = Some(tty);
        }
        Err(_) => {
            // No UART: boot continues, only serial logging is lost.
        }
    }
}

pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    if let Some(port) = SERIAL.lock().as_mut() {
        let _ = port.write_fmt(args);
    }
}
