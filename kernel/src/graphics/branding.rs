//! NoorOS visual identity: the Noor mark and the desktop wallpaper.
//!
//! Both are drawn procedurally with the kernel's own primitives. No image files
//! are embedded or decoded.

use super::canvas::Canvas;
use super::colors::{self, Color};
use super::primitives::*;
use super::text::{draw_text_centered, draw_text_tracked};

/// The Noor mark: a ring of light with a four-pointed star.
///
/// `accent` is the main colour; the ring is drawn slightly dimmer so the star
/// stays the focal point.
pub fn draw_logo(c: &mut Canvas, cx: i32, cy: i32, radius: i32, accent: Color) {
    if radius < 3 {
        return;
    }
    let ring = (radius / 12).max(1);
    let ring_color = accent.scale(72);
    circle_ring(c, cx, cy, radius, ring, ring_color);

    // Four-pointed sparkle: vertical and horizontal tapered arms.
    let len = (radius as i64 * 62 / 100) as i32;
    let mut dy = -len;
    while dy <= len {
        let w = (len - dy.abs()) * 22 / 100;
        fill_rect(c, cx - w, cy + dy, w * 2 + 1, 1, accent);
        dy += 1;
    }
    let mut dx = -len;
    while dx <= len {
        let h = (len - dx.abs()) * 22 / 100;
        fill_rect(c, cx + dx, cy - h, 1, h * 2 + 1, accent);
        dx += 1;
    }

    // Bright core.
    circle_filled(c, cx, cy, (radius / 9).max(1), colors::GOLD_LIGHT);
}

/// Dimmed variant used as a wallpaper watermark.
pub fn draw_logo_subtle(c: &mut Canvas, cx: i32, cy: i32, radius: i32, accent: Color) {
    let dim = accent.lerp(colors::DESKTOP, 90, 100);
    draw_logo(c, cx, cy, radius, dim);
}

/// Soft radial glow. `falloff` controls how quickly the light fades.
///
/// Sampling is done on a coarse grid (one alpha value per 6x6 block) and the
/// block is filled with a single blend. At these radii the gradient changes
/// slowly, so the result is visually smooth while costing ~36x less CPU than a
/// per-pixel evaluation - which matters under QEMU's TCG emulation.
pub fn radial_glow(c: &mut Canvas, cx: i32, cy: i32, radius: i32, color: Color, max_alpha: u8) {
    if radius <= 0 || max_alpha == 0 {
        return;
    }
    const BLOCK: i32 = 6;
    let x0 = (cx - radius).max(0);
    let x1 = (cx + radius).min(c.width());
    let y0 = (cy - radius).max(0);
    let y1 = (cy + radius).min(c.height());
    let r2 = radius as i64 * radius as i64;
    let mut y = y0;
    while y < y1 {
        let dy = (y + BLOCK / 2 - cy) as i64;
        let mut x = x0;
        while x < x1 {
            let dx = (x + BLOCK / 2 - cx) as i64;
            let d2 = dx * dx + dy * dy;
            if d2 < r2 {
                // 1 - (d/r)^2, expressed in integers.
                let t = ((r2 - d2) * 255 / r2) as u32;
                let alpha = (t * max_alpha as u32 / 255) as u8;
                if alpha > 1 {
                    fill_rect_alpha(c, x, y, BLOCK, BLOCK, color, alpha);
                }
            }
            x += BLOCK;
        }
        y += BLOCK;
    }
}

/// The NoorOS desktop wallpaper.
pub fn draw_wallpaper(c: &mut Canvas) {
    let w = c.width();
    let h = c.height();

    // Midnight base with a faint lift towards the top.
    v_gradient(
        c,
        0,
        0,
        w,
        h,
        Color::hex(0x0E1626),
        colors::Color::hex(0x05070D),
    );

    // Ambient light sources: gold above-right, blue below-left, violet centre.
    radial_glow(
        c,
        w * 78 / 100,
        h * 16 / 100,
        h * 52 / 100,
        colors::GOLD,
        16,
    );
    radial_glow(
        c,
        w * 14 / 100,
        h * 82 / 100,
        h * 58 / 100,
        colors::BLUE,
        14,
    );
    radial_glow(
        c,
        w * 52 / 100,
        h * 48 / 100,
        h * 62 / 100,
        colors::VIOLET,
        8,
    );

    // Faint technical grid.
    let step = 48;
    let mut x = 0;
    while x < w {
        for y in 0..h {
            c.blend_pixel(x, y, colors::BLUE, 5);
        }
        x += step;
    }
    let mut y = 0;
    while y < h {
        for x in 0..w {
            c.blend_pixel(x, y, colors::BLUE, 5);
        }
        y += step;
    }

    // Watermark brand mark.
    draw_logo_subtle(c, w * 70 / 100, h * 38 / 100, h / 5, colors::GOLD);

    // Thin horizon accent.
    let hy = h * 74 / 100;
    for x in 0..w {
        let t = ((x as u32 * 255) / w.max(1) as u32) as u8;
        let alpha = ((255 - (t as i32 - 128).abs()) * 26 / 255) as u8;
        c.blend_pixel(x, hy, colors::GOLD, alpha);
    }
}

/// Centred wordmark used by the boot screen, About window and taskbar.
pub fn draw_wordmark(c: &mut Canvas, cx: i32, y: i32, scale: i32, color: Color) {
    let tracking = scale;
    let width = super::text::text_width_tracked("NOOROS", scale, tracking);
    draw_text_tracked(c, cx - width / 2, y, "NOOROS", scale, tracking, color);
}

/// Tagline in secondary text colour.
pub fn draw_tagline(c: &mut Canvas, cx: i32, y: i32, scale: i32) {
    draw_text_centered(
        c,
        cx,
        y,
        "A Light Beyond Computing.",
        scale,
        colors::TEXT_DIM,
    );
}
