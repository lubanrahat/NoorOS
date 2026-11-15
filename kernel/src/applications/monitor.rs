//! Noor Monitor - system information gathered from the kernel itself.
//!
//! Every value shown here is read live from the kernel. When something is not
//! available (for example a temperature sensor, which NoorOS does not drive)
//! the monitor says so instead of inventing a number.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::applications::AppAction;
use crate::applications::ui;
use crate::drivers::keyboard::{InputEvent, Key};
use crate::drivers::{cpu, keyboard, rtc, timer};
use crate::graphics::colors::*;
use crate::graphics::primitives::{fill_rect, fill_rect_alpha};
use crate::graphics::text::{draw_text, text_width};
use crate::graphics::{Canvas, Rect};
use crate::memory;
use crate::util;

const PAD: i32 = 16;
const GAP: i32 = 12;

pub struct Monitor {
    scroll: usize,
}

impl Monitor {
    pub fn new() -> Self {
        Self { scroll: 0 }
    }

    pub fn handle_key(&mut self, event: InputEvent) -> AppAction {
        let max_scroll = memory::map().map_or(0, |m| m.regions.len().saturating_sub(4));
        match event.key {
            Key::Up => self.scroll = self.scroll.saturating_sub(1),
            Key::Down => self.scroll = (self.scroll + 1).min(max_scroll),
            Key::PageUp => self.scroll = self.scroll.saturating_sub(4),
            Key::PageDown => self.scroll = (self.scroll + 4).min(max_scroll),
            Key::Home => self.scroll = 0,
            Key::End => self.scroll = max_scroll,
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

        // Header
        draw_text(canvas, content.x, content.y, "System Monitor", 2, TEXT);
        ui::status_dot(
            canvas,
            content.x + text_width("System Monitor", 2) + 16,
            content.y + 8,
            SUCCESS,
        );
        draw_text(
            canvas,
            content.x + text_width("System Monitor", 2) + 28,
            content.y + 3,
            "kernel running",
            1,
            SUCCESS,
        );
        let stamp = alloc::format!(
            "{} UTC  -  uptime {}",
            rtc::now()
                .map(|t| t.clock())
                .unwrap_or_else(|| String::from("--:--:--")),
            util::uptime(timer::uptime_ms())
        );
        draw_text(
            canvas,
            content.right() - text_width(&stamp, 1),
            content.y + 4,
            &stamp,
            1,
            TEXT_DIM,
        );
        fill_rect(canvas, content.x, content.y + 26, content.w, 1, BORDER);

        let top = content.y + 38;
        let card_w = (content.w - GAP) / 2;
        let card_h = 124;

        // --- Operating system ---------------------------------------------
        let os_rect = Rect::new(content.x, top, card_w, card_h);
        let inner = ui::card(canvas, os_rect, "OPERATING SYSTEM", GOLD);
        let rows = [
            ("Name", String::from("NoorOS")),
            ("Version", String::from(crate::shell::commands::VERSION)),
            ("Kernel", String::from(crate::shell::commands::KERNEL_NAME)),
            ("Architecture", String::from("x86_64")),
            ("State", String::from("Running")),
        ];
        let mut y = inner.y;
        for (key, value) in rows {
            let color = if key == "State" { SUCCESS } else { TEXT };
            ui::kv_color(canvas, inner.x, y, inner.w, key, &value, color);
            y += 17;
        }

        // --- Memory -------------------------------------------------------
        let mem_rect = Rect::new(content.x + card_w + GAP, top, card_w, card_h);
        let inner = ui::card(canvas, mem_rect, "MEMORY", BLUE);
        let heap = memory::heap_stats();
        let mut rows: Vec<(&str, String)> = Vec::new();
        if let Some(map) = memory::map() {
            rows.push(("Usable", util::bytes(map.usable_bytes)));
            rows.push(("Reserved", util::bytes(map.reserved_bytes)));
            rows.push(("Regions", format!("{}", map.regions.len())));
        } else {
            rows.push(("Map", String::from("unavailable")));
        }
        rows.push(("Heap used", util::bytes(heap.used)));
        rows.push(("Heap free", util::bytes(heap.free)));
        let mut y = inner.y;
        for (key, value) in rows {
            ui::kv(canvas, inner.x, y, inner.w, key, &value);
            y += 17;
        }

        // --- Processor ----------------------------------------------------
        let cpu_rect = Rect::new(content.x, top + card_h + GAP, card_w, card_h);
        let inner = ui::card(canvas, cpu_rect, "PROCESSOR", VIOLET);
        let info = cpu::info();
        let rows = [
            ("Vendor", info.vendor.clone()),
            ("Brand", info.brand.clone()),
            ("Logical CPUs", format!("{}", info.logical_cpus)),
            (
                "Family / model",
                format!("{} / {}", info.family, info.model),
            ),
            (
                "Hypervisor",
                String::from(if info.hypervisor {
                    "yes (CPUID)"
                } else {
                    "no (CPUID)"
                }),
            ),
        ];
        let mut y = inner.y;
        for (key, value) in rows {
            ui::kv(canvas, inner.x, y, inner.w, key, &value);
            y += 17;
        }

        // --- System -------------------------------------------------------
        let sys_rect = Rect::new(content.x + card_w + GAP, top + card_h + GAP, card_w, card_h);
        let inner = ui::card(canvas, sys_rect, "SYSTEM", SUCCESS);
        let rows = [
            ("Uptime", util::uptime(timer::uptime_ms())),
            (
                "RTC clock",
                rtc::now()
                    .map(|t| format!("{} {}", t.date(), t.clock()))
                    .unwrap_or_else(|| String::from("unavailable")),
            ),
            ("PIT frequency", format!("{} Hz", timer::TICK_HZ)),
            ("Timer IRQs", format!("{}", timer::interrupt_count())),
            (
                "Keyboard IRQs",
                format!(
                    "{} ({} dropped)",
                    keyboard::interrupt_count(),
                    keyboard::dropped()
                ),
            ),
        ];
        let mut y = inner.y;
        for (key, value) in rows {
            ui::kv(canvas, inner.x, y, inner.w, key, &value);
            y += 17;
        }

        // --- Physical memory map ------------------------------------------
        let map_rect = Rect::new(
            content.x,
            top + card_h * 2 + GAP * 2,
            content.w,
            (content.bottom() - (top + card_h * 2 + GAP * 2)).max(60),
        );
        let inner = ui::card(canvas, map_rect, "PHYSICAL MEMORY MAP", GOLD);
        draw_text(canvas, inner.x, inner.y, "START", 1, TEXT_DIM);
        draw_text(canvas, inner.x + 104, inner.y, "END", 1, TEXT_DIM);
        draw_text(canvas, inner.x + 208, inner.y, "SIZE", 1, TEXT_DIM);
        draw_text(canvas, inner.x + 288, inner.y, "TYPE", 1, TEXT_DIM);
        fill_rect(canvas, inner.x, inner.y + 13, inner.w, 1, BORDER);

        let mut y = inner.y + 20;
        if let Some(map) = memory::map() {
            let visible = ((inner.h - 44) / 15).max(1) as usize;
            for region in map.regions.iter().skip(self.scroll).take(visible) {
                draw_text(
                    canvas,
                    inner.x,
                    y,
                    &format!("{:012X}", region.start),
                    1,
                    TEXT,
                );
                draw_text(
                    canvas,
                    inner.x + 104,
                    y,
                    &format!("{:012X}", region.start + region.len),
                    1,
                    TEXT,
                );
                draw_text(canvas, inner.x + 208, y, &util::bytes(region.len), 1, BLUE);
                draw_text(canvas, inner.x + 288, y, &region.kind.detail(), 1, TEXT_DIM);
                y += 15;
            }
            let hint = format!(
                "{} regions  -  source: bootloader memory map  -  Up/Down scroll",
                map.regions.len()
            );
            draw_text(canvas, inner.x, inner.bottom() - 12, &hint, 1, TEXT_DIM);
        } else {
            draw_text(canvas, inner.x, y, "Memory map unavailable", 1, DANGER);
        }

        // Subtle bottom accent line.
        fill_rect_alpha(canvas, rect.x, rect.y + rect.h - 2, rect.w, 2, BLUE, 90);
    }
}
