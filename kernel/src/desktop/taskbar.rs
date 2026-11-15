//! Top status bar and the floating Noor taskbar.
//!
//! Geometry lives in [`bar_rect`] and [`item_rect`] so that what is drawn and
//! what the mouse hit-tests can never drift apart.

use alloc::format;
use alloc::string::String;

use crate::desktop::app_manager::AppId;
use crate::drivers::{rtc, timer};
use crate::graphics::branding;
use crate::graphics::colors::{self, Color};
use crate::graphics::primitives::*;
use crate::graphics::text::*;
use crate::graphics::{Canvas, Rect};
use crate::util;

/// Height of the always-on-top status bar.
pub const TOP_BAR_H: i32 = 30;

/// Height of the floating taskbar.
pub const BAR_H: i32 = 58;

const BAR_ITEM_W: i32 = 82;
/// Distance from the bar's left edge to the first clickable slot.
const BAR_ITEM_X: i32 = 62;

/// A clickable taskbar slot.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TaskbarItem {
    Launcher,
    App(AppId),
}

pub fn bar_rect(screen_w: i32, screen_h: i32) -> Rect {
    let bar_w = (screen_w - 90).min(780);
    Rect::new((screen_w - bar_w) / 2, screen_h - 74, bar_w, BAR_H)
}

/// Slot `index`: 0 is the launcher, `1..=AppId::ALL.len()` are the applications.
pub fn item_rect(screen_w: i32, screen_h: i32, index: usize) -> Rect {
    let bar = bar_rect(screen_w, screen_h);
    Rect::new(
        bar.x + BAR_ITEM_X + index as i32 * BAR_ITEM_W,
        bar.y,
        BAR_ITEM_W,
        BAR_H,
    )
}

/// Which slot, if any, contains the point.
pub fn item_at(screen_w: i32, screen_h: i32, x: i32, y: i32) -> Option<TaskbarItem> {
    for index in 0..=AppId::ALL.len() {
        if item_rect(screen_w, screen_h, index).contains(x, y) {
            return Some(if index == 0 {
                TaskbarItem::Launcher
            } else {
                TaskbarItem::App(AppId::ALL[index - 1])
            });
        }
    }
    None
}

pub fn draw_top_bar(canvas: &mut Canvas, screen_w: i32) {
    let h = TOP_BAR_H;
    fill_rect_alpha(canvas, 0, 0, screen_w, h, colors::BG, 225);
    fill_rect_alpha(canvas, 0, h - 1, screen_w, 1, colors::BORDER, 190);

    branding::draw_logo(canvas, 18, 15, 7, colors::GOLD);
    let wordmark_end = draw_text_tracked(canvas, 32, 7, "NOOROS", 2, 2, colors::TEXT);
    draw_text(canvas, wordmark_end + 6, 11, "0.1.0", 1, colors::GOLD);

    let right = screen_w - 18;

    // Clock (real time from the CMOS RTC), right-aligned as one line.
    let now = rtc::now();
    let time = now
        .map(|t| t.clock_hm())
        .unwrap_or_else(|| String::from("--:--"));
    let date = now
        .map(|t| t.date())
        .unwrap_or_else(|| String::from("----------"));
    let mut cursor = right - text_width(&time, 1);
    draw_text(canvas, cursor, 11, &time, 1, colors::TEXT);
    cursor -= 18;
    cursor -= text_width(&date, 1);
    draw_text(canvas, cursor, 11, &date, 1, colors::TEXT_DIM);
    cursor -= 26;

    // Architecture.
    let arch = "x86_64";
    cursor -= text_width(arch, 1);
    draw_text(canvas, cursor, 11, arch, 1, colors::TEXT_DIM);
    cursor -= 22;

    // Kernel status.
    let status = "Noor Kernel: Running";
    cursor -= text_width(status, 1);
    draw_text(canvas, cursor, 11, status, 1, colors::TEXT);
    fill_rect_alpha(canvas, cursor - 14, 10, 6, 6, colors::SUCCESS, 70);
    circle_filled(canvas, cursor - 11, 13, 3, colors::SUCCESS);
    cursor -= 26;

    // Uptime, only if there is room left of the wordmark.
    if cursor - 150 > 220 {
        let uptime = format!("up {}", util::uptime(timer::uptime_ms()));
        draw_text_right(canvas, cursor, 11, &uptime, 1, Color::hex(0x5A6B86));
    }
}

pub fn draw(
    canvas: &mut Canvas,
    screen_w: i32,
    screen_h: i32,
    running: &[AppId],
    focused: Option<AppId>,
    launcher_open: bool,
) {
    let rect = bar_rect(screen_w, screen_h);
    panel(canvas, rect, colors::SURFACE, 238, 16, colors::BORDER, 210);

    branding::draw_logo(canvas, rect.x + 30, rect.y + BAR_H / 2, 13, colors::GOLD);
    fill_rect(
        canvas,
        rect.x + 52,
        rect.y + 15,
        1,
        BAR_H - 30,
        colors::BORDER,
    );

    // Launcher button, then one slot per application.
    let launcher = item_rect(screen_w, screen_h, 0);
    draw_item(
        canvas,
        launcher.x,
        launcher.y,
        launcher.w,
        "Launcher",
        None,
        launcher_open,
        false,
        true,
    );

    for (offset, app) in AppId::ALL.iter().enumerate() {
        let slot = item_rect(screen_w, screen_h, offset + 1);
        let is_running = running.contains(app);
        let is_focused = focused == Some(*app);
        draw_item(
            canvas,
            slot.x,
            slot.y,
            slot.w,
            app.short(),
            Some(*app),
            is_running,
            is_focused,
            false,
        );
    }

    // Clock + uptime on the right.
    let now = rtc::now();
    let time = now
        .map(|t| t.clock_hm())
        .unwrap_or_else(|| String::from("--:--"));
    let date = now
        .map(|t| t.date())
        .unwrap_or_else(|| String::from("----------"));
    let right = rect.right() - 18;
    draw_text_right(canvas, right, rect.y + 12, &time, 2, colors::TEXT);
    draw_text_right(canvas, right, rect.y + 34, &date, 1, colors::TEXT_DIM);

    let last_slot = item_rect(screen_w, screen_h, AppId::ALL.len());
    let uptime = format!("up {}", util::uptime(timer::uptime_ms()));
    let up_x = right - text_width(&time, 2) - 18;
    if up_x - text_width(&uptime, 1) > last_slot.right() + 8 {
        draw_text_right(canvas, up_x, rect.y + 25, &uptime, 1, Color::hex(0x5A6B86));
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_item(
    canvas: &mut Canvas,
    x: i32,
    bar_y: i32,
    width: i32,
    label: &str,
    app: Option<AppId>,
    active: bool,
    focused: bool,
    launcher: bool,
) {
    let icon_rect = Rect::new(x + (width - 24) / 2, bar_y + 8, 24, 24);
    match app {
        Some(app) => app.draw_icon(canvas, icon_rect),
        None => AppId::draw_launcher_icon(canvas, icon_rect),
    }
    if active && !launcher {
        // Highlight the icon of an open application.
        fill_rounded_rect_alpha(
            canvas,
            icon_rect.x - 3,
            icon_rect.y - 3,
            30,
            30,
            7,
            colors::GOLD,
            16,
        );
    }
    draw_text_centered(
        canvas,
        x + width / 2,
        bar_y + 38,
        label,
        1,
        if focused || (launcher && active) {
            colors::TEXT
        } else {
            colors::TEXT_DIM
        },
    );

    if active {
        let color = if focused || launcher {
            colors::GOLD
        } else {
            Color::hex(0x6B7C97)
        };
        fill_rounded_rect(canvas, x + width / 2 - 7, bar_y + 51, 14, 2, 1, color);
    }
}
