//! Noor Desktop event loop and composition.

use alloc::vec::Vec;

use bootloader_api::info::FrameBufferInfo;

use crate::applications::AppAction;
use crate::desktop::app_manager::AppId;
use crate::desktop::launcher::{Launcher, LauncherAction};
use crate::desktop::taskbar;
use crate::desktop::window::Window;
use crate::drivers::keyboard::{self, InputEvent, Key};
use crate::drivers::{framebuffer, power, timer};
use crate::graphics::branding;
use crate::graphics::colors::{self, Color};
use crate::graphics::primitives::*;
use crate::graphics::text::*;
use crate::graphics::{Canvas, Rect};

pub struct Desktop {
    screen_w: i32,
    screen_h: i32,
    windows: Vec<Window>,
    focused: Option<usize>,
    launcher: Launcher,
    tick: u64,
    wallpaper: Vec<u8>,
}

impl Desktop {
    pub fn new(info: FrameBufferInfo) -> Self {
        Self {
            screen_w: info.width as i32,
            screen_h: info.height as i32,
            windows: Vec::new(),
            focused: None,
            launcher: Launcher::new(),
            tick: 0,
            wallpaper: Vec::new(),
        }
    }

    /// Renders the procedural wallpaper once into an off-screen buffer.
    ///
    /// The wallpaper costs seconds under QEMU's software CPU, while a blit is
    /// nearly free; caching keeps the desktop responsive at the 2 Hz blink rate.
    pub fn prepare_wallpaper(&mut self, info: FrameBufferInfo) {
        let mut background = alloc::vec![0u8; info.byte_len];
        {
            let mut canvas = Canvas::new(&mut background, info);
            branding::draw_wallpaper(&mut canvas);
        }
        self.wallpaper = background;
    }

    fn open_app(&mut self, app: AppId) {
        if let Some(index) = self.windows.iter().position(|w| w.app == app) {
            self.focus_index(index);
            serial_println!("[desktop] focused {}", app.title());
            return;
        }
        let window = Window::new(app, self.screen_w, self.screen_h);
        self.windows.push(window);
        self.focused = Some(self.windows.len() - 1);
        serial_println!("[desktop] opened {}", app.title());
    }

    fn focus_index(&mut self, index: usize) {
        if index + 1 == self.windows.len() {
            self.focused = Some(index);
            return;
        }
        let window = self.windows.remove(index);
        self.windows.push(window);
        self.focused = Some(self.windows.len() - 1);
    }

    fn focus_next(&mut self) {
        if self.windows.len() < 2 {
            return;
        }
        let current = self.focused.unwrap_or(0);
        let next = (current + 1) % self.windows.len();
        self.focus_index(next);
    }

    fn close_focused(&mut self) {
        if let Some(index) = self.focused
            && index < self.windows.len()
        {
            let window = self.windows.remove(index);
            serial_println!("[desktop] closed {}", window.app.title());
        }
        self.focused = if self.windows.is_empty() {
            None
        } else {
            Some(self.windows.len() - 1)
        };
    }

    fn focused_app(&self) -> Option<AppId> {
        self.focused
            .and_then(|i| self.windows.get(i))
            .map(|w| w.app)
    }

    pub fn handle_key(&mut self, event: InputEvent) -> AppAction {
        if self.launcher.open {
            match self.launcher.handle_key(event) {
                LauncherAction::None => {}
                LauncherAction::Close => self.launcher.close(),
                LauncherAction::Launch(app) => {
                    self.launcher.close();
                    self.open_app(app);
                }
            }
            return AppAction::None;
        }

        match event.key {
            Key::F(1) => self.launcher.toggle(),
            Key::F(2) => self.open_app(AppId::Terminal),
            Key::F(3) => self.open_app(AppId::Monitor),
            Key::F(4) => self.open_app(AppId::Settings),
            Key::F(5) => self.open_app(AppId::About),
            Key::Tab if event.alt || event.ctrl => self.focus_next(),
            Key::Escape => self.close_focused(),
            Key::Char('w') | Key::Char('W') if event.ctrl => self.close_focused(),
            _ => {
                if let Some(index) = self.focused {
                    let action = self.windows[index].handle_key(event);
                    if action == AppAction::Close {
                        self.close_focused();
                    }
                    return action;
                }
            }
        }
        AppAction::None
    }

    pub fn render(&mut self, canvas: &mut Canvas) {
        if self.wallpaper.is_empty() {
            branding::draw_wallpaper(canvas);
        } else {
            canvas.blit(&self.wallpaper);
        }

        // Desktop keyboard hints, shown when no window covers the area.
        let hint = "F1 Launcher     F2 Terminal     F3 Monitor     F4 Settings     F5 About     Alt+Tab Switch     Ctrl+W Close";
        draw_text(
            canvas,
            30,
            self.screen_h - 98,
            hint,
            1,
            Color::hex(0x46566F),
        );

        if self.windows.is_empty() {
            self.draw_welcome(canvas);
        }

        let tick = self.tick;
        for index in 0..self.windows.len() {
            let focused = self.focused == Some(index);
            self.windows[index].render(canvas, tick, focused);
        }

        // The launcher is modal but the desktop chrome stays visible around it,
        // so the user keeps the clock and running-app context while choosing.
        if self.launcher.open {
            self.launcher.draw(canvas, self.screen_w, self.screen_h);
        }

        taskbar::draw_top_bar(canvas, self.screen_w);
        let running: Vec<AppId> = self.windows.iter().map(|w| w.app).collect();
        taskbar::draw(
            canvas,
            self.screen_w,
            self.screen_h,
            &running,
            self.focused_app(),
            self.launcher.open,
        );
    }

    fn draw_welcome(&self, canvas: &mut Canvas) {
        let w = (self.screen_w - 160).min(540);
        let h = 312;
        let x = (self.screen_w - w) / 2;
        let y = (self.screen_h - h) / 2 - 46;
        let rect = Rect::new(x, y, w, h);
        panel(canvas, rect, colors::SURFACE, 208, 18, colors::GOLD, 70);

        let cx = x + w / 2;
        branding::draw_logo(canvas, cx, y + 52, 27, colors::GOLD);
        draw_text_centered(canvas, cx, y + 92, "Welcome to NoorOS", 3, colors::TEXT);
        draw_text_centered(
            canvas,
            cx,
            y + 128,
            "A Light Beyond Computing.",
            1,
            colors::TEXT_DIM,
        );

        fill_rect(canvas, x + 40, y + 152, w - 80, 1, colors::BORDER);

        let rows = [
            ("F1", "Open Noor Launcher"),
            ("F2", "Launch Noor Terminal"),
            ("F3", "Open Noor Monitor"),
            ("Alt+Tab", "Switch windows"),
        ];
        let mut row_y = y + 168;
        for (key, label) in rows {
            let key_w = text_width(key, 1) + 14;
            let row_x = cx - 80;
            fill_rounded_rect(canvas, row_x, row_y - 1, key_w, 16, 4, Color::hex(0x0B1120));
            stroke_rounded_rect(canvas, row_x, row_y - 1, key_w, 16, 4, 1, colors::BORDER);
            draw_text(canvas, row_x + 7, row_y + 3, key, 1, colors::GOLD);
            draw_text(
                canvas,
                row_x + key_w + 12,
                row_y + 3,
                label,
                1,
                colors::TEXT_DIM,
            );
            row_y += 24;
        }

        draw_text_centered(
            canvas,
            cx,
            y + h - 24,
            "NoorOS 0.1.0  -  Noor Kernel running  -  x86_64",
            1,
            Color::hex(0x5A6B86),
        );
    }
}

pub fn run(mut desktop: Desktop) -> ! {
    if let Some(info) = framebuffer::info() {
        desktop.prepare_wallpaper(info);
    }
    let mut last_minute = u64::MAX;

    loop {
        let ticks = timer::ticks();
        let mut dirty = false;

        // The status bars show a minute-resolution clock, so a repaint per
        // minute is enough. Rendering is deliberately event-driven: a full
        // frame costs hundreds of milliseconds under QEMU's software CPU, and
        // repainting on a fast blink timer would leave the screen permanently
        // half-drawn.
        let minute = ticks / timer::TICK_HZ / 60;
        if minute != last_minute {
            last_minute = minute;
            desktop.tick = ticks;
            dirty = true;
        }

        while let Some(event) = keyboard::read() {
            desktop.tick = ticks;
            match desktop.handle_key(event) {
                AppAction::Shutdown => power::shutdown(),
                AppAction::Reboot => power::reboot(),
                AppAction::Close => desktop.close_focused(),
                AppAction::None => {}
            }
            dirty = true;
        }

        if dirty {
            let _ = framebuffer::with(|canvas| desktop.render(canvas));
        }

        x86_64::instructions::hlt();
    }
}
