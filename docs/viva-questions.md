# NoorOS - 22 viva questions and answers

Answers are written to be spoken aloud. Each one names the file or mechanism
that can be shown in the code.

---

### 1. What exactly makes NoorOS an operating system rather than a program?

It is a freestanding x86_64 kernel that is loaded by firmware and a bootloader
and then owns the machine. It installs its own IDT, handles hardware interrupts,
programs the PIT, reads the physical memory map, runs a global allocator and
draws directly into the framebuffer. There is no host OS underneath: `main.rs`
on the host only starts QEMU, and the kernel has `#![no_std] #![no_main]`.

It is a *small* OS: single address space, no processes. But the boundary between
"program" and "kernel" is real.

### 2. Walk me through the boot sequence from power-on to Noor Desktop.

Firmware (SeaBIOS or OVMF) loads the disk image. The `bootloader` 0.11 first
stage loads the kernel ELF, enables long mode, builds page tables, sets up a
128 KiB stack and passes `BootInfo`. `kernel_main` disables interrupts, then
`kernel/init.rs` initialises serial, framebuffer, the bitmap frame allocator and
active page tables, the initial heap pages, the memory-map snapshot, the keyboard
parser, the PS/2 mouse, the PIT at 100 Hz, and finally the
IDT and PIC, at which point interrupts are enabled. The boot screen prints one
`[ OK ]` line after each step. Then the desktop is created, its wallpaper cached,
and the boot screen background is dropped before `desktop::run` enters the event
loop.

### 3. Why use the `bootloader` crate instead of writing your own bootloader?

Writing a BIOS bootloader, a UEFI application, long-mode transition, ELF loading
and a video-mode negotiation is a project on its own and would have consumed the
whole time budget without teaching anything about the kernel's own subsystems.
`bootloader` 0.11 is maintained by rust-osdev and gives a clean `BootInfo`
contract for both BIOS and UEFI. The educational value is in what happens after
the hand-off: IDT, interrupts, memory, input, graphics and the desktop.

### 4. How does the kernel know the screen resolution, and what if the requested mode is unavailable?

`build.rs` asks for at least 1280x800, but the kernel never trusts that request.
It reads `BootInfo.framebuffer` and uses the reported width, height, stride,
pixel format and bytes per pixel. In this QEMU configuration the actual mode is
1280x720 at 3 bytes per pixel, BGR, and the boot screen prints exactly that.
All layout code uses the reported values, so a different mode changes the layout
rather than corrupting it.

### 5. How do you know the boot screen is not fake?

`BootScreen::ok(label, detail)` is called only after the corresponding function
returns, and it repaints immediately. The eight lines map one-to-one to the calls
in `kernel/init.rs`, and the serial log prints the same lines in the same order.
There is no timer that reveals pre-written messages. The framebuffer line is
generated from `FrameBufferInfo`; the memory line from the classified map; the
CPU line from CPUID.

### 6. Why must memory be initialised before the boot screen is created?

The boot screen stores `String` labels and details, and `alloc::format!`
allocates. The global allocator is backed by a heap whose first pages are mapped
by our own paging code, so the frame allocator, the active page tables and the
initial heap pages must exist before the first allocation or the kernel panics.
`memory::init()` therefore runs before `BootScreen::new` and before any
`format!`. This ordering was learned from a real deadlock during development.

### 7. Explain the heap implementation. Why is it no longer in `.bss`?

The heap is a contiguous virtual range at `0x0000_4000_0000_0000`, not a static
array. At boot only 2 MiB are mapped. A `linked_list_allocator::Heap` behind a
spin mutex provides first-fit allocation with coalescing. When an allocation
fails, the allocator releases the heap lock, maps enough fresh 4 KiB frames
directly after the existing heap, calls `Heap::extend`, and retries. A partial
mapping is rolled back. The boot screen and wallpaper grow the heap from 2 MiB to
about 4.6 MiB, which the `memory` command displays as mapped and in-use bytes.
The cost is that growth is explicit: this is not demand paging.

### 8. How does virtual memory work in NoorOS, and what is still missing?

Three layers. `memory::frame_allocator` builds a bitmap from the bootloader's
memory map and owns 4 KiB physical frames. `memory::paging` reads CR3 and edits
the active PML4 through the bootloader's physical-memory offset, using the
`x86_64` crate's `OffsetPageTable`/`Mapper` traits; it provides `map_page`,
`unmap_page` and `translate_addr`, allocates intermediate tables on demand and
flushes the TLB. The heap uses those mappings to grow. A boot-time self-test maps,
translates and unmaps a scratch page, and the `memory` command shows live heap and
kernel translations. Still missing: demand paging, guard pages, swapping and
user-mode address spaces; a wild kernel write still corrupts the kernel.

### 9. How do interrupts work here?

`interrupts::init` builds an IDT with handlers for CPU exceptions and for IRQ0,
IRQ1 and IRQ12, then remaps the 8259 PIC from vectors 8-15 to 32-47 so hardware
IRQs no longer collide with exceptions. The PIT is programmed to 100 Hz; each
IRQ0 increments a monotonic tick counter and the interrupt counter. IRQ1 feeds
the keyboard queue. IRQ12 is the slave PIC's line 4, mapped to vector `0x2C`, so
the master unmasks IRQ2 for the cascade and the slave unmasks only IRQ12 (masks
`0xF8` and `0xEF`). Each handler sends EOI. Interrupts are disabled at kernel
entry because firmware may leave the PIC unmapped, where IRQ0 would arrive as
vector 8 and be interpreted as a double fault.

### 10. Trace a key press from the keyboard to the terminal.

The PS/2 controller raises IRQ1. The handler reads port `0x60`, passes the
scancode to `pc-keyboard` (set 1, US 104-key, control keys ignored), and pushes
an `InputEvent` with shift/ctrl/alt state into a fixed 64-entry ring queue. The
desktop event loop drains that queue with `without_interrupts` and routes each
event: function keys to the desktop, everything else to the focused application.
Noor Terminal appends characters to its input buffer or executes a command on
Enter. The IRQ never allocates and never blocks.

### 11. How do you obtain CPU information without an external crate?

With the `CPUID` instruction through `core::arch::x86_64::__cpuid`. Leaf 0 gives
the vendor string; leaf `0x8000_0000` is checked before reading the brand leaves
`0x8000_0002..4`; leaf 1 gives family, model and the hypervisor bit. Nothing is
hard-coded. In QEMU the result is `AuthenticAMD`, `QEMU Virtual CPU version 2.5+`,
family 15, model 107, hypervisor yes.

### 12. What is the difference between uptime and the RTC clock in NoorOS?

Uptime comes from the PIT tick counter: ticks × 1000 / 100 Hz. It measures time
since boot and is monotonic, but it depends on interrupts being enabled and is
not wall-clock accurate over long periods. The RTC comes from CMOS registers via
ports `0x70/0x71`, handles BCD/binary and 12/24-hour mode from status register B,
and gives calendar date and time. The status bar uses both: uptime for the
session, RTC for the date and wall clock.

### 13. How is a pixel actually drawn?

`Canvas` computes `offset = (y * stride + x) * bytes_per_pixel`, checks bounds,
encodes the colour according to `PixelFormat` (RGB, BGR or greyscale luminance)
and copies 3 or 4 bytes. Alpha blending reads the existing pixel and mixes:
`(fg * a + bg * (255 - a)) / 255`. Higher layers only call primitives. The 3-byte
BGR path is optimised with `as_chunks_mut::<3>()` in `fast_hline`.

### 14. How does the window manager decide what is on top, who has focus, and what a click hits?

Windows live in a `Vec<Window>` in open order. Rendering iterates from index 0
upward, so later windows are drawn over earlier ones. Opening an existing
application moves it to the end and focuses it; `Alt/Ctrl+Tab` advances the
focused index cyclically; closing removes the window and focuses the last
remaining one. Focus changes the title-bar colour, the close-button colour and
adds an accent strip to the content. The launcher is modal and consumes keys
while open.

Hit-testing walks the same order backwards, because the last thing painted is the
first thing visible: taskbar, then the modal launcher if it is open, then windows
from top to bottom. The rectangles come from the same helpers used for drawing
(`Window::title_rect`, `Window::close_rect`, `bar_rect`, `panel_rect`), so a click
cannot disagree with the picture.

### 15. Why is the desktop event-driven instead of running at 60 Hz?

A full frame under QEMU TCG costs roughly 250 ms with one window and about 1.2 s
with the launcher. Repainting on a timer would leave the screen permanently
half-drawn and waste emulated CPU. The loop blocks in `hlt`, repaints only when a
key event, a mouse event or a minute-boundary clock update occurs, and therefore
looks static between events. The terminal cursor blinks only while focused,
during repaints. The tradeoff is no animations, but the UI is usable under
emulation.

### 16. How does the mouse pointer move without repainting the whole screen?

Two paths share one `Cursor`. `Cursor::erase` copies back the pixels that were
saved underneath the arrow; `Cursor::render` saves the new area, draws a soft
shadow, then the outlined arrow. A move only calls `render_cursor`, which erases
the old position and draws at the new one, touching a 13x21 rectangle twice
instead of a 1280x720 frame. Keyboard events still request `Repaint::Full`.
Because the saved pixels are restored exactly, moving away and back produces a
byte-identical frame - there are no trails.

### 17. What did you do to make rendering fast enough?

Seven things: cache the boot background and desktop wallpaper and blit them; draw
text as coalesced horizontal runs instead of per-pixel rectangles; add a
3-byte-per-pixel `fast_hline`; block-sample radial glows at 6x6 instead of
per-pixel; repaint only on events; use an opaque scrim for the launcher instead of
full-screen per-pixel alpha; and give the pointer a save-under cursor-only
repaint. Screenshots wait 2.5 s after each action because even optimised frames
are slow under TCG.

### 18. Trace a mouse movement and a click from the wire to the desktop.

The 8042 controller's auxiliary port raises IRQ12. The handler reads status port
`0x64`, checks the aux bit and that data is ready, then reads port `0x60` and
feeds the byte to a three-byte packet state machine: flags (buttons plus X/Y
sign), then X delta, then Y delta. Completed motion updates an atomic pointer
position clamped to the framebuffer, and a button edge is pushed into a small
queue. The IRQ itself never draws and never allocates: it holds a short spin lock
that readers take inside `without_interrupts`, so the main loop cannot deadlock
against its own interrupt.

The desktop loop is woken from `hlt`, takes the newest pointer position once
(motion is coalesced because only the final position matters), and drains queued
button edges (edges are queued because every click matters). It clamps to the
screen, hit-tests, and requests `Repaint::Cursor` for movement or `Repaint::Full`
when something changed. A press on a title bar starts a drag that stores the
grab offset, so the window follows the pointer without jumping; release ends it.
A release over `[X]` closes the window, and a click anywhere in a window raises
and focuses it.

### 19. How do shutdown and reboot work, and why is `isa-debug-exit` involved?

Shutdown writes the ACPI sleep type plus enable bit to `PM1a_CNT` at port
`0x604`, tries the legacy `0xB004` port, then writes to QEMU's
`isa-debug-exit` device at `0xF4`. That device gives the host launcher a
deterministic exit code for testing, including a distinct code for panics.
Reboot pulses the 8042 reset line with `0xFE` and falls back to a triple fault.
The launcher passes `-no-reboot`, so a demo reboot exits QEMU; the test harness
omits it to observe a real reset and returns to a fresh desktop.

### 20. How was the system tested, and what was not tested?

Every feature was exercised on the running image: QEMU boots the release image,
`scripts/interact.py` sends real key events through the monitor's `sendkey` and
real pointer events through `mouse_move`/`mouse_button`, captures each screen
with `screendump`, and the serial log is checked for the boot steps. The mouse
run moves the pointer 1:1, leaves and returns to prove frames are identical (no
trails), launches Terminal from the taskbar, drags a window by its title bar,
closes it with `[X]`, raises a covered window and picks a launcher row - seventeen
screenshots in total. BIOS and UEFI boots both reach the desktop; `shutdown` exits
QEMU with status 0; `reboot` returns to a desktop with uptime 1 s;
`cargo clippy --release` is clean. Not tested: physical hardware, Secure Boot,
non-OVMF UEFI, USB input, mice with wheels or extra buttons, non-US layouts, SMP,
long-run stability and offline OVMF download.

### 21. What was the hardest bug, and what did it teach you?

Two stand out. First, a heap deadlock: `serial_println!` was called while the
allocator's `free()` lock was held, so formatting re-entered the allocator. The
fix reads accounting into a local before taking the lock, and the boot order now
guarantees the heap exists before any allocation. Second, a stale disk image:
`build.rs` watched `kernel/src` as a directory, which Cargo does not treat as a
recursive content hash, so editing an existing file could boot the old kernel and
look like a rendering bug. The build script now registers every file, and the
capture scripts choose the newest image. Both taught that ordering and build
correctness are part of the design, not incidental details.

### 22. If you had another month, what would you build next?

In order: demand paging and guard pages; user mode with a syscall boundary and
two processes; damage rectangles with double buffering so repaints are partial
and the cursor path covers the rest; a ramdisk filesystem and a text editor; then
SMP with an APIC and a scheduler. Each is a project in itself, which is exactly
why 0.1.0 stops at a coherent single-address-space desktop with keyboard and
mouse input, and documents the boundary.

---

## Quick recall sheet

- Entry: `kernel/src/main.rs` → `kernel/init.rs`
- Boot screen: `kernel/src/boot/mod.rs`
- IDT/PIC/PIT: `kernel/src/interrupts/mod.rs`, `drivers/timer.rs`
- Keyboard and mouse: `kernel/src/drivers/keyboard.rs`, `drivers/mouse.rs`
- Cursor: `kernel/src/desktop/cursor.rs`
- Frame allocator, paging, heap: `kernel/src/memory/{frame_allocator,paging,heap}.rs`
- Canvas: `kernel/src/graphics/canvas.rs`
- Windows: `kernel/src/desktop/desktop.rs`, `window.rs`
- Terminal/shell: `applications/terminal.rs`, `shell/commands.rs`
- Power: `drivers/power.rs`
- Evidence: `screenshots/demo/`, `scripts/interact.py`
- Real mode: 1280x720, 3 bpp, Bgr; 11 regions, 506 MiB usable; 100 Hz PIT
- IRQ vectors: PIT 0x20, keyboard 0x21, mouse 0x2C; PIC masks `0xF8` / `0xEF`
