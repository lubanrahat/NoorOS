//! Application registry: identity, metadata, icons and default geometry.

use crate::applications::AppState;
use crate::applications::about::About;
use crate::applications::monitor::Monitor;
use crate::applications::settings::Settings;
use crate::applications::terminal::Terminal;
use crate::graphics::colors::{self, Color};
use crate::graphics::primitives::{
    circle_filled, circle_ring, fill_rect, fill_rounded_rect, line, stroke_rounded_rect,
};
use crate::graphics::{Canvas, Rect};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppId {
    Terminal,
    Monitor,
    Settings,
    About,
}

impl AppId {
    pub const ALL: [AppId; 4] = [
        AppId::Terminal,
        AppId::Monitor,
        AppId::Settings,
        AppId::About,
    ];

    pub fn title(self) -> &'static str {
        match self {
            AppId::Terminal => "Noor Terminal",
            AppId::Monitor => "Noor Monitor",
            AppId::Settings => "Noor Settings",
            AppId::About => "About NoorOS",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            AppId::Terminal => "Terminal",
            AppId::Monitor => "Monitor",
            AppId::Settings => "Settings",
            AppId::About => "About",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            AppId::Terminal => "Command-line shell with system commands",
            AppId::Monitor => "Live kernel and hardware information",
            AppId::Settings => "Informational system configuration",
            AppId::About => "Project, version and credits",
        }
    }

    pub fn accent(self) -> Color {
        match self {
            AppId::Terminal => colors::GOLD,
            AppId::Monitor => colors::BLUE,
            AppId::Settings => colors::VIOLET,
            AppId::About => colors::SUCCESS,
        }
    }

    pub fn create_state(self) -> AppState {
        match self {
            AppId::Terminal => AppState::Terminal(Terminal::new()),
            AppId::Monitor => AppState::Monitor(Monitor::new()),
            AppId::Settings => AppState::Settings(Settings::new()),
            AppId::About => AppState::About(About::new()),
        }
    }

    /// Default window rectangle for a screen, cascaded per application so
    /// windows do not stack exactly on top of each other.
    pub fn default_rect(self, screen_w: i32, screen_h: i32) -> Rect {
        let (w, h) = match self {
            AppId::Terminal => (
                (screen_w - 200).clamp(520, 900),
                (screen_h - 250).clamp(320, 560),
            ),
            AppId::Monitor => (
                (screen_w - 160).clamp(560, 920),
                (screen_h - 160).clamp(360, 620),
            ),
            AppId::Settings => (
                (screen_w - 190).clamp(540, 840),
                (screen_h - 250).clamp(340, 540),
            ),
            AppId::About => (
                (screen_w - 300).clamp(440, 660),
                (screen_h - 260).clamp(340, 540),
            ),
        };
        let cascade = match self {
            AppId::Terminal => (0, 0),
            AppId::Monitor => (28, 20),
            AppId::Settings => (56, 40),
            AppId::About => (84, 60),
        };
        let x = ((screen_w - w) / 2 + cascade.0).clamp(24, (screen_w - w - 24).max(24));
        let y = ((screen_h - h) / 2 - 18 + cascade.1).clamp(40, (screen_h - h - 100).max(40));
        Rect::new(x, y, w, h)
    }

    /// Draws the application icon inside `rect` (typically 24x24).
    pub fn draw_icon(self, canvas: &mut Canvas, rect: Rect) {
        let accent = self.accent();
        match self {
            AppId::Terminal => {
                fill_rounded_rect(canvas, rect.x, rect.y, rect.w, rect.h, 5, colors::SURFACE_2);
                stroke_rounded_rect(canvas, rect.x, rect.y, rect.w, rect.h, 5, 1, accent);
                let cx = rect.x + rect.w / 2;
                let cy = rect.y + rect.h / 2;
                line(canvas, cx - 5, cy - 4, cx, cy, accent);
                line(canvas, cx, cy, cx - 5, cy + 4, accent);
                fill_rect(canvas, cx + 2, cy + 4, 6, 2, accent);
            }
            AppId::Monitor => {
                fill_rounded_rect(canvas, rect.x, rect.y, rect.w, rect.h, 5, colors::SURFACE_2);
                stroke_rounded_rect(canvas, rect.x, rect.y, rect.w, rect.h, 5, 1, accent);
                let base = rect.y + rect.h - 6;
                for (i, height) in [5, 9, 13].iter().enumerate() {
                    let x = rect.x + 5 + i as i32 * 5;
                    fill_rect(
                        canvas,
                        x,
                        base - height,
                        3,
                        *height,
                        if i == 2 { accent } else { colors::BLUE },
                    );
                }
            }
            AppId::Settings => {
                fill_rounded_rect(canvas, rect.x, rect.y, rect.w, rect.h, 5, colors::SURFACE_2);
                stroke_rounded_rect(canvas, rect.x, rect.y, rect.w, rect.h, 5, 1, accent);
                let cx = rect.x + rect.w / 2;
                let cy = rect.y + rect.h / 2;
                circle_ring(canvas, cx, cy, 7, 2, accent);
                for (dx, dy) in [(0, -10), (0, 10), (-10, 0), (10, 0)] {
                    line(canvas, cx + dx / 2, cy + dy / 2, cx + dx, cy + dy, accent);
                }
            }
            AppId::About => {
                fill_rounded_rect(canvas, rect.x, rect.y, rect.w, rect.h, 5, colors::SURFACE_2);
                stroke_rounded_rect(canvas, rect.x, rect.y, rect.w, rect.h, 5, 1, accent);
                let cx = rect.x + rect.w / 2;
                let cy = rect.y + rect.h / 2;
                circle_ring(canvas, cx, cy, 6, 1, accent);
                circle_filled(canvas, cx, cy, 2, accent);
            }
        }
    }

    /// Launcher grid icon (2x2 dots).
    pub fn draw_launcher_icon(canvas: &mut Canvas, rect: Rect) {
        let accent = colors::GOLD;
        fill_rounded_rect(canvas, rect.x, rect.y, rect.w, rect.h, 5, colors::SURFACE_2);
        stroke_rounded_rect(canvas, rect.x, rect.y, rect.w, rect.h, 5, 1, accent);
        let cx = rect.x + rect.w / 2;
        let cy = rect.y + rect.h / 2;
        for (dx, dy) in [(-4, -4), (4, -4), (-4, 4), (4, 4)] {
            fill_rounded_rect(canvas, cx + dx - 2, cy + dy - 2, 4, 4, 1, accent);
        }
    }
}
