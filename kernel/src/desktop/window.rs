//! Window manager: chrome, z-order and keyboard routing.
//!
//! NoorOS uses a simple stacking model: `Desktop` keeps windows in z-order
//! (last = topmost). Focusing a window moves it to the end of the list.

use crate::applications::{AppAction, AppState};
use crate::desktop::app_manager::AppId;
use crate::drivers::keyboard::InputEvent;
use crate::graphics::colors::{self, Color};
use crate::graphics::primitives::*;
use crate::graphics::text::{draw_text, text_width};
use crate::graphics::{Canvas, Rect};

pub const TITLE_H: i32 = 34;

pub struct Window {
    pub app: AppId,
    pub rect: Rect,
    pub state: AppState,
}

impl Window {
    pub fn new(app: AppId, screen_w: i32, screen_h: i32) -> Self {
        Self {
            app,
            rect: app.default_rect(screen_w, screen_h),
            state: app.create_state(),
        }
    }

    pub fn content_rect(&self) -> Rect {
        Rect::new(
            self.rect.x + 1,
            self.rect.y + TITLE_H,
            self.rect.w - 2,
            self.rect.h - TITLE_H - 1,
        )
    }

    pub fn handle_key(&mut self, event: InputEvent) -> AppAction {
        self.state.handle_key(event)
    }

    pub fn render(&mut self, canvas: &mut Canvas, tick: u64, focused: bool) {
        let rect = self.rect;
        let accent = self.app.accent();

        shadow(canvas, rect.x, rect.y, rect.w, rect.h, 12);

        // Body
        fill_rounded_rect(canvas, rect.x, rect.y, rect.w, rect.h, 12, colors::SURFACE);

        // Title bar (rounded top, square bottom)
        let title_color = if focused {
            Color::hex(0x1C2740)
        } else {
            Color::hex(0x141C2E)
        };
        fill_rounded_rect(canvas, rect.x, rect.y, rect.w, TITLE_H, 12, title_color);
        fill_rect(
            canvas,
            rect.x,
            rect.y + 20,
            rect.w,
            TITLE_H - 20,
            title_color,
        );
        fill_rect(
            canvas,
            rect.x,
            rect.y + TITLE_H - 1,
            rect.w,
            1,
            colors::BORDER,
        );

        // Accent dot + title
        circle_filled(canvas, rect.x + 18, rect.y + TITLE_H / 2, 3, accent);
        draw_text(
            canvas,
            rect.x + 30,
            rect.y + 12,
            self.app.title(),
            1,
            if focused {
                colors::TEXT
            } else {
                colors::TEXT_DIM
            },
        );

        // Right-hand chrome: a small "x86_64" tag and the close button.
        let close_rect = Rect::new(rect.right() - 30, rect.y + 10, 14, 14);
        let tag = "x86_64";
        let tag_x = close_rect.x - 16 - text_width(tag, 1);
        if tag_x > rect.x + 160 {
            draw_text(canvas, tag_x, rect.y + 13, tag, 1, Color::hex(0x46566F));
        }
        let close_color = if focused {
            Color::hex(0xE27474)
        } else {
            Color::hex(0x516079)
        };
        fill_rounded_rect(
            canvas,
            close_rect.x - 3,
            close_rect.y - 3,
            20,
            20,
            6,
            Color::hex(0x22304A),
        );
        line(
            canvas,
            close_rect.x,
            close_rect.y,
            close_rect.x + 8,
            close_rect.y + 8,
            close_color,
        );
        line(
            canvas,
            close_rect.x + 8,
            close_rect.y,
            close_rect.x,
            close_rect.y + 8,
            close_color,
        );

        // Content
        let content = self.content_rect();
        fill_rect(
            canvas,
            content.x,
            content.y,
            content.w,
            content.h,
            colors::BG,
        );
        self.state.render(canvas, content, tick, focused);

        // Focused outline + top glow.
        if focused {
            stroke_rounded_rect(canvas, rect.x, rect.y, rect.w, rect.h, 12, 1, accent);
            fill_rect_alpha(canvas, rect.x + 12, rect.y, rect.w - 24, 2, accent, 170);
        } else {
            stroke_rounded_rect(
                canvas,
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                12,
                1,
                colors::BORDER,
            );
        }
    }
}
