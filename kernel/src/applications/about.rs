//! About NoorOS - branded project information.

use alloc::string::String;

use crate::applications::AppAction;
use crate::applications::ui;
use crate::drivers::keyboard::InputEvent;
use crate::drivers::timer;
use crate::graphics::branding;
use crate::graphics::colors::*;
use crate::graphics::primitives::fill_rect_alpha;
use crate::graphics::text::{draw_text, draw_text_centered};
use crate::graphics::{Canvas, Rect};
use crate::{memory, util};

pub struct About;

impl About {
    pub fn new() -> Self {
        Self
    }

    pub fn handle_key(&mut self, _event: InputEvent) -> AppAction {
        AppAction::None
    }

    pub fn render(&mut self, canvas: &mut Canvas, rect: Rect, _tick: u64) {
        ui::background(canvas, rect);

        // Brand header
        let cx = rect.center_x();
        let header_h = 150;
        for y in 0..header_h {
            let color = Color::hex(0x0F1728).lerp(BG, y as u32, header_h as u32);
            canvas.fast_hline(rect.x, rect.y + y, rect.w, color);
        }
        branding::draw_logo(canvas, cx, rect.y + 44, 26, GOLD);
        branding::draw_wordmark(canvas, cx, rect.y + 80, 4, TEXT);
        draw_text_centered(
            canvas,
            cx,
            rect.y + 116,
            "A Light Beyond Computing.",
            1,
            TEXT_DIM,
        );
        fill_rect_alpha(canvas, rect.x, rect.y + header_h, rect.w, 1, GOLD, 90);

        let x = rect.x + 30;
        let w = rect.w - 60;
        let mut y = rect.y + header_h + 18;

        let rows = [
            ("Version", String::from(crate::shell::commands::VERSION)),
            ("Kernel", String::from(crate::shell::commands::KERNEL_NAME)),
            ("Architecture", String::from("x86_64")),
            ("Language", String::from("Rust (no_std, no_main)")),
            ("Project type", String::from("Educational operating system")),
        ];
        for (key, value) in rows {
            ui::kv(canvas, x, y, w, key, &value);
            y += 19;
        }

        y += 8;
        ui::divider(canvas, x, y, w);
        y += 14;

        y = ui::paragraph(
            canvas,
            x,
            y,
            w,
            "NoorOS is a minimal graphical educational operating system built using Rust to \
             demonstrate fundamental operating system concepts, kernel development, memory \
             management and framebuffer-based desktop rendering.",
            1,
            TEXT_DIM,
            4,
        );

        let heap = memory::heap_stats();
        draw_text(
            canvas,
            x,
            y + 4,
            &alloc::format!(
                "Booted {} ago - {} usable RAM - heap {}",
                util::uptime(timer::uptime_ms()),
                memory::map()
                    .map(|m| util::bytes(m.usable_bytes))
                    .unwrap_or_else(|| String::from("?")),
                util::bytes(heap.size)
            ),
            1,
            Color::hex(0x4A5A75),
        );

        // Stack chips
        let chips = [
            "bootloader_api",
            "x86_64",
            "pic8259",
            "pc-keyboard",
            "font8x8",
        ];
        let mut cx2 = x;
        let chip_y = y + 26;
        for chip in chips {
            let width = crate::graphics::text::text_width(chip, 1) + 16;
            if cx2 + width > x + w {
                break;
            }
            ui::pill(canvas, cx2, chip_y, chip, TEXT_DIM, SURFACE_2);
            cx2 += width + 8;
        }

        draw_text_centered(
            canvas,
            cx,
            rect.bottom() - 18,
            "NoorOS 0.1.0 - MIT OR Apache-2.0",
            1,
            Color::hex(0x4A5A75),
        );
    }
}
