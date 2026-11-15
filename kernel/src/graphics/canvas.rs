//! Raw framebuffer canvas.
//!
//! This is the lowest graphics layer: it knows about pixel formats, strides and
//! bounds checking, and nothing else. All higher-level drawing is built on
//! [`Canvas::put_pixel`] and [`Canvas::blend_pixel`].

use bootloader_api::info::{FrameBufferInfo, PixelFormat};

use super::colors::Color;

pub struct Canvas<'a> {
    buffer: &'a mut [u8],
    info: FrameBufferInfo,
}

impl<'a> Canvas<'a> {
    pub fn new(buffer: &'a mut [u8], info: FrameBufferInfo) -> Self {
        Self { buffer, info }
    }

    pub fn width(&self) -> i32 {
        self.info.width as i32
    }

    pub fn height(&self) -> i32 {
        self.info.height as i32
    }

    #[inline]
    fn offset(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.info.width as i32 || y >= self.info.height as i32 {
            return None;
        }
        Some((y as usize * self.info.stride + x as usize) * self.info.bytes_per_pixel)
    }

    /// Encode a colour the way this framebuffer expects.
    #[inline]
    fn encode(&self, color: Color) -> u32 {
        match self.info.pixel_format {
            PixelFormat::Rgb => u32::from_le_bytes([color.r, color.g, color.b, 0xff]),
            PixelFormat::Bgr => u32::from_le_bytes([color.b, color.g, color.r, 0xff]),
            // Greyscale framebuffer: approximate luminance.
            _ => {
                let l =
                    ((color.r as u32 * 77 + color.g as u32 * 150 + color.b as u32 * 29) >> 8) as u8;
                u32::from_le_bytes([l, l, l, 0xff])
            }
        }
    }

    #[inline]
    pub fn put_pixel(&mut self, x: i32, y: i32, color: Color) {
        let Some(off) = self.offset(x, y) else { return };
        let bpp = self.info.bytes_per_pixel;
        let word = self.encode(color);
        let bytes = word.to_le_bytes();
        let n = bpp.min(4);
        self.buffer[off..off + n].copy_from_slice(&bytes[..n]);
        // Unused high byte (if any) stays opaque.
        if bpp > 4 {
            for byte in &mut self.buffer[off + 4..off + bpp] {
                *byte = 0xff;
            }
        }
    }

    #[inline]
    pub fn pixel_at(&self, x: i32, y: i32) -> Option<Color> {
        let off = self.offset(x, y)?;
        let bpp = self.info.bytes_per_pixel;
        if bpp < 3 {
            return None;
        }
        let a = self.buffer[off];
        let b = self.buffer[off + 1];
        let c = self.buffer[off + 2];
        Some(match self.info.pixel_format {
            PixelFormat::Rgb => Color::new(a, b, c),
            PixelFormat::Bgr => Color::new(c, b, a),
            _ => Color::new(a, a, a),
        })
    }

    /// Per-pixel alpha blend against the current framebuffer contents.
    ///
    /// NoorOS has no separate compositor: translucency is achieved by blending
    /// straight into the framebuffer, so a panel lets the already-drawn
    /// wallpaper or window behind it show through.
    #[inline]
    pub fn blend_pixel(&mut self, x: i32, y: i32, color: Color, alpha: u8) {
        if alpha == 0 {
            return;
        }
        if alpha == 255 {
            self.put_pixel(x, y, color);
            return;
        }
        let Some(bg) = self.pixel_at(x, y) else {
            return;
        };
        let a = alpha as u32;
        let inv = 255 - a;
        let mix = |f: u8, b: u8| -> u8 { ((f as u32 * a + b as u32 * inv) / 255) as u8 };
        self.put_pixel(
            x,
            y,
            Color::new(mix(color.r, bg.r), mix(color.g, bg.g), mix(color.b, bg.b)),
        );
    }

    /// Fill the whole framebuffer, including row padding.
    pub fn clear(&mut self, color: Color) {
        let bpp = self.info.bytes_per_pixel;
        if bpp == 4 {
            let word = self.encode(color);
            let bytes = word.to_le_bytes();
            for chunk in self.buffer.as_chunks_mut::<4>().0 {
                chunk.copy_from_slice(&bytes);
            }
            return;
        }
        let len = self.buffer.len();
        for off in (0..len).step_by(bpp.max(1)) {
            let n = bpp.min(4);
            let bytes = self.encode(color).to_le_bytes();
            self.buffer[off..off + n].copy_from_slice(&bytes[..n]);
        }
    }

    /// Fill one horizontal line as fast as the pixel format allows.
    pub fn fast_hline(&mut self, x: i32, y: i32, w: i32, color: Color) {
        if w <= 0 || y < 0 || y >= self.info.height as i32 {
            return;
        }
        let x0 = x.max(0);
        let x1 = (x + w).min(self.info.width as i32);
        if x1 <= x0 {
            return;
        }
        let bpp = self.info.bytes_per_pixel;
        let start = (y as usize * self.info.stride + x0 as usize) * bpp;
        let end = (y as usize * self.info.stride + x1 as usize) * bpp;
        match bpp {
            4 => {
                let bytes = self.encode(color).to_le_bytes();
                for chunk in self.buffer[start..end].as_chunks_mut::<4>().0 {
                    chunk.copy_from_slice(&bytes);
                }
            }
            3 => {
                // The common case on QEMU's std VGA: write three bytes per
                // pixel without recomputing the format on every pixel.
                let bytes = match self.info.pixel_format {
                    PixelFormat::Rgb => [color.r, color.g, color.b],
                    _ => [color.b, color.g, color.r],
                };
                for chunk in self.buffer[start..end].as_chunks_mut::<3>().0 {
                    chunk.copy_from_slice(&bytes);
                }
            }
            _ => {
                for px in x0..x1 {
                    self.put_pixel(px, y, color);
                }
            }
        }
    }

    /// Copies a previously rendered frame (or background) into the display.
    ///
    /// Wallpapers are procedurally expensive, so the desktop renders one once
    /// into an off-screen buffer and blits it on every repaint.
    pub fn blit(&mut self, source: &[u8]) {
        let n = source.len().min(self.buffer.len());
        self.buffer[..n].copy_from_slice(&source[..n]);
    }
}
