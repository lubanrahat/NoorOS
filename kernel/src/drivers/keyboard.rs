//! PS/2 keyboard driver and input queue.
//!
//! The IRQ handler pushes scancodes here; the desktop event loop drains decoded
//! [`InputEvent`]s. The queue is a fixed-size ring buffer so the interrupt
//! handler never allocates.

use core::sync::atomic::{AtomicU64, Ordering};

use pc_keyboard::{HandleControl, KeyCode, KeyState, PS2Keyboard, ScancodeSet1, layouts::Us104Key};
use spin::Mutex;

/// A decoded key, independent of the physical keyboard layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Backspace,
    Tab,
    Escape,
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    Delete,
    F(u8),
    Unknown,
}

#[derive(Clone, Copy, Debug)]
pub struct InputEvent {
    pub key: Key,
    /// Modifier snapshot at the time of the event. The desktop routes Ctrl/Alt
    /// shortcuts itself; `shift` is exposed for callers that need it.
    #[allow(dead_code)]
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

const QUEUE_LEN: usize = 64;

struct KeyboardState {
    keyboard: Option<PS2Keyboard<Us104Key, ScancodeSet1>>,
    shift: bool,
    ctrl: bool,
    alt: bool,
    queue: [Option<InputEvent>; QUEUE_LEN],
    head: usize,
    tail: usize,
}

impl KeyboardState {
    const fn new() -> Self {
        Self {
            keyboard: None,
            shift: false,
            ctrl: false,
            alt: false,
            queue: [None; QUEUE_LEN],
            head: 0,
            tail: 0,
        }
    }

    fn push(&mut self, event: InputEvent) {
        let next = (self.tail + 1) % QUEUE_LEN;
        if next == self.head {
            // Queue full: drop the newest event rather than overwrite input the
            // desktop has not consumed yet.
            DROPPED.fetch_add(1, Ordering::Relaxed);
            return;
        }
        self.queue[self.tail] = Some(event);
        self.tail = next;
    }

    fn pop(&mut self) -> Option<InputEvent> {
        if self.head == self.tail {
            return None;
        }
        let event = self.queue[self.head].take();
        self.head = (self.head + 1) % QUEUE_LEN;
        event
    }
}

static STATE: Mutex<KeyboardState> = Mutex::new(KeyboardState::new());
static INTERRUPTS: AtomicU64 = AtomicU64::new(0);
static DROPPED: AtomicU64 = AtomicU64::new(0);

pub fn init() {
    let mut state = STATE.lock();
    state.keyboard = Some(PS2Keyboard::new(
        ScancodeSet1::new(),
        Us104Key,
        HandleControl::Ignore,
    ));
    state.head = 0;
    state.tail = 0;
    state.shift = false;
    state.ctrl = false;
    state.alt = false;
}

/// Called from the keyboard IRQ with the byte read from port 0x60.
pub fn on_scancode(byte: u8) {
    INTERRUPTS.fetch_add(1, Ordering::Relaxed);
    let mut state = STATE.lock();
    let Some(keyboard) = state.keyboard.as_mut() else {
        return;
    };
    let Ok(Some(event)) = keyboard.add_byte(byte) else {
        return;
    };

    // Track modifier state first: it must be current when the key is decoded.
    match event.code {
        KeyCode::LShift | KeyCode::RShift => {
            state.shift = event.state != KeyState::Up;
            return;
        }
        KeyCode::LControl | KeyCode::RControl | KeyCode::RControl2 => {
            state.ctrl = event.state != KeyState::Up;
            return;
        }
        KeyCode::LAlt | KeyCode::RAltGr | KeyCode::RAlt2 => {
            state.alt = event.state != KeyState::Up;
            return;
        }
        _ => {}
    }

    if event.state == KeyState::Up {
        return;
    }

    let shift = state.shift;
    let ctrl = state.ctrl;
    let alt = state.alt;
    let key = decode(event.code, shift);
    if key == Key::Unknown {
        return;
    }
    state.push(InputEvent {
        key,
        shift,
        ctrl,
        alt,
    });
}

/// Drains one event. Interrupts are masked while the queue lock is held so the
/// IRQ handler can never interleave with this critical section.
pub fn read() -> Option<InputEvent> {
    x86_64::instructions::interrupts::without_interrupts(|| STATE.lock().pop())
}

pub fn interrupt_count() -> u64 {
    INTERRUPTS.load(Ordering::Relaxed)
}

/// Number of key events discarded because the 64-entry queue was full.
pub fn dropped() -> u64 {
    DROPPED.load(Ordering::Relaxed)
}

fn decode(code: KeyCode, shift: bool) -> Key {
    use KeyCode::*;
    let letter = |c: char| {
        if shift {
            Key::Char(c.to_ascii_uppercase())
        } else {
            Key::Char(c)
        }
    };
    let punct = |plain: char, shifted: char| Key::Char(if shift { shifted } else { plain });
    match code {
        A => letter('a'),
        B => letter('b'),
        C => letter('c'),
        D => letter('d'),
        E => letter('e'),
        F => letter('f'),
        G => letter('g'),
        H => letter('h'),
        I => letter('i'),
        J => letter('j'),
        K => letter('k'),
        L => letter('l'),
        M => letter('m'),
        N => letter('n'),
        O => letter('o'),
        P => letter('p'),
        Q => letter('q'),
        R => letter('r'),
        S => letter('s'),
        T => letter('t'),
        U => letter('u'),
        V => letter('v'),
        W => letter('w'),
        X => letter('x'),
        Y => letter('y'),
        Z => letter('z'),
        Key0 => Key::Char(if shift { ')' } else { '0' }),
        Key1 => Key::Char(if shift { '!' } else { '1' }),
        Key2 => Key::Char(if shift { '@' } else { '2' }),
        Key3 => Key::Char(if shift { '#' } else { '3' }),
        Key4 => Key::Char(if shift { '$' } else { '4' }),
        Key5 => Key::Char(if shift { '%' } else { '5' }),
        Key6 => Key::Char(if shift { '^' } else { '6' }),
        Key7 => Key::Char(if shift { '&' } else { '7' }),
        Key8 => Key::Char(if shift { '*' } else { '8' }),
        Key9 => Key::Char(if shift { '(' } else { '9' }),
        Numpad0 => Key::Char('0'),
        Numpad1 => Key::Char('1'),
        Numpad2 => Key::Char('2'),
        Numpad3 => Key::Char('3'),
        Numpad4 => Key::Char('4'),
        Numpad5 => Key::Char('5'),
        Numpad6 => Key::Char('6'),
        Numpad7 => Key::Char('7'),
        Numpad8 => Key::Char('8'),
        Numpad9 => Key::Char('9'),
        NumpadAdd => Key::Char('+'),
        NumpadSubtract => Key::Char('-'),
        NumpadMultiply => Key::Char('*'),
        NumpadDivide => Key::Char('/'),
        NumpadPeriod => Key::Char('.'),
        Spacebar => Key::Char(' '),
        OemMinus => punct('-', '_'),
        OemPlus => punct('=', '+'),
        Oem1 => punct(';', ':'),
        Oem3 => punct('\'', '"'),
        OemComma => punct(',', '<'),
        OemPeriod => punct('.', '>'),
        Oem2 => punct('/', '?'),
        Oem4 => punct('[', '{'),
        Oem6 => punct(']', '}'),
        Oem5 => punct('\\', '|'),
        Oem7 => punct('`', '~'),
        Return | NumpadEnter => Key::Enter,
        Backspace => Key::Backspace,
        Tab => Key::Tab,
        Escape => Key::Escape,
        ArrowUp => Key::Up,
        ArrowDown => Key::Down,
        ArrowLeft => Key::Left,
        ArrowRight => Key::Right,
        PageUp => Key::PageUp,
        PageDown => Key::PageDown,
        Home => Key::Home,
        End => Key::End,
        Delete => Key::Delete,
        F1 => Key::F(1),
        F2 => Key::F(2),
        F3 => Key::F(3),
        F4 => Key::F(4),
        F5 => Key::F(5),
        F6 => Key::F(6),
        F7 => Key::F(7),
        F8 => Key::F(8),
        F9 => Key::F(9),
        F10 => Key::F(10),
        F11 => Key::F(11),
        F12 => Key::F(12),
        _ => Key::Unknown,
    }
}
