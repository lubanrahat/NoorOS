# NoorOS - 20 viva questions and answers

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
`kernel/init.rs` initialises serial, framebuffer, the 8 MiB heap, the memory-map
snapshot, the keyboard parser, the PIT at 100 Hz, and finally the IDT and PIC,
at which point interrupts are enabled. The boot screen prints one `[ OK ]` line
after each step. Then the desktop is created, its wallpaper cached, and the boot
screen background is dropped before `desktop::run` enters the event loop.

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

### 6. Why must the heap exist before the boot screen is created?

The boot screen stores `String` labels and details, and `serial_println!`
allocates while formatting. The global allocator is `linked_list_allocator`
backed by a static buffer, so it must be initialised before the first allocation
or the kernel panics or deadlocks. `memory::init_heap()` is therefore called
before `BootScreen::new` and before any `format!`. This ordering was learned from
a real deadlock during development.

### 7. Explain the heap implementation. Why put it in `.bss`?

An 8 MiB array is declared with `#[repr(align(4096))]` and wrapped in
`UnsafeCell`, with a `Sync` implementation justified by the fact that it is only
reachable through the global allocator's spin lock. `LockedHeap::empty()` is
registered as `#[global_allocator]` and initialised with that memory. `.bss` is
part of the kernel image and is mapped by the bootloader, so no page allocator is
needed. The cost is a fixed ceiling; the kernel reports total, used and free
bytes honestly.

### 8. Why is there no virtual memory, and what are the consequences?

Implementing page tables, a frame allocator, address spaces and a mapping API is
a substantial subsystem. For 0.1.0 the bootloader's identity mappings are used
for the kernel's lifetime. Consequences: every pointer is physical, there is no
memory protection, a wild write corrupts the kernel, the heap cannot grow, and
there is no user/kernel separation. The `memory` command and Noor Settings state
`not implemented (no paging)` rather than hiding it. The kernel still handles
page faults so a fault is reported, not silent.

### 9. How do interrupts work here?

`interrupts::init` builds an IDT with handlers for CPU exceptions and for IRQ0
and IRQ1, then remaps the 8259 PIC from vectors 8-15 to 32-47 so hardware IRQs
no longer collide with exceptions. The PIT is programmed to 100 Hz; each IRQ0
increments a monotonic tick counter and the interrupt counter. IRQ1 feeds the
keyboard queue. Each handler sends EOI. Interrupts are disabled at kernel entry
because firmware may leave the PIC unmapped, where IRQ0 would arrive as vector 8
and be interpreted as a double fault.

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

### 14. How does the window manager decide what is on top and who has focus?

Windows live in a `Vec<Window>` in open order. Rendering iterates from index 0
upward, so later windows are drawn over earlier ones. Opening an existing
application moves it to the end and focuses it; `Alt/Ctrl+Tab` advances the
focused index cyclically; closing removes the window and focuses the last
remaining one. Focus changes the title-bar colour, the close-button colour and
adds an accent strip to the content. The launcher is modal and consumes keys
while open.

### 15. Why is the desktop event-driven instead of running at 60 Hz?

A full frame under QEMU TCG costs roughly 250 ms with one window and about 1.2 s
with the launcher. Repainting on a timer would leave the screen permanently
half-drawn and waste emulated CPU. The loop blocks in `hlt`, repaints only when a
key event or a minute-boundary clock update occurs, and therefore looks static
between events. The terminal cursor blinks only while focused, during repaints.
The tradeoff is no animations, but the UI is usable under emulation.

### 16. What did you do to make rendering fast enough?

Six things: cache the boot background and desktop wallpaper and blit them; draw
text as coalesced horizontal runs instead of per-pixel rectangles; add a
3-byte-per-pixel `fast_hline`; block-sample radial glows at 6x6 instead of
per-pixel; repaint only on events; and use an opaque scrim for the launcher
instead of full-screen per-pixel alpha. Screenshots wait 2.5 s after each action
because even optimised frames are slow under TCG.

### 17. How do shutdown and reboot work, and why is `isa-debug-exit` involved?

Shutdown writes the ACPI sleep type plus enable bit to `PM1a_CNT` at port
`0x604`, tries the legacy `0xB004` port, then writes to QEMU's
`isa-debug-exit` device at `0xF4`. That device gives the host launcher a
deterministic exit code for testing, including a distinct code for panics.
Reboot pulses the 8042 reset line with `0xFE` and falls back to a triple fault.
The launcher passes `-no-reboot`, so a demo reboot exits QEMU; the test harness
omits it to observe a real reset and returns to a fresh desktop.

### 18. How was the system tested, and what was not tested?

Every feature was exercised on the running image: QEMU boots the release image,
`scripts/interact.py` sends real key events through the monitor's `sendkey`,
captures each screen with `screendump`, and the serial log is checked for the
boot steps. BIOS and UEFI boots both reach the desktop; `shutdown` exits QEMU
with status 0; `reboot` returns to a desktop with uptime 1 s; `cargo clippy
--release` is clean. Not tested: physical hardware, Secure Boot, non-OVMF UEFI,
USB input, non-US layouts, SMP, long-run stability and offline OVMF download.

### 19. What was the hardest bug, and what did it teach you?

Two stand out. First, a heap deadlock: `serial_println!` was called while the
allocator's `free()` lock was held, so formatting re-entered the allocator. The
fix reads accounting into a local before taking the lock, and the boot order now
guarantees the heap exists before any allocation. Second, a stale disk image:
`build.rs` watched `kernel/src` as a directory, which Cargo does not treat as a
recursive content hash, so editing an existing file could boot the old kernel and
look like a rendering bug. The build script now registers every file, and the
capture scripts choose the newest image. Both taught that ordering and build
correctness are part of the design, not incidental details.

### 20. If you had another month, what would you build next?

In order: a frame allocator and real virtual memory; user mode with a syscall
boundary and two processes; a PS/2 mouse and pointer hit-testing; damage
rectangles with double buffering so repaints are partial; a ramdisk filesystem
and a text editor; then SMP with an APIC and a scheduler. Each is a project in
itself, which is exactly why 0.1.0 stops at a coherent single-address-space
desktop and documents the boundary.

---

## Quick recall sheet

- Entry: `kernel/src/main.rs` → `kernel/init.rs`
- Boot screen: `kernel/src/boot/mod.rs`
- IDT/PIC/PIT: `kernel/src/interrupts/mod.rs`, `drivers/timer.rs`
- Heap and map: `kernel/src/memory/mod.rs`
- Canvas: `kernel/src/graphics/canvas.rs`
- Windows: `kernel/src/desktop/desktop.rs`, `window.rs`
- Terminal/shell: `applications/terminal.rs`, `shell/commands.rs`
- Power: `drivers/power.rs`
- Evidence: `screenshots/demo/`, `scripts/interact.py`
- Real mode: 1280x720, 3 bpp, Bgr; 11 regions, 502 MiB usable; 100 Hz PIT
