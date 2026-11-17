//! PS/2 mouse driver (the 8042 auxiliary device, IRQ12).
//!
//! The controller is programmed once at boot; after that the IRQ12 handler
//! feeds raw bytes into a three-byte packet parser. Two pieces of state leave
//! this module:
//!
//! * [`pointer`] - the newest position and button snapshot. Motion is **not**
//!   queued: only the latest position matters, so a slow frame can never make
//!   the cursor crawl through a backlog of stale intermediate positions.
//! * [`read_event`] - queued button *transitions* (press/release). These are
//!   never coalesced, because losing a click would lose a user action.
//!
//! The handler only reads a port and updates a fixed-size struct: it never
//! allocates and never blocks.

use core::sync::atomic::{AtomicU64, Ordering};

use spin::Mutex;
use x86_64::instructions::interrupts::without_interrupts;
use x86_64::instructions::port::Port;

// 8042 controller ports.
const STATUS_PORT: u16 = 0x64;
const DATA_PORT: u16 = 0x60;

// Controller commands.
const CMD_READ_CONFIG: u8 = 0x20;
const CMD_WRITE_CONFIG: u8 = 0x60;
const CMD_ENABLE_AUX: u8 = 0xA8;
const CMD_WRITE_AUX: u8 = 0xD4;

// Auxiliary device commands.
const AUX_DISABLE_REPORTING: u8 = 0xF5;
const AUX_SET_DEFAULTS: u8 = 0xF6;
const AUX_ENABLE_REPORTING: u8 = 0xF4;

const ACK: u8 = 0xFA;
const RESEND: u8 = 0xFE;

/// Bounded polling: firmware may be missing the auxiliary device entirely, and
/// boot must not hang on a port that never answers.
const IO_TIMEOUT: usize = 100_000;

/// Button transitions the desktop has not consumed yet.
const EVENT_QUEUE: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Buttons {
    pub left: bool,
    pub right: bool,
    pub middle: bool,
}

/// The pointer position in framebuffer pixels plus the current buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pointer {
    pub x: i32,
    pub y: i32,
    pub buttons: Buttons,
}

impl Pointer {
    pub const fn new(x: i32, y: i32) -> Self {
        Self {
            x,
            y,
            buttons: Buttons {
                left: false,
                right: false,
                middle: false,
            },
        }
    }
}

/// A button edge, with the pointer position at the moment it happened.
#[derive(Clone, Copy, Debug)]
pub struct ButtonEvent {
    pub button: Button,
    pub pressed: bool,
    pub pointer: Pointer,
}

struct MouseState {
    pointer: Pointer,
    width: i32,
    height: i32,
    /// The packet being assembled, and how many bytes of it are valid.
    packet: [u8; 3],
    index: usize,
    present: bool,
    queue: [Option<ButtonEvent>; EVENT_QUEUE],
    head: usize,
    tail: usize,
}

impl MouseState {
    const fn new() -> Self {
        Self {
            pointer: Pointer::new(0, 0),
            width: 0,
            height: 0,
            packet: [0; 3],
            index: 0,
            present: false,
            queue: [None; EVENT_QUEUE],
            head: 0,
            tail: 0,
        }
    }

    fn push(&mut self, event: ButtonEvent) {
        let next = (self.tail + 1) % EVENT_QUEUE;
        if next == self.head {
            // The desktop drains this every loop iteration; a full queue means
            // something is badly wrong. Drop the newest rather than corrupting
            // the ring.
            return;
        }
        self.queue[self.tail] = Some(event);
        self.tail = next;
    }

    fn pop(&mut self) -> Option<ButtonEvent> {
        if self.head == self.tail {
            return None;
        }
        let event = self.queue[self.head].take();
        self.head = (self.head + 1) % EVENT_QUEUE;
        event
    }

    /// Applies one complete packet.
    fn apply(&mut self, packet: [u8; 3]) {
        let flags = packet[0];

        // Bits 6 and 7 report that a delta overflowed the 9-bit range. Only
        // that axis is unusable; the other axis and the button bits are still
        // valid, so discard the overflowing displacement instead of the whole
        // packet.
        let x_overflow = flags & 0x40 != 0;
        let y_overflow = flags & 0x80 != 0;

        // Deltas are 9-bit two's-complement values: the low eight bits are in
        // the packet and the ninth is the sign bit in `flags`. Reading the low
        // byte as `i8` would use bit 7 as the sign, which is wrong for every
        // value outside -128..=127. Subtracting 256 when the flag sign bit is
        // set reconstructs the full range.
        let dx = if x_overflow {
            0
        } else {
            packet[1] as i32 - (((flags & 0x10) as i32) << 4)
        };
        let dy = if y_overflow {
            0
        } else {
            packet[2] as i32 - (((flags & 0x20) as i32) << 3)
        };

        let buttons = Buttons {
            left: flags & 0x01 != 0,
            right: flags & 0x02 != 0,
            middle: flags & 0x04 != 0,
        };
        let previous = self.pointer.buttons;

        // PS/2 Y grows upwards; screen Y grows downwards.
        self.pointer.x = (self.pointer.x + dx).clamp(0, (self.width - 1).max(0));
        self.pointer.y = (self.pointer.y - dy).clamp(0, (self.height - 1).max(0));
        self.pointer.buttons = buttons;

        for (button, was, is) in [
            (Button::Left, previous.left, buttons.left),
            (Button::Right, previous.right, buttons.right),
            (Button::Middle, previous.middle, buttons.middle),
        ] {
            if was != is {
                self.push(ButtonEvent {
                    button,
                    pressed: is,
                    pointer: self.pointer,
                });
            }
        }
    }
}

static STATE: Mutex<MouseState> = Mutex::new(MouseState::new());
static INTERRUPTS: AtomicU64 = AtomicU64::new(0);

// --- 8042 helpers -----------------------------------------------------------

fn read_status() -> u8 {
    // SAFETY: reading the controller status register has no side effects.
    unsafe { Port::<u8>::new(STATUS_PORT).read() }
}

fn read_data() -> u8 {
    // SAFETY: only called when the output buffer is known to be full.
    unsafe { Port::<u8>::new(DATA_PORT).read() }
}

/// Waits until the controller can accept a command or data byte.
fn wait_input_clear() -> bool {
    for _ in 0..IO_TIMEOUT {
        // Bit 1 = input buffer full.
        if read_status() & 0x02 == 0 {
            return true;
        }
    }
    false
}

/// Waits for one byte from the controller itself (not the auxiliary device).
///
/// The config byte returned by `0x20` has the status "auxiliary data" bit
/// clear. A mouse packet that raced with the command is discarded rather than
/// misread as the reply.
fn wait_controller_output() -> Option<u8> {
    for _ in 0..IO_TIMEOUT {
        let status = read_status();
        if status & 0x01 == 0 {
            continue;
        }
        let byte = read_data();
        if status & 0x20 != 0 || status & 0xC0 != 0 {
            continue;
        }
        return Some(byte);
    }
    None
}

/// Sends a byte to the controller's command register.
fn command(byte: u8) -> bool {
    if !wait_input_clear() {
        return false;
    }
    // SAFETY: port 0x64 is the 8042 command register.
    unsafe { Port::<u8>::new(STATUS_PORT).write(byte) };
    true
}

/// Sends a byte to the controller's data register.
fn write_data(byte: u8) -> bool {
    if !wait_input_clear() {
        return false;
    }
    // SAFETY: port 0x60 is the 8042 data register.
    unsafe { Port::<u8>::new(DATA_PORT).write(byte) };
    true
}

/// Drains stale bytes (leftover scancodes, command responses) so the next
/// read cannot mistake them for the reply it is waiting for.
fn flush_output() {
    for _ in 0..64 {
        if read_status() & 0x01 == 0 {
            break;
        }
        let _ = read_data();
    }
}

/// The reply to an auxiliary-device command.
enum Reply {
    Ack,
    Resend,
    /// No reply, or something that is not a device reply.
    Invalid,
}

/// Waits for the auxiliary device's reply to the last command.
fn wait_reply() -> Reply {
    for _ in 0..IO_TIMEOUT {
        let status = read_status();
        if status & 0x01 == 0 {
            continue;
        }
        let byte = read_data();
        // Bit 5 says the byte came from the auxiliary port. Anything else is a
        // keyboard byte or a controller response and must not be mistaken for
        // our device's acknowledgement.
        if status & 0x20 == 0 {
            continue;
        }
        return match byte {
            ACK => Reply::Ack,
            RESEND => Reply::Resend,
            _ => Reply::Invalid,
        };
    }
    Reply::Invalid
}

/// Sends a byte to the mouse itself, via the controller's auxiliary channel.
///
/// The device may answer `RESEND` when a byte was corrupted; the protocol
/// requires the command to be sent again, up to a small bounded retry count.
fn aux_command(byte: u8) -> bool {
    for _ in 0..3 {
        if !command(CMD_WRITE_AUX) || !write_data(byte) {
            return false;
        }
        match wait_reply() {
            Reply::Ack => return true,
            Reply::Resend => continue,
            Reply::Invalid => return false,
        }
    }
    false
}

/// Programs the controller and the mouse. Returns whether a device answered.
fn configure() -> bool {
    flush_output();

    if !command(CMD_ENABLE_AUX) {
        return false;
    }

    // Read the controller configuration byte, enable the auxiliary interrupt
    // (bit 1) and make sure the auxiliary clock is not disabled (bit 5). Bit 0
    // (keyboard interrupt) and bit 6 (translation) are preserved so the PS/2
    // keyboard keeps working exactly as before.
    if !command(CMD_READ_CONFIG) {
        return false;
    }
    let Some(mut config) = wait_controller_output() else {
        return false;
    };
    config |= 0x02;
    config &= !0x20;
    if !command(CMD_WRITE_CONFIG) || !write_data(config) {
        return false;
    }
    flush_output();

    // Stop any stream the firmware may have left running. Otherwise a motion
    // packet could arrive while we are still waiting for command replies and
    // be mistaken for one.
    let _ = aux_command(AUX_DISABLE_REPORTING);
    flush_output();

    // 0xF6: restore defaults (100 samples/s, 4 counts/mm, 1:1 scaling).
    // 0xF4: enable data reporting, i.e. start streaming packets.
    aux_command(AUX_SET_DEFAULTS) && aux_command(AUX_ENABLE_REPORTING)
}

// --- public interface -------------------------------------------------------

/// Programs the auxiliary device. `width`/`height` bound the pointer to the
/// framebuffer; called once, before interrupts are enabled.
pub fn init(width: i32, height: i32) {
    let present = configure();

    let mut state = STATE.lock();
    state.width = width;
    state.height = height;
    state.pointer = Pointer::new(width / 2, height / 2);
    state.index = 0;
    state.head = 0;
    state.tail = 0;
    state.present = present;

    if present {
        serial_println!("[mouse] PS/2 auxiliary device streaming on IRQ12");
    } else {
        serial_println!("[mouse] no PS/2 auxiliary device answered; pointer disabled");
    }
}

/// Whether a mouse answered during initialisation.
pub fn present() -> bool {
    without_interrupts(|| STATE.lock().present)
}

/// The newest pointer snapshot. Cheap enough to call once per loop iteration.
pub fn pointer() -> Pointer {
    // Interrupts must be masked: the IRQ12 handler takes the same lock, so a
    // nested interrupt while it is held would spin forever on a single CPU.
    without_interrupts(|| STATE.lock().pointer)
}

/// Pops one queued button transition, oldest first.
pub fn read_event() -> Option<ButtonEvent> {
    without_interrupts(|| STATE.lock().pop())
}

/// Number of IRQ12 interrupts handled (reported by `info`).
pub fn interrupt_count() -> u64 {
    INTERRUPTS.load(Ordering::Relaxed)
}

/// Called from the IRQ12 handler with the byte read from port 0x60.
///
/// The controller may interleave a command response with packet data, so the
/// parser resynchronises on the "always 1" bit of the first packet byte.
pub fn on_byte(byte: u8) {
    INTERRUPTS.fetch_add(1, Ordering::Relaxed);

    let mut state = STATE.lock();

    if state.index == 0 && byte & 0x08 == 0 {
        // Not a packet header: discard and wait for the next byte.
        return;
    }

    let index = state.index;
    state.packet[index] = byte;
    state.index = index + 1;
    if state.index < 3 {
        return;
    }
    state.index = 0;

    let packet = state.packet;
    state.apply(packet);
}
