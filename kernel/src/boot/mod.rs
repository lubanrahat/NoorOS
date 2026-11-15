//! NoorOS boot screen.
//!
//! Every line on this screen is printed *after* the corresponding subsystem has
//! actually finished initialising. The screen is redrawn after each step, so the
//! boot animation is a direct visualisation of the kernel's real init sequence.

use alloc::string::String;
use alloc::vec::Vec;

use crate::drivers::framebuffer;
use crate::graphics::branding::{self, radial_glow};
use crate::graphics::colors::{self, Color};
use crate::graphics::primitives::*;
use crate::graphics::text::*;
use crate::graphics::{Canvas, Rect};
use crate::util;
const MAX_STEPS: usize = 8;

struct Step {
    label: String,
    detail: String,
}

pub struct BootScreen {
    steps: Vec<Step>,
    framebuffer_note: String,
    background: Vec<u8>,
}

impl BootScreen {
    pub fn new(framebuffer_note: String) -> Self {
        let mut screen = Self {
            steps: Vec::new(),
            framebuffer_note,
            background: Vec::new(),
        };
        // The wallpaper (gradient + glows + brand mark) is the expensive part,
        // and it never changes during boot. Render it once into an off-screen
        // buffer, then blit + overlay text for every step.
        if let Some(info) = framebuffer::info() {
            let mut background = alloc::vec![0u8; info.byte_len];
            {
                let mut canvas = Canvas::new(&mut background, info);
                draw_background(&mut canvas);
            }
            screen.background = background;
        }
        screen.push("Bootloader hand-off", "bootinfo + framebuffer received");
        screen.render("Initializing Noor Kernel...");
        screen
    }

    fn push(&mut self, label: &str, detail: &str) {
        if self.steps.len() < MAX_STEPS {
            self.steps.push(Step {
                label: String::from(label),
                detail: String::from(detail),
            });
        }
    }

    /// Records a completed initialisation step and repaints the screen.
    pub fn ok(&mut self, label: &str, detail: &str) {
        self.push(label, detail);
        serial_println!("[ OK ] {label} - {detail}");
        self.render("Initializing Noor Kernel...");
    }

    /// Final screen before the desktop takes over.
    pub fn starting_desktop(&mut self) {
        self.render("Starting Noor Desktop...");
        crate::drivers::timer::sleep_ms(420);
    }

    fn render(&self, status: &str) {
        let note = self.framebuffer_note.clone();
        let _ = framebuffer::with(|canvas| {
            if !self.background.is_empty() {
                canvas.blit(&self.background);
            } else {
                draw_background(canvas);
            }
            draw_foreground(canvas, &self.steps, status, &note);
        });
    }
}

fn draw_background(canvas: &mut Canvas) {
    let w = canvas.width();
    let h = canvas.height();
    let cx = w / 2;

    canvas.clear(colors::BG);
    // Deep midnight ambience with a warm Noor glow above the mark.
    v_gradient(
        canvas,
        0,
        0,
        w,
        h,
        Color::hex(0x0C1424),
        Color::hex(0x05070C),
    );
    radial_glow(canvas, cx, h / 5, h / 2, colors::GOLD, 22);
    radial_glow(canvas, w / 5, h, h / 2, colors::BLUE, 12);

    let logo_y = h * 21 / 100;
    branding::draw_logo(canvas, cx, logo_y, (h / 17).max(26), colors::GOLD);
    branding::draw_wordmark(canvas, cx, logo_y + h / 13, 6, colors::TEXT);
    branding::draw_tagline(canvas, cx, logo_y + h / 13 + 52, 2);

    // Divider
    let div_w = (w / 4).clamp(120, 320);
    fill_rect(
        canvas,
        cx - div_w / 2,
        logo_y + h / 13 + 86,
        div_w,
        1,
        colors::BORDER,
    );
    fill_rect(canvas, cx - 18, logo_y + h / 13 + 86, 36, 1, colors::GOLD);

    // Step panel
    let panel_w = (w * 62 / 100).clamp(560, 820);
    let rows = MAX_STEPS as i32;
    let panel_h = 52 + rows * 25;
    let panel_rect = Rect::new(cx - panel_w / 2, logo_y + h / 13 + 112, panel_w, panel_h);
    panel(
        canvas,
        panel_rect,
        colors::SURFACE,
        170,
        12,
        colors::BORDER,
        200,
    );
}

fn draw_foreground(canvas: &mut Canvas, steps: &[Step], status: &str, framebuffer_note: &str) {
    let w = canvas.width();
    let h = canvas.height();
    let cx = w / 2;
    let logo_y = h * 21 / 100;
    let panel_w = (w * 62 / 100).clamp(560, 820);
    let rows = MAX_STEPS as i32;
    let panel_h = 52 + rows * 25;
    let panel_rect = Rect::new(cx - panel_w / 2, logo_y + h / 13 + 112, panel_w, panel_h);

    let mut y = panel_rect.y + 18;
    let label_x = panel_rect.x + 26;
    let detail_x = panel_rect.x + 338;
    for step in steps {
        draw_text(canvas, label_x, y, "[ OK ]", 1, colors::SUCCESS);
        draw_text(canvas, label_x + 58, y, &step.label, 1, colors::TEXT);
        draw_text(canvas, detail_x, y, &step.detail, 1, colors::TEXT_DIM);
        y += 25;
    }
    // Pending slots keep the panel from jumping around while booting.
    for _ in steps.len()..MAX_STEPS {
        draw_text(canvas, label_x, y, "[ .. ]", 1, Color::hex(0x3B4A63));
        y += 25;
    }

    // Status line + progress bar
    let status_y = panel_rect.bottom() + 26;
    draw_text_centered(canvas, cx, status_y, status, 1, colors::TEXT_DIM);

    let bar_w = panel_w - 120;
    let bar_y = status_y + 22;
    fill_rounded_rect(canvas, cx - bar_w / 2, bar_y, bar_w, 4, 2, colors::BORDER);
    let progress = (steps.len() as i32 * bar_w / MAX_STEPS as i32).max(6);
    fill_rounded_rect(canvas, cx - bar_w / 2, bar_y, progress, 4, 2, colors::GOLD);
    circle_filled(
        canvas,
        cx - bar_w / 2 + progress,
        bar_y + 2,
        4,
        colors::GOLD_LIGHT,
    );

    // Footer
    draw_text(canvas, 28, h - 30, "Noor Kernel 0.1.0", 1, colors::TEXT_DIM);
    draw_text(
        canvas,
        28,
        h - 16,
        "A Light Beyond Computing.",
        1,
        Color::hex(0x4A5A75),
    );
    let right = alloc::format!("x86_64 emulated - {framebuffer_note}");
    draw_text_right(canvas, w - 28, h - 30, &right, 1, colors::TEXT_DIM);
    draw_text_right(
        canvas,
        w - 28,
        h - 16,
        &util::uptime(crate::drivers::timer::uptime_ms()),
        1,
        Color::hex(0x4A5A75),
    );
}
