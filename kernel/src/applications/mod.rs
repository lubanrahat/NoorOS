//! NoorOS applications.
//!
//! Each application is a small state machine with `render` and `handle_key`.
//! The window manager calls those methods; applications never touch the
//! framebuffer directly.

pub mod about;
pub mod monitor;
pub mod settings;
pub mod terminal;
pub mod ui;

use alloc::string::String;

use crate::drivers::keyboard::InputEvent;
use crate::graphics::{Canvas, Rect};

/// What an application asks the desktop to do after handling a key.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    None,
    Close,
    Shutdown,
    Reboot,
}

/// Per-window application state.
pub enum AppState {
    Terminal(terminal::Terminal),
    Monitor(monitor::Monitor),
    Settings(settings::Settings),
    About(about::About),
}

impl AppState {
    pub fn handle_key(&mut self, event: InputEvent) -> AppAction {
        match self {
            AppState::Terminal(app) => app.handle_key(event),
            AppState::Monitor(app) => app.handle_key(event),
            AppState::Settings(app) => app.handle_key(event),
            AppState::About(app) => app.handle_key(event),
        }
    }

    pub fn render(&mut self, canvas: &mut Canvas, content: Rect, tick: u64, focused: bool) {
        match self {
            AppState::Terminal(app) => app.render(canvas, content, tick, focused),
            AppState::Monitor(app) => app.render(canvas, content, tick),
            AppState::Settings(app) => app.render(canvas, content, tick),
            AppState::About(app) => app.render(canvas, content, tick),
        }
    }
}

/// Shorten `text` to `max_chars`, appending an ellipsis when truncated.
pub fn ellipsis(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return String::from(text);
    }
    let mut out = String::new();
    for ch in text.chars().take(max_chars.saturating_sub(3)) {
        out.push(ch);
    }
    out.push_str("...");
    out
}
