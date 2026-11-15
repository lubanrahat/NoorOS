//! Noor Desktop - the NoorOS desktop environment.
//!
//! `Desktop` owns the window list, focus, launcher and the keyboard routing.
//! Rendering is immediate-mode: every repaint draws the wallpaper, windows in
//! z-order, the taskbar and finally the launcher overlay.

pub mod app_manager;
#[allow(clippy::module_inception)]
pub mod desktop;
pub mod launcher;
pub mod taskbar;
pub mod window;

pub use desktop::{Desktop, run};
