//! Top status bar and the floating Noor taskbar.

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

pub fn draw_top_bar(canvas: &mut Canvas, screen_w: i32) {
    let h = 30;
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
    let bar_w = (screen_w - 90).min(780);
    let bar_h = 58;
    let x = (screen_w - bar_w) / 2;
    let y = screen_h - 74;
    let rect = Rect::new(x, y, bar_w, bar_h);

    panel(canvas, rect, colors::SURFACE, 238, 16, colors::BORDER, 210);

    branding::draw_logo(canvas, x + 30, y + bar_h / 2, 13, colors::GOLD);
    fill_rect(canvas, x + 52, y + 15, 1, bar_h - 30, colors::BORDER);

    let item_w = 82;
    let mut item_x = x + 62;

    // Launcher button
    draw_item(
        canvas,
        item_x,
        y,
        item_w,
        "Launcher",
        None,
        launcher_open,
        false,
        true,
    );
    item_x += item_w;

    for app in AppId::ALL {
        let is_running = running.contains(&app);
        let is_focused = focused == Some(app);
        draw_item(
            canvas,
            item_x,
            y,
            item_w,
            app.short(),
            Some(app),
            is_running,
            is_focused,
            false,
        );
        item_x += item_w;
    }

    // Clock + uptime on the right.
    let now = rtc::now();
    let time = now
        .map(|t| t.clock_hm())
        .unwrap_or_else(|| String::from("--:--"));
    let date = now
        .map(|t| t.date())
        .unwrap_or_else(|| String::from("----------"));
    let right = x + bar_w - 18;
    draw_text_right(canvas, right, y + 12, &time, 2, colors::TEXT);
    draw_text_right(canvas, right, y + 34, &date, 1, colors::TEXT_DIM);

    let uptime = format!("up {}", util::uptime(timer::uptime_ms()));
    let up_x = right - text_width(&time, 2) - 18;
    if up_x - text_width(&uptime, 1) > item_x + 8 {
        draw_text_right(canvas, up_x, y + 25, &uptime, 1, Color::hex(0x5A6B86));
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
