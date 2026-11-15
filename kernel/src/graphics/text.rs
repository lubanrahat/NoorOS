//! Text rendering.
//!
//! Glyphs are 8x8 bitmaps scaled by an integer factor. There is no anti
//! aliasing: at scale 1 the result is a crisp pixel font, at scale 2+ it is a
//! chunky display font. That is a deliberate trade-off for a tiny kernel.

use super::canvas::Canvas;
use super::colors::Color;
use super::fonts::{self, CHAR_H, CHAR_W};
use super::primitives::fill_rect;

pub fn text_width(text: &str, scale: i32) -> i32 {
    text.chars().count() as i32 * CHAR_W * scale
}

pub fn text_width_tracked(text: &str, scale: i32, tracking: i32) -> i32 {
    let n = text.chars().count() as i32;
    if n == 0 {
        return 0;
    }
    n * (CHAR_W * scale + tracking) - tracking
}

pub fn line_height(scale: i32) -> i32 {
    CHAR_H * scale
}

pub fn draw_char(c: &mut Canvas, x: i32, y: i32, ch: char, scale: i32, color: Color) {
    draw_char_alpha(c, x, y, ch, scale, color, 255);
}

pub fn draw_char_alpha(
    c: &mut Canvas,
    x: i32,
    y: i32,
    ch: char,
    scale: i32,
    color: Color,
    alpha: u8,
) {
    if ch == ' ' {
        return;
    }
    let glyph = fonts::glyph(ch).unwrap_or_else(fonts::fallback);
    for (row, bits) in glyph.iter().enumerate() {
        // Merge consecutive set pixels into a single rectangle per row.
        let mut col = 0;
        while col < 8 {
            if bits & (1 << col) != 0 {
                let start = col;
                while col < 8 && bits & (1 << col) != 0 {
                    col += 1;
                }
                let w = (col - start) * scale;
                let px = x + start * scale;
                let py = y + row as i32 * scale;
                if alpha == 255 {
                    fill_rect(c, px, py, w, scale, color);
                } else {
                    super::primitives::fill_rect_alpha(c, px, py, w, scale, color, alpha);
                }
            } else {
                col += 1;
            }
        }
    }
}

/// Draws `text` and returns the x coordinate just past the last glyph.
pub fn draw_text(c: &mut Canvas, x: i32, y: i32, text: &str, scale: i32, color: Color) -> i32 {
    draw_text_alpha(c, x, y, text, scale, color, 255)
}

pub fn draw_text_alpha(
    c: &mut Canvas,
    x: i32,
    y: i32,
    text: &str,
    scale: i32,
    color: Color,
    alpha: u8,
) -> i32 {
    let mut pen = x;
    for ch in text.chars() {
        if ch == '\n' {
            pen = x;
            continue;
        }
        draw_char_alpha(c, pen, y, ch, scale, color, alpha);
        pen += CHAR_W * scale;
    }
    pen
}

/// Letter-spaced text, used for the NOOROS wordmark.
pub fn draw_text_tracked(
    c: &mut Canvas,
    x: i32,
    y: i32,
    text: &str,
    scale: i32,
    tracking: i32,
    color: Color,
) -> i32 {
    let mut pen = x;
    for ch in text.chars() {
        draw_char(c, pen, y, ch, scale, color);
        pen += CHAR_W * scale + tracking;
    }
    pen
}

pub fn draw_text_centered(
    c: &mut Canvas,
    center_x: i32,
    y: i32,
    text: &str,
    scale: i32,
    color: Color,
) -> i32 {
    let x = center_x - text_width(text, scale) / 2;
    draw_text(c, x, y, text, scale, color)
}

pub fn draw_text_right(c: &mut Canvas, right: i32, y: i32, text: &str, scale: i32, color: Color) {
    let x = right - text_width(text, scale);
    draw_text(c, x, y, text, scale, color);
}

/// Simple greedy word wrap for monospace text. Returns at most `max_lines`
/// lines, ellipsising the last one if the text does not fit.
pub fn wrap<'a>(text: &'a str, columns: usize, max_lines: usize) -> alloc::vec::Vec<&'a str> {
    use alloc::vec::Vec;
    let mut lines: Vec<&'a str> = Vec::new();
    if columns == 0 || max_lines == 0 {
        return lines;
    }
    for raw in text.split('\n') {
        let mut rest = raw;
        if rest.is_empty() {
            lines.push("");
            continue;
        }
        while !rest.is_empty() {
            if lines.len() == max_lines {
                return lines;
            }
            let count = rest.chars().count();
            if count <= columns {
                lines.push(rest);
                break;
            }
            // Find a byte index at the column limit and prefer a space break.
            let mut cut = 0;
            for (seen, (idx, _ch)) in rest.char_indices().enumerate() {
                if seen == columns {
                    cut = idx;
                    break;
                }
            }
            if cut == 0 {
                cut = rest.len();
            }
            let window = &rest[..cut];
            let break_at = window.rfind(' ').unwrap_or(cut);
            let (line, next) = if break_at == 0 || break_at >= cut {
                (&rest[..cut], &rest[cut..])
            } else {
                (&rest[..break_at], &rest[break_at + 1..])
            };
            lines.push(line);
            rest = next;
        }
    }
    lines
}
