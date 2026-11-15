//! NoorOS colour system.
//!
//! Colours are plain RGB triples. Alpha is not stored in the colour; drawing
//! functions take an explicit alpha value and blend against the pixel that is
//! already in the framebuffer (see [`crate::graphics::canvas`]).

/// A 24-bit RGB colour.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// `0xRRGGBB`
    pub const fn hex(rgb: u32) -> Self {
        Self {
            r: ((rgb >> 16) & 0xff) as u8,
            g: ((rgb >> 8) & 0xff) as u8,
            b: (rgb & 0xff) as u8,
        }
    }

    /// Linear interpolation, `num/den` of the way from `self` to `other`.
    pub fn lerp(self, other: Color, num: u32, den: u32) -> Color {
        if den == 0 {
            return self;
        }
        let mix = |a: u8, b: u8| -> u8 {
            let a = a as u32;
            let b = b as u32;
            ((a * (den - num) + b * num) / den) as u8
        };
        Color::new(
            mix(self.r, other.r),
            mix(self.g, other.g),
            mix(self.b, other.b),
        )
    }

    /// Scale brightness by `percent` (100 = unchanged).
    pub fn scale(self, percent: u32) -> Color {
        let f = |c: u8| -> u8 { ((c as u32 * percent) / 100).min(255) as u8 };
        Color::new(f(self.r), f(self.g), f(self.b))
    }
}

// ---------------------------------------------------------------------------
// Official NoorOS palette
// ---------------------------------------------------------------------------

/// Application background (deepest surface).
pub const BG: Color = Color::hex(0x080B14);
/// Desktop background.
pub const DESKTOP: Color = Color::hex(0x0B1120);
/// Primary surface.
pub const SURFACE: Color = Color::hex(0x111827);
/// Secondary surface.
pub const SURFACE_2: Color = Color::hex(0x1E293B);
/// Noor Gold - the primary brand accent.
pub const GOLD: Color = Color::hex(0xF5C76A);
/// Electric Blue - secondary accent.
pub const BLUE: Color = Color::hex(0x38BDF8);
/// Secondary violet accent.
pub const VIOLET: Color = Color::hex(0x8B5CF6);
/// Success / healthy status.
pub const SUCCESS: Color = Color::hex(0x10B981);
/// Primary text.
pub const TEXT: Color = Color::hex(0xF8FAFC);
/// Secondary text.
pub const TEXT_DIM: Color = Color::hex(0x94A3B8);
/// Borders and separators.
pub const BORDER: Color = Color::hex(0x334155);

// Derived shades used by the UI (kept here so the whole system agrees).
pub const WHITE: Color = Color::hex(0xFFFFFF);
pub const BLACK: Color = Color::hex(0x000000);
pub const DANGER: Color = Color::hex(0xF87171);
pub const GOLD_DEEP: Color = Color::hex(0xB98A2E);
pub const TERMINAL_BG: Color = Color::hex(0x070B12);

/// Gradient stops for the Noor brand mark.
pub const GOLD_LIGHT: Color = Color::hex(0xFFE3A3);
