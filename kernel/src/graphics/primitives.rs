//! Drawing primitives implemented on top of [`Canvas`].
//!
//! Everything here is clipped to the framebuffer, so callers may draw partially
//! off-screen without panicking.

#![allow(clippy::too_many_arguments)] // immediate-mode geometry is positional

use super::Rect;
use super::canvas::Canvas;
use super::colors::{BLACK, Color, WHITE};

/// Integer square root (no floating point, no libm).
pub fn isqrt(n: u32) -> u32 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// Horizontal inset of a rounded corner at `dy` rows from the corner tip.
fn corner_inset(radius: i32, dy: i32) -> i32 {
    if dy <= 0 {
        return 0;
    }
    let r = radius.max(0);
    let dy = dy.min(r);
    let remain = (r * r - dy * dy).max(0) as u32;
    r - isqrt(remain) as i32
}

pub fn fill_rect(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, color: Color) {
    if w <= 0 || h <= 0 {
        return;
    }
    let y0 = y.max(0);
    let y1 = (y + h).min(c.height());
    for row in y0..y1 {
        c.fast_hline(x, row, w, color);
    }
}

pub fn fill_rect_alpha(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, color: Color, alpha: u8) {
    if alpha == 255 {
        return fill_rect(c, x, y, w, h, color);
    }
    if w <= 0 || h <= 0 {
        return;
    }
    let x0 = x.max(0);
    let x1 = (x + w).min(c.width());
    let y0 = y.max(0);
    let y1 = (y + h).min(c.height());
    for row in y0..y1 {
        for col in x0..x1 {
            c.blend_pixel(col, row, color, alpha);
        }
    }
}

pub fn v_gradient(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, top: Color, bottom: Color) {
    if h <= 0 {
        return;
    }
    for row in 0..h {
        let color = top.lerp(bottom, row as u32, (h - 1).max(1) as u32);
        c.fast_hline(x, y + row, w, color);
    }
}

pub fn fill_rounded_rect(
    c: &mut Canvas,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    radius: i32,
    color: Color,
) {
    fill_rounded_rect_alpha(c, x, y, w, h, radius, color, 255);
}

pub fn fill_rounded_rect_alpha(
    c: &mut Canvas,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    radius: i32,
    color: Color,
    alpha: u8,
) {
    if w <= 0 || h <= 0 {
        return;
    }
    let r = radius.max(0).min(w / 2).min(h / 2);
    for row in 0..h {
        let dy = if row < r {
            r - row
        } else if row >= h - r {
            row - (h - r) + 1
        } else {
            0
        };
        let inset = corner_inset(r, dy);
        let rw = w - 2 * inset;
        if rw <= 0 {
            continue;
        }
        if alpha == 255 {
            c.fast_hline(x + inset, y + row, rw, color);
        } else {
            fill_rect_alpha(c, x + inset, y + row, rw, 1, color, alpha);
        }
    }
}

pub fn stroke_rounded_rect(
    c: &mut Canvas,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    radius: i32,
    t: i32,
    color: Color,
) {
    stroke_rounded_rect_alpha(c, x, y, w, h, radius, t, color, 255);
}

pub fn stroke_rounded_rect_alpha(
    c: &mut Canvas,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    radius: i32,
    t: i32,
    color: Color,
    alpha: u8,
) {
    if w <= 0 || h <= 0 || t <= 0 {
        return;
    }
    let r = radius.max(0).min(w / 2).min(h / 2);
    let paint = |c: &mut Canvas, x: i32, y: i32, w: i32, h: i32| {
        if alpha == 255 {
            fill_rect(c, x, y, w, h, color);
        } else {
            fill_rect_alpha(c, x, y, w, h, color, alpha);
        }
    };
    for row in 0..h {
        let dy = if row < r {
            r - row
        } else if row >= h - r {
            row - (h - r) + 1
        } else {
            0
        };
        let inset = corner_inset(r, dy);
        paint(c, x + inset, y + row, t, 1);
        paint(c, x + w - inset - t, y + row, t, 1);
    }
    // Straight sections of the top and bottom edges.
    paint(c, x + r, y, w - 2 * r, t);
    paint(c, x + r, y + h - t, w - 2 * r, t);
}

/// Soft drop shadow around a rounded rectangle.
pub fn shadow(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, radius: i32) {
    for spread in 1..=6 {
        let alpha = (16 - spread * 2).max(4) as u8;
        stroke_rounded_rect_alpha(
            c,
            x - spread,
            y - spread + 3,
            w + spread * 2,
            h + spread * 2,
            radius + spread,
            1,
            BLACK,
            alpha,
        );
    }
}

/// Filled circle using the distance test (integer only).
pub fn circle_filled(c: &mut Canvas, cx: i32, cy: i32, radius: i32, color: Color) {
    if radius <= 0 {
        return;
    }
    let r2 = radius * radius;
    for dy in -radius..=radius {
        let dx = isqrt((r2 - dy * dy).max(0) as u32) as i32;
        c.fast_hline(cx - dx, cy + dy, dx * 2 + 1, color);
    }
}

pub fn circle_ring(c: &mut Canvas, cx: i32, cy: i32, radius: i32, t: i32, color: Color) {
    if radius <= 0 || t <= 0 {
        return;
    }
    let outer = radius * radius;
    let inner = (radius - t).max(0);
    let inner = inner * inner;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let d = dx * dx + dy * dy;
            if d <= outer && d >= inner {
                c.put_pixel(cx + dx, cy + dy, color);
            }
        }
    }
}

pub fn line(c: &mut Canvas, x0: i32, y0: i32, x1: i32, y1: i32, color: Color) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let (mut x, mut y) = (x0, y0);
    loop {
        c.put_pixel(x, y, color);
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// A consistent NoorOS surface: shadow, translucent rounded fill, hairline
/// border and a subtle highlight along the top edge.
pub fn panel(
    c: &mut Canvas,
    rect: Rect,
    fill: Color,
    alpha: u8,
    radius: i32,
    border: Color,
    border_alpha: u8,
) {
    shadow(c, rect.x, rect.y, rect.w, rect.h, radius);
    fill_rounded_rect_alpha(c, rect.x, rect.y, rect.w, rect.h, radius, fill, alpha);
    stroke_rounded_rect_alpha(
        c,
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        radius,
        1,
        border,
        border_alpha,
    );
    // Top-edge highlight: glass-like sheen without claiming real transparency.
    fill_rect_alpha(
        c,
        rect.x + radius,
        rect.y + 1,
        rect.w - radius * 2,
        1,
        WHITE,
        10,
    );
}
