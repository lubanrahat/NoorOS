//! Noor Desktop event loop and composition.
//!
//! Input arrives from two drivers and is routed in z-order:
//!
//! * the keyboard queue, which the taskbar/launcher/window manager interpret,
//! * the mouse, whose motion is coalesced to the newest position and whose
//!   button edges are queued so a click is never lost.
//!
//! Rendering has two paths: a full repaint after any structural change, and a
//! cursor-only repaint while the pointer moves. The latter is what makes the
//! pointer usable under QEMU's software CPU, where a full frame costs hundreds
//! of milliseconds.

use alloc::vec::Vec;

use bootloader_api::info::FrameBufferInfo;

use crate::applications::AppAction;
use crate::desktop::app_manager::AppId;
use crate::desktop::cursor::Cursor;
use crate::desktop::launcher::{Launcher, LauncherAction, LauncherHit};
use crate::desktop::taskbar::{self, TOP_BAR_H, TaskbarItem};
use crate::desktop::window::{TITLE_H, Window};
use crate::drivers::keyboard::{self, InputEvent, Key};
use crate::drivers::mouse::{self, Button, ButtonEvent, Pointer};
use crate::drivers::{framebuffer, power, timer};
use crate::graphics::branding;
use crate::graphics::colors::{self, Color};
use crate::graphics::primitives::*;
use crate::graphics::text::*;
use crate::graphics::{Canvas, Rect};

/// How much of the screen a pointer change invalidated.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Repaint {
    None,
    Cursor,
    Full,
}

/// An in-progress title-bar drag.
#[derive(Clone, Copy)]
struct Drag {
    app: AppId,
    /// Pointer offset from the window's top-left corner when the drag started.
    grab_dx: i32,
    grab_dy: i32,
}

pub struct Desktop {
    screen_w: i32,
    screen_h: i32,
    windows: Vec<Window>,
    focused: Option<usize>,
    launcher: Launcher,
    pointer: Pointer,
    drag: Option<Drag>,
    cursor: Cursor,
    tick: u64,
    wallpaper: Vec<u8>,
}

impl Desktop {
    pub fn new(info: FrameBufferInfo) -> Self {
        let (screen_w, screen_h) = (info.width as i32, info.height as i32);
        Self {
            screen_w,
            screen_h,
            windows: Vec::new(),
            focused: None,
            launcher: Launcher::new(),
            pointer: Pointer::new(screen_w / 2, screen_h / 2),
            drag: None,
            cursor: Cursor::new(),
            tick: 0,
            wallpaper: Vec::new(),
        }
    }

    /// Renders the procedural wallpaper once into an off-screen buffer.
    ///
    /// The wallpaper costs seconds under QEMU's software CPU, while a blit is
    /// nearly free; caching keeps the desktop responsive.
    pub fn prepare_wallpaper(&mut self, info: FrameBufferInfo) {
        let mut background = alloc::vec![0u8; info.byte_len];
        {
            let mut canvas = Canvas::new(&mut background, info);
            branding::draw_wallpaper(&mut canvas);
        }
        self.wallpaper = background;
    }

    pub fn pointer(&self) -> Pointer {
        self.pointer
    }

    /// Applies a new pointer snapshot.
    ///
    /// Returns whether the caller should repaint the whole desktop or only the
    /// cursor. While a title bar is held, the window follows the pointer, which
    /// necessarily invalidates the frame.
    pub fn set_pointer(&mut self, pointer: Pointer) -> Repaint {
        if pointer.x == self.pointer.x && pointer.y == self.pointer.y {
            self.pointer = pointer;
            return Repaint::None;
        }
        self.pointer = pointer;

        let Some(drag) = self.drag else {
            return Repaint::Cursor;
        };
        let Some(index) = self.windows.iter().position(|w| w.app == drag.app) else {
            self.drag = None;
            return Repaint::Cursor;
        };

        let width = self.windows[index].rect.w;
        // Keep at least 120 px of the window on screen horizontally and the
        // whole title bar visible vertically, so it can always be grabbed back.
        let max_x = (self.screen_w - 120).max(120 - width);
        let max_y = (self.screen_h - TITLE_H - 8).max(TOP_BAR_H);
        let window = &mut self.windows[index];
        let (old_x, old_y) = (window.rect.x, window.rect.y);
        window.rect.x = (pointer.x - drag.grab_dx).clamp(120 - width, max_x);
        window.rect.y = (pointer.y - drag.grab_dy).clamp(TOP_BAR_H, max_y);

        // At a screen edge the window may already be clamped, while the
        // pointer itself still moved. Only the cursor needs repainting then.
        if window.rect.x == old_x && window.rect.y == old_y {
            Repaint::Cursor
        } else {
            Repaint::Full
        }
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

    fn close_index(&mut self, index: usize) {
        if index >= self.windows.len() {
            return;
        }
        let window = self.windows.remove(index);
        serial_println!("[desktop] closed {}", window.app.title());
        if self.drag.map(|d| d.app) == Some(window.app) {
            self.drag = None;
        }
        self.focused = if self.windows.is_empty() {
            None
        } else {
            Some(self.windows.len() - 1)
        };
    }

    fn close_focused(&mut self) {
        if let Some(index) = self.focused.filter(|&i| i < self.windows.len()) {
            self.close_index(index);
        }
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

    /// Routes one mouse button edge.
    ///
    /// Hit-testing follows the same order as rendering: taskbar and top bar are
    /// above everything, the launcher is modal, then windows are tested from the
    /// top of the z-order down.
    pub fn handle_mouse(&mut self, event: ButtonEvent) -> AppAction {
        // Only the left button acts for now; right and middle are tracked by the
        // driver and reserved for future context menus.
        if event.button != Button::Left {
            return AppAction::None;
        }
        if !event.pressed {
            self.drag = None;
            return AppAction::None;
        }

        // A fresh press supersedes any drag whose release edge was never
        // delivered (for example because the event queue overflowed).
        self.drag = None;

        let (x, y) = (event.pointer.x, event.pointer.y);

        if let Some(item) = taskbar::item_at(self.screen_w, self.screen_h, x, y) {
            match item {
                TaskbarItem::Launcher => self.launcher.toggle(),
                TaskbarItem::App(app) => {
                    self.launcher.close();
                    self.open_app(app);
                }
            }
            return AppAction::None;
        }

        if self.launcher.open {
            match self.launcher.hit(x, y, self.screen_w, self.screen_h) {
                LauncherHit::Row(index) => {
                    let app = AppId::ALL[index];
                    self.launcher.close();
                    self.open_app(app);
                }
                LauncherHit::Outside => self.launcher.close(),
                LauncherHit::None => {}
            }
            return AppAction::None;
        }

        for index in (0..self.windows.len()).rev() {
            if !self.windows[index].contains(x, y) {
                continue;
            }
            if self.windows[index].close_rect().contains(x, y) {
                self.close_index(index);
                return AppAction::None;
            }
            if self.windows[index].title_rect().contains(x, y) {
                // Bring to front first, then remember the grab offset relative
                // to the window's new position.
                self.focus_index(index);
                if let Some(window) = self.windows.last() {
                    self.drag = Some(Drag {
                        app: window.app,
                        grab_dx: x - window.rect.x,
                        grab_dy: y - window.rect.y,
                    });
                }
                return AppAction::None;
            }
            // A click anywhere else in the window focuses it.
            self.focus_index(index);
            return AppAction::None;
        }

        AppAction::None
    }

    pub fn render(&mut self, canvas: &mut Canvas) {
        // The cursor owns a copy of the pixels it covers; restore them before
        // repainting so the new frame starts from clean background.
        self.cursor.erase(canvas);

        if self.wallpaper.is_empty() {
            branding::draw_wallpaper(canvas);
        } else {
            canvas.blit(&self.wallpaper);
        }

        // Desktop mouse/keyboard hints, shown when no window covers the area.
        let hint = "Click to focus   Drag a title bar   F1 Launcher   F2-F5 Apps   Alt+Tab Switch   Esc Close";
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

        // Last, so the pointer is on top of every panel and the modal scrim.
        self.cursor.move_to(canvas, self.pointer.x, self.pointer.y);
    }

    /// Repaints only the cursor. Used while the pointer moves with no window
    /// under it, which is the common case and must stay cheap.
    pub fn render_cursor(&mut self, canvas: &mut Canvas) {
        self.cursor.move_to(canvas, self.pointer.x, self.pointer.y);
    }

    fn draw_welcome(&self, canvas: &mut Canvas) {
        let w = (self.screen_w - 160).min(540);
        let h = 336;
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
            ("Mouse", "Click icons, drag title bars"),
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
        let mut full = false;
        let mut cursor = false;

        // The status bars show a minute-resolution clock, so a repaint per
        // minute is enough. Rendering is deliberately event-driven: a full
        // frame costs hundreds of milliseconds under QEMU's software CPU, and
        // repainting on a fast blink timer would leave the screen permanently
        // half-drawn.
        let minute = ticks / timer::TICK_HZ / 60;
        if minute != last_minute {
            last_minute = minute;
            desktop.tick = ticks;
            full = true;
        }

        while let Some(event) = keyboard::read() {
            desktop.tick = ticks;
            match desktop.handle_key(event) {
                AppAction::Shutdown => power::shutdown(),
                AppAction::Reboot => power::reboot(),
                AppAction::Close => desktop.close_focused(),
                AppAction::None => {}
            }
            full = true;
        }

        // Motion is coalesced: the newest position wins, so a slow repaint can
        // never replay a backlog of stale cursor positions.
        let pointer = mouse::pointer();
        if pointer != desktop.pointer() {
            match desktop.set_pointer(pointer) {
                Repaint::Full => full = true,
                Repaint::Cursor => cursor = true,
                Repaint::None => {}
            }
        }

        while let Some(event) = mouse::read_event() {
            desktop.tick = ticks;
            match desktop.handle_mouse(event) {
                AppAction::Shutdown => power::shutdown(),
                AppAction::Reboot => power::reboot(),
                _ => {}
            }
            full = true;
        }

        if full {
            let _ = framebuffer::with(|canvas| desktop.render(canvas));
        } else if cursor {
            let _ = framebuffer::with(|canvas| desktop.render_cursor(canvas));
        }

        x86_64::instructions::hlt();
    }
}
