//! Noor Terminal - the NoorOS graphical terminal.
//!
//! The terminal owns a scrollback buffer, line editing and history. Command
//! execution is delegated to `shell`; this module is presentation + input.

use alloc::string::String;
use alloc::vec::Vec;

use crate::applications::AppAction;
use crate::drivers::keyboard::{InputEvent, Key};
use crate::graphics::colors::{self, Color};
use crate::graphics::primitives::{fill_rect, fill_rect_alpha, v_gradient};
use crate::graphics::text::{self, draw_text, text_width};
use crate::graphics::{Canvas, Rect};
use crate::shell;

const PROMPT: &str = "noor@nooros:~$ ";
const SCALE: i32 = 2;
const ROW_H: i32 = 20;
const PAD: i32 = 14;
const MAX_LINES: usize = 400;
const MAX_INPUT: usize = 200;

struct Line {
    prompt: Option<String>,
    text: String,
    color: Color,
}

impl Line {
    fn output(text: impl Into<String>, color: Color) -> Self {
        Self {
            prompt: None,
            text: text.into(),
            color,
        }
    }

    fn command(text: impl Into<String>) -> Self {
        Self {
            prompt: Some(String::from(PROMPT)),
            text: text.into(),
            color: colors::TEXT,
        }
    }
}

pub struct Terminal {
    lines: Vec<Line>,
    input: String,
    history: Vec<String>,
    history_index: Option<usize>,
    scroll: usize,
}

impl Terminal {
    pub fn new() -> Self {
        Self {
            lines: alloc::vec![
                Line::output("Noor Terminal 0.1.0", colors::GOLD),
                Line::output("NoorOS - A Light Beyond Computing.", colors::TEXT_DIM),
                Line::output(
                    "Type 'help' for commands, 'about' for project details.",
                    colors::TEXT_DIM,
                ),
                Line::output("", colors::TEXT),
            ],
            input: String::new(),
            history: Vec::new(),
            history_index: None,
            scroll: 0,
        }
    }

    pub fn handle_key(&mut self, event: InputEvent) -> AppAction {
        let ctrl = event.ctrl;
        match event.key {
            Key::Char(c) if ctrl && (c == 'c' || c == 'C') => {
                let mut line = Line::command(core::mem::take(&mut self.input));
                line.color = colors::TEXT_DIM;
                // Append the conventional interrupt marker after the command.
                line.text.push_str("^C");
                self.lines.push(line);
                self.scroll = 0;
            }
            Key::Char('l') | Key::Char('L') if ctrl => {
                self.lines.clear();
                self.scroll = 0;
            }
            Key::Char(c) if !ctrl && !event.alt => {
                if self.input.chars().count() < MAX_INPUT {
                    self.input.push(c);
                }
            }
            Key::Enter => return self.submit(),
            Key::Backspace => {
                self.input.pop();
            }
            Key::Up => {
                if !self.history.is_empty() {
                    let index = match self.history_index {
                        Some(i) => i.saturating_sub(1),
                        None => self.history.len() - 1,
                    };
                    self.history_index = Some(index);
                    self.input = self.history[index].clone();
                }
            }
            Key::Down => {
                if let Some(index) = self.history_index {
                    if index + 1 < self.history.len() {
                        self.history_index = Some(index + 1);
                        self.input = self.history[index + 1].clone();
                    } else {
                        self.history_index = None;
                        self.input.clear();
                    }
                }
            }
            Key::PageUp => self.scroll = self.scroll.saturating_add(8),
            Key::PageDown => self.scroll = self.scroll.saturating_sub(8),
            _ => {}
        }
        AppAction::None
    }

    fn submit(&mut self) -> AppAction {
        let command = core::mem::take(&mut self.input);
        self.lines.push(Line::command(command.clone()));
        if !command.trim().is_empty() {
            self.history.push(command.clone());
            if self.history.len() > 64 {
                self.history.remove(0);
            }
        }
        self.history_index = None;
        self.scroll = 0;

        let output = shell::execute(&command);
        if output.clear {
            self.lines.clear();
        }
        for line in output.lines {
            let color = if output.error {
                colors::DANGER
            } else {
                colors::TEXT
            };
            self.lines.push(Line::output(line, color));
        }
        if self.lines.len() > MAX_LINES {
            let excess = self.lines.len() - MAX_LINES;
            self.lines.drain(0..excess);
        }
        match output.action {
            shell::Action::None => AppAction::None,
            shell::Action::Shutdown => AppAction::Shutdown,
            shell::Action::Reboot => AppAction::Reboot,
        }
    }

    pub fn render(&mut self, canvas: &mut Canvas, rect: Rect, tick: u64, focused: bool) {
        fill_rect(canvas, rect.x, rect.y, rect.w, rect.h, colors::TERMINAL_BG);
        v_gradient(
            canvas,
            rect.x,
            rect.y,
            rect.w,
            40,
            Color::hex(0x0C1320),
            colors::TERMINAL_BG,
        );

        let input_h = ROW_H + PAD;
        let area = Rect::new(
            rect.x + PAD,
            rect.y + PAD,
            (rect.w - PAD * 2).max(10),
            (rect.h - PAD * 2 - input_h).max(ROW_H),
        );
        let rows_visible = (area.h / ROW_H).max(1) as usize;

        // Build display rows (one entry per physical line after wrapping).
        let mut rows: Vec<Row<'_>> = Vec::new();
        for line in &self.lines {
            let prompt_width = line.prompt.as_ref().map_or(0, |p| text_width(p, SCALE));
            let columns = ((area.w - prompt_width) / (8 * SCALE)).max(8) as usize;
            let wrapped = text::wrap(&line.text, columns, 500);
            if wrapped.is_empty() {
                rows.push(Row {
                    prompt: line.prompt.as_deref(),
                    indent: 0,
                    text: "",
                    color: line.color,
                });
                continue;
            }
            for (i, part) in wrapped.into_iter().enumerate() {
                rows.push(Row {
                    prompt: if i == 0 { line.prompt.as_deref() } else { None },
                    indent: prompt_width,
                    text: part,
                    color: line.color,
                });
            }
        }

        let total = rows.len();
        let scroll = self.scroll.min(total.saturating_sub(rows_visible));
        let start = total.saturating_sub(rows_visible + scroll);
        let end = (start + rows_visible).min(total);

        let mut y = area.y;
        for row in &rows[start..end] {
            if let Some(prompt) = row.prompt {
                draw_text(canvas, area.x, y, prompt, SCALE, colors::GOLD);
            }
            draw_text(canvas, area.x + row.indent, y, row.text, SCALE, row.color);
            y += ROW_H;
        }

        if scroll > 0 {
            let note = alloc::format!("[ scrolled back {scroll} lines - PgDn ]");
            draw_text(canvas, area.x, area.bottom() - 18, &note, 1, colors::GOLD);
        }

        // Input line
        let input_y = area.bottom() + 10;
        fill_rect(
            canvas,
            area.x - PAD,
            input_y - 8,
            rect.w - PAD,
            1,
            colors::BORDER,
        );
        draw_text(canvas, area.x, input_y, PROMPT, SCALE, colors::GOLD);

        let prompt_w = text_width(PROMPT, SCALE);
        let available = (area.w - prompt_w).max(64);
        let max_chars = (available / (8 * SCALE)).max(4) as usize;
        let input_chars = self.input.chars().count();
        let shown: String = if input_chars > max_chars {
            self.input.chars().skip(input_chars - max_chars).collect()
        } else {
            self.input.clone()
        };
        draw_text(
            canvas,
            area.x + prompt_w,
            input_y,
            &shown,
            SCALE,
            colors::TEXT,
        );

        // Block cursor, blinking at 2 Hz while the window is focused.
        let cursor_x = area.x + prompt_w + text_width(&shown, SCALE);
        let cursor_visible = focused && (tick / 500) % 2 == 0;
        if cursor_visible {
            fill_rect(
                canvas,
                cursor_x,
                input_y - 2,
                7 * SCALE,
                8 * SCALE + 4,
                colors::GOLD,
            );
            fill_rect(
                canvas,
                cursor_x + 2,
                input_y,
                7 * SCALE - 4,
                8 * SCALE,
                colors::TERMINAL_BG,
            );
        } else if focused {
            fill_rect(
                canvas,
                cursor_x,
                input_y + 8 * SCALE,
                8 * SCALE,
                2,
                colors::GOLD_DEEP,
            );
        }

        // Focus indicator strip
        if focused {
            fill_rect_alpha(
                canvas,
                rect.x,
                rect.y + rect.h - 2,
                rect.w,
                2,
                colors::GOLD,
                120,
            );
        }
        let hint = alloc::format!(
            "{} cols x {} rows  |  linked-list heap {}",
            (area.w / (8 * SCALE)).max(1),
            rows_visible,
            crate::util::bytes(crate::memory::heap_stats().used)
        );
        draw_text(
            canvas,
            rect.x + PAD,
            rect.y + rect.h - 14,
            &hint,
            1,
            Color::hex(0x3D4C66),
        );
    }
}

struct Row<'a> {
    prompt: Option<&'a str>,
    indent: i32,
    text: &'a str,
    color: Color,
}
