//! Noor Settings - informational settings and display diagnostics.
//!
//! Version 0.1.0 is deliberately read-only: it reports the system's real
//! configuration instead of offering switches that do nothing.

use alloc::format;
use alloc::string::{String, ToString};

use crate::applications::AppAction;
use crate::applications::ui;
use crate::drivers::keyboard::{InputEvent, Key};
use crate::drivers::{cpu, framebuffer, timer};
use crate::graphics::colors::*;
use crate::graphics::primitives::{fill_rect, fill_rect_alpha, fill_rounded_rect};
use crate::graphics::text::{draw_text, text_width};
use crate::graphics::{Canvas, Rect};
use crate::{memory, util};

const PAD: i32 = 16;

const SECTIONS: [&str; 5] = ["General", "Appearance", "Display", "System", "About NoorOS"];

pub struct Settings {
    selected: usize,
}

impl Settings {
    pub fn new() -> Self {
        Self { selected: 0 }
    }

    pub fn handle_key(&mut self, event: InputEvent) -> AppAction {
        match event.key {
            Key::Up => self.selected = self.selected.saturating_sub(1),
            Key::Down => self.selected = (self.selected + 1).min(SECTIONS.len() - 1),
            _ => {}
        }
        AppAction::None
    }

    pub fn render(&mut self, canvas: &mut Canvas, rect: Rect, _tick: u64) {
        ui::background(canvas, rect);
        let content = Rect::new(
            rect.x + PAD,
            rect.y + PAD,
            rect.w - PAD * 2,
            rect.h - PAD * 2,
        );

        // --- Navigation ----------------------------------------------------
        let nav_w = 180;
        let nav_rect = Rect::new(content.x, content.y, nav_w, content.h);
        ui::card(canvas, nav_rect, "SETTINGS", GOLD);
        let mut y = nav_rect.y + 46;
        for (index, name) in SECTIONS.iter().enumerate() {
            let item = Rect::new(nav_rect.x + 10, y, nav_w - 20, 28);
            let selected = index == self.selected;
            if selected {
                fill_rounded_rect(canvas, item.x, item.y, item.w, item.h, 7, SURFACE_2);
                fill_rect(canvas, item.x, item.y + 5, 2, item.h - 10, GOLD);
            }
            draw_text(
                canvas,
                item.x + 12,
                item.y + 9,
                name,
                1,
                if selected { TEXT } else { TEXT_DIM },
            );
            y += 32;
        }
        draw_text(
            canvas,
            nav_rect.x + 10,
            nav_rect.bottom() - 22,
            "Up/Down to browse",
            1,
            Color::hex(0x4A5A75),
        );

        // --- Page ----------------------------------------------------------
        let page = Rect::new(
            content.x + nav_w + 16,
            content.y,
            content.w - nav_w - 16,
            content.h,
        );
        fill_rounded_rect(canvas, page.x, page.y, page.w, page.h, 10, BG);
        stroke_rounded_rect_border(canvas, page);

        let inner = Rect::new(page.x + 22, page.y + 20, page.w - 44, page.h - 40);
        match self.selected {
            0 => general(canvas, inner),
            1 => appearance(canvas, inner),
            2 => display(canvas, inner),
            3 => system(canvas, inner),
            _ => about(canvas, inner),
        }

        fill_rect_alpha(canvas, rect.x, rect.y + rect.h - 2, rect.w, 2, VIOLET, 80);
    }
}

fn stroke_rounded_rect_border(canvas: &mut Canvas, page: Rect) {
    crate::graphics::primitives::stroke_rounded_rect(
        canvas, page.x, page.y, page.w, page.h, 10, 1, BORDER,
    );
}

fn general(canvas: &mut Canvas, area: Rect) {
    ui::heading(canvas, area.x, area.y, "General");
    let mut y = area.y + 46;
    for (key, value) in [
        ("Operating system", String::from("NoorOS")),
        ("Version", String::from(crate::shell::commands::VERSION)),
        ("Kernel", String::from(crate::shell::commands::KERNEL_NAME)),
        ("Architecture", String::from("x86_64")),
        ("Project type", String::from("Educational operating system")),
        ("Language", String::from("Rust (no_std, no_main)")),
        ("Build target", String::from("x86_64-unknown-none")),
    ] {
        ui::kv(canvas, area.x, y, area.w, key, &value);
        y += 20;
    }
    ui::divider(canvas, area.x, y + 6, area.w);
    ui::paragraph(
        canvas,
        area.x,
        y + 20,
        area.w,
        "Noor Settings is informational in this release. Every value is read from the kernel; \
         there are no runtime switches yet, so nothing here can silently do nothing.",
        1,
        TEXT_DIM,
        3,
    );
}

fn appearance(canvas: &mut Canvas, area: Rect) {
    ui::heading(canvas, area.x, area.y, "Appearance");
    let mut y = area.y + 46;
    ui::kv(canvas, area.x, y, area.w, "Theme", "Noor Dark (built in)");
    y += 20;
    ui::kv(canvas, area.x, y, area.w, "Primary accent", "Noor Gold");
    y += 20;
    ui::kv(
        canvas,
        area.x,
        y,
        area.w,
        "Typography",
        "Embedded 8x8 bitmap font",
    );
    y += 20;
    ui::kv(
        canvas,
        area.x,
        y,
        area.w,
        "Window corners",
        "Rounded, 12 px radius",
    );
    y += 28;
    ui::divider(canvas, area.x, y, area.w);
    y += 14;

    let swatches = [
        ("Noor Gold", GOLD),
        ("Electric Blue", BLUE),
        ("Violet", VIOLET),
        ("Success", SUCCESS),
        ("Surface", SURFACE_2),
        ("Text", TEXT),
    ];
    let col_w = area.w / 2;
    for (index, (name, color)) in swatches.iter().enumerate() {
        let col = (index % 2) as i32;
        let row = (index / 2) as i32;
        ui::swatch(canvas, area.x + col * col_w, y + row * 38, *color, name);
    }
    ui::paragraph(
        canvas,
        area.x,
        y + 3 * 38 + 6,
        area.w,
        "Transparency is real per-pixel alpha blending into the framebuffer. There is no \
         offscreen compositor and no GPU acceleration.",
        1,
        TEXT_DIM,
        3,
    );
}

fn display(canvas: &mut Canvas, area: Rect) {
    ui::heading(canvas, area.x, area.y, "Display");
    let mut y = area.y + 46;
    if let Some(info) = framebuffer::info() {
        for (key, value) in [
            ("Resolution", format!("{} x {} px", info.width, info.height)),
            ("Bytes per pixel", format!("{}", info.bytes_per_pixel)),
            ("Pixel format", format!("{:?}", info.pixel_format)),
            ("Stride", format!("{} px", info.stride)),
            ("Framebuffer size", util::bytes(info.byte_len as u64)),
            (
                "Renderer",
                String::from("Noor graphics engine (immediate mode)"),
            ),
        ] {
            ui::kv(canvas, area.x, y, area.w, key, &value);
            y += 20;
        }
        y += 10;
        ui::divider(canvas, area.x, y, area.w);
        ui::paragraph(
            canvas,
            area.x,
            y + 16,
            area.w,
            "The kernel writes every pixel itself, row by row, into the linear framebuffer \
             supplied by the bootloader. No display server, GPU driver or compositor is used.",
            1,
            TEXT_DIM,
            3,
        );
    } else {
        draw_text(
            canvas,
            area.x,
            y,
            "No framebuffer information available.",
            1,
            DANGER,
        );
    }
}

fn system(canvas: &mut Canvas, area: Rect) {
    ui::heading(canvas, area.x, area.y, "System");
    let mut y = area.y + 46;
    let heap = memory::heap_stats();
    let mut rows = alloc::vec![
        ("Kernel heap", format!("{} total", util::bytes(heap.size))),
        ("Heap in use", util::bytes(heap.used).to_string()),
        ("Heap free", util::bytes(heap.free).to_string()),
        ("Uptime", util::uptime(timer::uptime_ms())),
        ("PIT frequency", format!("{} Hz", timer::TICK_HZ)),
    ];
    if let Some(map) = memory::map() {
        rows.insert(0, ("Usable memory", util::bytes(map.usable_bytes)));
        rows.insert(1, ("Reserved memory", util::bytes(map.reserved_bytes)));
        rows.insert(2, ("Map regions", format!("{}", map.regions.len())));
    }
    let cpu = cpu::info();
    rows.push(("Processor", cpu.brand.clone()));
    rows.push(("Logical CPUs", format!("{}", cpu.logical_cpus)));
    for (key, value) in rows {
        ui::kv(canvas, area.x, y, area.w, key, &value);
        y += 19;
    }
    ui::paragraph(
        canvas,
        area.x,
        y + 10,
        area.w,
        "Virtual memory is not implemented: the kernel runs on the mappings created by the \
         bootloader and uses a static heap for dynamic allocation.",
        1,
        TEXT_DIM,
        3,
    );
}

fn about(canvas: &mut Canvas, area: Rect) {
    ui::heading(canvas, area.x, area.y, "About NoorOS");
    let mut y = area.y + 56;
    crate::graphics::branding::draw_logo(canvas, area.x + 26, y + 10, 26, GOLD);
    draw_text(canvas, area.x + 66, y - 2, "NOOROS", 3, TEXT);
    draw_text(
        canvas,
        area.x + 66,
        y + 26,
        "A Light Beyond Computing.",
        1,
        TEXT_DIM,
    );
    y += 70;
    ui::divider(canvas, area.x, y, area.w);
    y += 16;
    for (key, value) in [
        ("Version", String::from(crate::shell::commands::VERSION)),
        ("Kernel", String::from(crate::shell::commands::KERNEL_NAME)),
        ("Language", String::from("Rust")),
        ("Project", String::from("Educational operating system")),
    ] {
        ui::kv(canvas, area.x, y, area.w, key, &value);
        y += 20;
    }
    ui::paragraph(
        canvas,
        area.x,
        y + 14,
        area.w,
        "NoorOS is a minimal graphical educational operating system built using Rust to \
         demonstrate fundamental operating system concepts, kernel development, memory \
         management and framebuffer-based desktop rendering.",
        1,
        TEXT_DIM,
        4,
    );
    draw_text(
        canvas,
        area.x,
        area.bottom() - 12,
        "NoorOS 0.1.0 - built with Rust, no_std, bare metal",
        1,
        Color::hex(0x4A5A75),
    );
    let _ = text_width;
}
