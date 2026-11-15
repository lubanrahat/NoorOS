//! Noor Desktop - the NoorOS desktop environment.
//!
//! `Desktop` owns the window list, focus, the launcher and input routing.
//! Rendering is immediate-mode: every repaint draws the wallpaper, windows in
//! z-order, the taskbar and finally the launcher overlay. The mouse cursor is
//! painted on top and restores the pixels underneath it when it moves (see
//! [`cursor`]).

pub mod app_manager;
pub mod cursor;
#[allow(clippy::module_inception)]
pub mod desktop;
pub mod launcher;
pub mod taskbar;
pub mod window;

pub use desktop::{Desktop, run};
