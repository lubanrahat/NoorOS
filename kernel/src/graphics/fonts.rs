//! Bitmap font access.
//!
//! NoorOS embeds the classic 8x8 IBM-style bitmap font from the `font8x8`
//! crate. Glyphs are rendered by the kernel pixel by pixel (with integer
//! scaling), so there is no font rasteriser and no external asset dependency.

use font8x8::{BASIC_FONTS, UnicodeFonts};

pub const CHAR_W: i32 = 8;
pub const CHAR_H: i32 = 8;

/// Returns the 8 rows of a glyph. Row bit 0 is the leftmost pixel.
pub fn glyph(ch: char) -> Option<[u8; 8]> {
    BASIC_FONTS.get(ch)
}

/// Fallback glyph for characters the font cannot render.
pub fn fallback() -> [u8; 8] {
    [0x00, 0x7e, 0x42, 0x42, 0x42, 0x7e, 0x00, 0x00]
}
