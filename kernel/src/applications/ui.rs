//! Small reusable widgets shared by all NoorOS applications.
//!
//! These keep the application code focused on content instead of pixel maths.

#![allow(clippy::too_many_arguments)] // widget geometry is positional by design

use alloc::string::String;

use crate::graphics::colors::*;
use crate::graphics::primitives::*;
use crate::graphics::text::*;
use crate::graphics::{Canvas, Rect};

use super::ellipsis;

/// Paints an application's page background.
pub fn background(canvas: &mut Canvas, rect: Rect) {
    fill_rect(canvas, rect.x, rect.y, rect.w, rect.h, BG);
}

/// A titled card. Returns the inner content rectangle.
pub fn card(canvas: &mut Canvas, rect: Rect, title: &str, accent: crate::graphics::Color) -> Rect {
    panel(canvas, rect, SURFACE, 255, 10, BORDER, 255);
    fill_rect(canvas, rect.x + 14, rect.y + 15, 3, 9, accent);
    draw_text_tracked(canvas, rect.x + 25, rect.y + 15, title, 1, 1, TEXT);
    fill_rect(canvas, rect.x + 14, rect.y + 38, rect.w - 28, 1, BORDER);
    Rect::new(rect.x + 14, rect.y + 48, rect.w - 28, rect.h - 56)
}

/// Key/value row: dim key on the left, value right-aligned.
pub fn kv(canvas: &mut Canvas, x: i32, y: i32, width: i32, key: &str, value: &str) {
    draw_text(canvas, x, y, key, 1, TEXT_DIM);
    let key_width = text_width(key, 1);
    let available = ((width - key_width - 12) / 8).max(4) as usize;
    let value = ellipsis(value, available);
    draw_text_right(canvas, x + width, y, &value, 1, TEXT);
}

/// Key/value row with a coloured value (used for status indicators).
pub fn kv_color(
    canvas: &mut Canvas,
    x: i32,
    y: i32,
    width: i32,
    key: &str,
    value: &str,
    color: crate::graphics::Color,
) {
    draw_text(canvas, x, y, key, 1, TEXT_DIM);
    let key_width = text_width(key, 1);
    let available = ((width - key_width - 12) / 8).max(4) as usize;
    let value = ellipsis(value, available);
    draw_text_right(canvas, x + width, y, &value, 1, color);
}

pub fn divider(canvas: &mut Canvas, x: i32, y: i32, width: i32) {
    fill_rect(canvas, x, y, width, 1, BORDER);
}

/// Small status dot with a soft halo.
pub fn status_dot(canvas: &mut Canvas, x: i32, y: i32, color: crate::graphics::Color) {
    fill_rect_alpha(canvas, x - 3, y - 3, 9, 9, color, 40);
    circle_filled(canvas, x, y, 3, color);
}

/// Draws wrapped body text and returns the next free y coordinate.
pub fn paragraph(
    canvas: &mut Canvas,
    x: i32,
    y: i32,
    width: i32,
    text: &str,
    scale: i32,
    color: crate::graphics::Color,
    max_lines: usize,
) -> i32 {
    let columns = (width / (8 * scale)).max(8) as usize;
    let mut cy = y;
    for line in wrap(text, columns, max_lines) {
        draw_text(canvas, x, cy, line, scale, color);
        cy += line_height(scale) + 4;
    }
    cy
}

/// A pill-shaped label.
pub fn pill(
    canvas: &mut Canvas,
    x: i32,
    y: i32,
    text: &str,
    fg: crate::graphics::Color,
    bg: crate::graphics::Color,
) {
    let width = text_width(text, 1) + 16;
    fill_rounded_rect(canvas, x, y - 2, width, 14, 7, bg);
    draw_text(canvas, x + 8, y + 1, text, 1, fg);
}

/// Section heading used by Noor Settings.
pub fn heading(canvas: &mut Canvas, x: i32, y: i32, text: &str) {
    draw_text(canvas, x, y, text, 2, TEXT);
    fill_rect(canvas, x, y + 20, text_width(text, 2), 1, GOLD);
}

/// Colour swatch with a label (Noor Settings -> Appearance).
pub fn swatch(canvas: &mut Canvas, x: i32, y: i32, color: crate::graphics::Color, name: &str) {
    fill_rounded_rect(canvas, x, y, 26, 26, 6, color);
    stroke_rounded_rect(canvas, x, y, 26, 26, 6, 1, BORDER);
    draw_text(canvas, x + 36, y + 2, name, 1, TEXT);
    draw_text(canvas, x + 36, y + 14, &hex_string(color), 1, TEXT_DIM);
}

pub fn hex_string(color: crate::graphics::Color) -> String {
    alloc::format!("#{:02X}{:02X}{:02X}", color.r, color.g, color.b)
}
