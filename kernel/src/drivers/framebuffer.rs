//! Framebuffer ownership.
//!
//! The bootloader maps the display for us and hands over a mutable byte slice.
//! Exactly one [`Canvas`] exists at a time; the desktop loop borrows it through
//! [`with`], and the panic handler uses [`with_try`] so a panic inside drawing
//! code can never deadlock on the framebuffer lock.

use bootloader_api::info::FrameBufferInfo;
use spin::Mutex;

use crate::graphics::Canvas;

struct Framebuffer {
    ptr: *mut u8,
    len: usize,
    info: FrameBufferInfo,
}

// SAFETY: the framebuffer is only accessed while the `FRAMEBUFFER` mutex is held.
// NoorOS runs on a single CPU, and the mutex guarantees one canvas at a time.
unsafe impl Send for Framebuffer {}

static FRAMEBUFFER: Mutex<Option<Framebuffer>> = Mutex::new(None);
static INFO: Mutex<Option<FrameBufferInfo>> = Mutex::new(None);

pub fn init(buffer: &mut [u8], info: FrameBufferInfo) {
    *INFO.lock() = Some(info);
    *FRAMEBUFFER.lock() = Some(Framebuffer {
        ptr: buffer.as_mut_ptr(),
        len: buffer.len(),
        info,
    });
}

/// Display metadata, available without touching the drawing lock.
pub fn info() -> Option<FrameBufferInfo> {
    *INFO.lock()
}

/// Runs `f` with the one and only canvas.
pub fn with<R>(f: impl FnOnce(&mut Canvas<'_>) -> R) -> Option<R> {
    let mut guard = FRAMEBUFFER.lock();
    let fb = guard.as_mut()?;
    // SAFETY: `ptr`/`len` describe the bootloader framebuffer, and the mutex
    // guarantees exclusive access for the duration of the borrow.
    let slice = unsafe { core::slice::from_raw_parts_mut(fb.ptr, fb.len) };
    let mut canvas = Canvas::new(slice, fb.info);
    Some(f(&mut canvas))
}

/// Non-blocking variant for the panic path.
pub fn with_try<R>(f: impl FnOnce(&mut Canvas<'_>) -> R) -> Option<R> {
    let mut guard = FRAMEBUFFER.try_lock()?;
    let fb = guard.as_mut()?;
    // SAFETY: as above; the mutex is held.
    let slice = unsafe { core::slice::from_raw_parts_mut(fb.ptr, fb.len) };
    let mut canvas = Canvas::new(slice, fb.info);
    Some(f(&mut canvas))
}
