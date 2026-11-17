//! Software mouse cursor with save-under.
//!
//! NoorOS has no compositor and no second framebuffer, so the cursor cannot be
//! a separate layer. Instead it copies the pixels it is about to cover, draws
//! itself, and copies those pixels back before moving again. Two rules keep the
//! screen free of trails:
//!
//! 1. [`Cursor::erase`] must run before anything else repaints the region.
//!    `Desktop::render` therefore erases first and draws the cursor last.
//! 2. The saved pixels belong to the last drawn position, never to the last
//!    *requested* position, so a clipped sprite at the screen edge stays
//!    consistent.
//!
//! The arrow's hotspot is its top-left tip, and it is painted exactly at the
//! pointer coordinate that the window manager hit-tests. Near the right or
//! bottom edge the sprite is clipped by the framebuffer instead of being moved
//! away from the pointer, so what the user aims at is what they click.

use crate::graphics::Canvas;
use crate::graphics::colors::{self, Color};

/// Arrow footprint, without the shadow.
const ARROW_W: usize = 12;
const ARROW_H: usize = 19;

/// The cursor's saved region also covers the one-pixel shadow at (+1, +2).
const REGION_W: i32 = ARROW_W as i32 + 1;
const REGION_H: i32 = ARROW_H as i32 + 2;

const SHADOW_ALPHA: u8 = 80;

/// `' '` transparent, `'X'` outline, `'.'` filled interior.
#[rustfmt::skip]
const SHAPE: [[u8; ARROW_W]; ARROW_H] = [
    *b"X           ",
    *b"XX          ",
    *b"X.X         ",
    *b"X..X        ",
    *b"X...X       ",
    *b"X....X      ",
    *b"X.....X     ",
    *b"X......X    ",
    *b"X.......X   ",
    *b"X........X  ",
    *b"X.........X ",
    *b"X......XXXX ",
    *b"X...X..X    ",
    *b"X..X X..X   ",
    *b"X.X  X..X   ",
    *b"XX   X..X   ",
    *b"X    X..X   ",
    *b"      X..X  ",
    *b"       XX   ",
];

pub struct Cursor {
    /// Where the arrow was last painted, if anywhere.
    drawn: Option<(i32, i32)>,
    /// The framebuffer pixels underneath that arrow.
    saved: [Color; (REGION_W * REGION_H) as usize],
}

impl Cursor {
    pub const fn new() -> Self {
        Self {
            drawn: None,
            saved: [Color::new(0, 0, 0); (REGION_W * REGION_H) as usize],
        }
    }

    /// Restores whatever the cursor is covering. Safe to call when nothing is
    /// drawn, which lets the render path call it unconditionally.
    pub fn erase(&mut self, canvas: &mut Canvas) {
        let Some((x, y)) = self.drawn.take() else {
            return;
        };
        for row in 0..REGION_H {
            for col in 0..REGION_W {
                let color = self.saved[(row * REGION_W + col) as usize];
                canvas.put_pixel(x + col, y + row, color);
            }
        }
    }

    /// Moves the cursor so its hotspot sits exactly on `(x, y)`.
    ///
    /// The driver already clamps the pointer to the framebuffer, so the sprite
    /// may be clipped at the right and bottom edges. Off-screen pixels are
    /// skipped by the canvas and therefore never appear in the save-under
    /// buffer, which keeps erasing correct.
    pub fn move_to(&mut self, canvas: &mut Canvas, x: i32, y: i32) {
        if self.drawn == Some((x, y)) {
            return;
        }
        self.erase(canvas);
        self.paint(canvas, x, y);
    }

    fn paint(&mut self, canvas: &mut Canvas, x: i32, y: i32) {
        // 1. Remember the background before touching it.
        for row in 0..REGION_H {
            for col in 0..REGION_W {
                let pixel = canvas
                    .pixel_at(x + col, y + row)
                    .unwrap_or_else(|| Color::new(0, 0, 0));
                self.saved[(row * REGION_W + col) as usize] = pixel;
            }
        }

        // 2. A soft shadow so the arrow reads on light and dark backgrounds.
        for (row, line) in SHAPE.iter().enumerate() {
            for (col, mask) in line.iter().enumerate() {
                if *mask != b' ' {
                    canvas.blend_pixel(
                        x + col as i32 + 1,
                        y + row as i32 + 2,
                        colors::BLACK,
                        SHADOW_ALPHA,
                    );
                }
            }
        }

        // 3. The arrow itself: outlined so it never disappears into a panel.
        for (row, line) in SHAPE.iter().enumerate() {
            for (col, mask) in line.iter().enumerate() {
                let color = match *mask {
                    b'X' => colors::BG,
                    b'.' => colors::TEXT,
                    _ => continue,
                };
                canvas.put_pixel(x + col as i32, y + row as i32, color);
            }
        }

        self.drawn = Some((x, y));
    }
}
