//! Noor Launcher - the application launcher overlay.

use alloc::format;

use crate::desktop::app_manager::AppId;
use crate::drivers::keyboard::{InputEvent, Key};
use crate::graphics::colors::{self, Color};
use crate::graphics::primitives::*;
use crate::graphics::text::*;
use crate::graphics::{Canvas, Rect};

use crate::graphics::branding;

pub enum LauncherAction {
    None,
    Launch(AppId),
    Close,
}

pub struct Launcher {
    pub open: bool,
    pub selected: usize,
}

impl Launcher {
    pub fn new() -> Self {
        Self {
            open: false,
            selected: 0,
        }
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.selected = 0;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn handle_key(&mut self, event: InputEvent) -> LauncherAction {
        if !self.open {
            return LauncherAction::None;
        }
        match event.key {
            Key::Up => {
                self.selected = self.selected.saturating_sub(1);
                LauncherAction::None
            }
            Key::Down => {
                self.selected = (self.selected + 1).min(AppId::ALL.len() - 1);
                LauncherAction::None
            }
            Key::Enter => LauncherAction::Launch(AppId::ALL[self.selected]),
            Key::Escape | Key::F(1) => LauncherAction::Close,
            _ => LauncherAction::None,
        }
    }

    pub fn draw(&self, canvas: &mut Canvas, screen_w: i32, screen_h: i32) {
        if !self.open {
            return;
        }

        // Opaque modal backdrop. A full-screen per-pixel alpha blend costs
        // seconds under QEMU's TCG, so the launcher uses a solid scrim and puts
        // its contrast into the panel instead.
        fill_rect(canvas, 0, 0, screen_w, screen_h, Color::hex(0x05080F));

        let w = (screen_w - 160).min(560);
        let row_h = 58;
        let h = 92 + AppId::ALL.len() as i32 * row_h + 44;
        let x = (screen_w - w) / 2;
        let y = (screen_h - h) / 2 - 24;
        let rect = Rect::new(x, y, w, h);
        panel(canvas, rect, colors::SURFACE, 246, 16, colors::GOLD, 110);

        branding::draw_logo(canvas, x + 34, y + 32, 15, colors::GOLD);
        draw_text_tracked(canvas, x + 58, y + 22, "NOOR LAUNCHER", 2, 2, colors::TEXT);
        draw_text(
            canvas,
            x + 58,
            y + 46,
            "Choose an application",
            1,
            colors::TEXT_DIM,
        );
        fill_rect(canvas, x + 20, y + 70, w - 40, 1, colors::BORDER);

        let mut row_y = y + 82;
        for (index, app) in AppId::ALL.iter().enumerate() {
            let selected = index == self.selected;
            let item = Rect::new(x + 14, row_y, w - 28, row_h - 6);
            if selected {
                fill_rounded_rect(
                    canvas,
                    item.x,
                    item.y,
                    item.w,
                    item.h,
                    10,
                    colors::SURFACE_2,
                );
                stroke_rounded_rect(canvas, item.x, item.y, item.w, item.h, 10, 1, colors::GOLD);
                fill_rect(canvas, item.x, item.y + 12, 3, item.h - 24, app.accent());
            }
            app.draw_icon(canvas, Rect::new(item.x + 14, item.y + 12, 26, 26));
            draw_text(
                canvas,
                item.x + 54,
                item.y + 10,
                app.title(),
                2,
                colors::TEXT,
            );
            draw_text(
                canvas,
                item.x + 54,
                item.y + 32,
                app.description(),
                1,
                colors::TEXT_DIM,
            );
            if selected {
                let key = format!("F{}", index + 2);
                draw_keycap(canvas, item.right() - 42, item.y + 17, &key, app.accent());
            }
            row_y += row_h;
        }

        draw_text_centered(
            canvas,
            x + w / 2,
            y + h - 28,
            "Up / Down  select      Enter  open      Esc  close",
            1,
            Color::hex(0x6B7C97),
        );
    }
}

fn draw_keycap(canvas: &mut Canvas, x: i32, y: i32, label: &str, accent: Color) {
    let w = text_width(label, 1) + 14;
    fill_rounded_rect(canvas, x, y, w, 18, 5, Color::hex(0x0C1320));
    stroke_rounded_rect(canvas, x, y, w, 18, 5, 1, accent);
    draw_text(canvas, x + 7, y + 5, label, 1, accent);
}
