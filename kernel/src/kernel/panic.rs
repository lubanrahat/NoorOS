//! Kernel panic and CPU exception reporting.
//!
//! Both paths write to the serial console and, when the framebuffer is usable,
//! paint a readable error screen. Nothing here allocates: a panic may have been
//! caused by the allocator itself.

use core::fmt::{self, Write};
use core::panic::PanicInfo;

use x86_64::structures::idt::InterruptStackFrame;

use crate::drivers::framebuffer;
use crate::graphics::branding::radial_glow;
use crate::graphics::colors::{self, Color};
use crate::graphics::primitives::*;
use crate::graphics::text::*;

/// A fixed-capacity string so the panic path never allocates.
struct FixedText<const N: usize> {
    buffer: [u8; N],
    len: usize,
}

impl<const N: usize> FixedText<N> {
    fn new() -> Self {
        Self {
            buffer: [0; N],
            len: 0,
        }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buffer[..self.len]).unwrap_or("<invalid utf-8>")
    }
}

impl<const N: usize> fmt::Write for FixedText<N> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            if self.len + 1 >= N {
                break;
            }
            // Keep the panic screen single-line and printable.
            self.buffer[self.len] = if byte == b'\n' || byte < 0x20 {
                b' '
            } else {
                byte
            };
            self.len += 1;
        }
        Ok(())
    }
}

pub fn halt() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}

/// Shared screen for exceptions and panics.
fn draw_fatal(
    canvas: &mut crate::graphics::Canvas,
    title: &str,
    detail: &str,
    location: Option<&str>,
) {
    let w = canvas.width();
    let h = canvas.height();
    canvas.clear(colors::BG);
    v_gradient(canvas, 0, 0, w, h / 3, Color::hex(0x1A0B12), colors::BG);
    radial_glow(canvas, w / 2, 0, h / 2, colors::DANGER, 24);

    let cx = w / 2;
    circle_ring(canvas, cx, 110, 34, 3, colors::DANGER);
    line(canvas, cx - 14, 110, cx + 14, 110, colors::DANGER);
    line(canvas, cx, 96, cx, 118, colors::DANGER);

    draw_text_centered(canvas, cx, 170, "NOOR KERNEL", 3, colors::TEXT);
    draw_text_centered(canvas, cx, 200, title, 2, colors::DANGER);

    let panel_rect = crate::graphics::Rect::new(cx - 320, 250, 640, 170);
    panel(
        canvas,
        panel_rect,
        colors::SURFACE,
        235,
        10,
        colors::BORDER,
        220,
    );
    let mut y = 270;
    for line_text in wrap(detail, 70, 4) {
        draw_text(canvas, cx - 296, y, line_text, 1, colors::TEXT);
        y += 18;
    }
    if let Some(location) = location {
        let mut y = 380;
        for line_text in wrap(location, 70, 3) {
            draw_text(canvas, cx - 296, y, line_text, 1, colors::TEXT_DIM);
            y += 16;
        }
    }

    draw_text_centered(
        canvas,
        cx,
        h - 96,
        "The system has halted.",
        2,
        colors::TEXT,
    );
    draw_text_centered(
        canvas,
        cx,
        h - 62,
        "Diagnostics were written to COM1 - see the serial console.",
        1,
        colors::TEXT_DIM,
    );
}

/// Called by CPU exception handlers; never returns.
pub fn exception(title: &str, detail: &str, frame: &InterruptStackFrame) -> ! {
    serial_println!("\n!!!!!! NOOR KERNEL EXCEPTION: {title} !!!!!!");
    serial_println!("{detail}");
    serial_println!("{frame:#?}");
    let mut location = FixedText::<160>::new();
    let _ = write!(
        location,
        "at {:#X} (RIP)",
        frame.instruction_pointer.as_u64()
    );
    framebuffer::with_try(|canvas| draw_fatal(canvas, title, detail, Some(location.as_str())));
    halt()
}

/// Rust panic entry point.
#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
    serial_println!("\n!!!!!! NOOR KERNEL PANIC !!!!!!");
    if let Some(location) = info.location() {
        serial_println!(
            "at {}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        );
    }
    serial_println!("{info}");

    let mut message = FixedText::<200>::new();
    let mut location = FixedText::<160>::new();
    if let Some(location_info) = info.location() {
        let _ = write!(
            location,
            "{}:{}:{}",
            location_info.file(),
            location_info.line(),
            location_info.column()
        );
    }
    let _ = write!(message, "{info}");

    framebuffer::with_try(|canvas| {
        draw_fatal(canvas, "PANIC", message.as_str(), Some(location.as_str()))
    });
    halt()
}
