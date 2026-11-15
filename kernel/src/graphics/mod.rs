//! NoorOS graphics engine.
//!
//! ```text
//! applications / desktop      <- UI logic
//!         |
//! primitives.rs, text.rs      <- shapes, panels, glyphs
//!         |
//! canvas.rs                   <- pixels, formats, alpha blending
//!         |
//! bootloader framebuffer      <- hardware
//! ```

pub mod branding;
pub mod canvas;
pub mod colors;
pub mod fonts;
pub mod primitives;
pub mod text;

pub use canvas::Canvas;
pub use colors::Color;

/// An axis-aligned rectangle in screen coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> i32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }

    pub fn center_x(&self) -> i32 {
        self.x + self.w / 2
    }
}
