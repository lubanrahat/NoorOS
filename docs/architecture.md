# NoorOS architecture

This document describes how NoorOS 0.1.0 is put together and why. It reflects
the code that produced the screenshots in `screenshots/demo/`.

---

## 1. The shape of the system

NoorOS is two programs:

1. **The host launcher** (`src/main.rs`, crate `nooros`) is an ordinary macOS/Linux
   binary. It does not contain OS code. It starts `qemu-system-x86_64` with the
   disk image built by `build.rs`.
2. **The guest kernel** (`kernel/`, crate `kernel`) is a bare-metal
   `#![no_std] #![no_main]` x86_64 ELF. It owns the machine after the bootloader
   hands over.

```
                 build time                              run time
┌──────────────────────────────┐        ┌──────────────────────────────────┐
│ kernel/  (x86_64-unknown-none)│        │ qemu-system-x86_64 -machine pc   │
│        │ cargo artifact dep   │        │   -m 512M -vga std               │
│        ▼                      │        │   -drive file=nooros-bios.img    │
│ build.rs ── bootloader 0.11 ──┼───────▶│                                  │
│        │                      │        │  BIOS / OVMF                     │
│        ├─ nooros-bios.img     │        │    └─ bootloader                 │
│        └─ nooros-uefi.img     │        │        └─ Noor Kernel           │
└──────────────────────────────┘        │            └─ Noor Desktop       │
                                        └──────────────────────────────────┘
```

The kernel and bootloader communicate through `bootloader_api`. The kernel does
not implement its own boot protocol.

## 2. Boot chain

### 2.1 Build

`Cargo.toml` declares the kernel as an artifact dependency:

```toml
[build-dependencies]
kernel = { path = "kernel", artifact = "bin", target = "x86_64-unknown-none" }
bootloader = "0.11.17"
```

Artifact dependencies are a Cargo nightly feature, enabled in
`.cargo/config.toml` with `[unstable] bindeps = true`. Cargo therefore builds the
kernel ELF first and exposes it to `build.rs` through
`CARGO_BIN_FILE_KERNEL_kernel`.

`build.rs` then:

1. creates a `BootConfig` asking for a framebuffer at least 1280x800,
2. disables firmware framebuffer logging but keeps serial logging,
3. builds `nooros-bios.img` with `BiosBoot`, and
4. builds `nooros-uefi.img` with `UefiBoot`.

The kernel also embeds a `BootloaderConfig` with a 128 KiB stack, because the
desktop's nested drawing code needs more than the default 80 KiB.

### 2.2 Firmware and bootloader

- **BIOS path:** SeaBIOS loads the bootloader's first stage, which enters
  protected mode/long mode, reads the kernel ELF, sets up page tables and a
  stack, and jumps to the kernel entry point.
- **UEFI path:** OVMF loads the same bootloader as a UEFI application; the
  bootloader performs the same long-mode hand-off.

In both cases the kernel receives `&'static mut BootInfo` containing:

- an optional framebuffer: address, size, width, height, stride, pixel format,
  bytes per pixel,
- the physical memory map,
- the kernel ELF address and length,
- optional RSDP and other firmware data (not used in 0.1.0).

### 2.3 Requested vs actual display mode

The image asks for 1280x800. QEMU's `-vga std` supplies 1280x720 in this setup.
The kernel never assumes the request succeeded: every layout reads
`framebuffer_info.width/height/stride/bytes_per_pixel`. The boot screen prints
the real mode (`1280x720 - 3 bpp - Bgr`).

### 2.4 Kernel entry

`kernel_main` runs with interrupts masked. Firmware and bootloader may leave
the 8259 PIC on its default vectors, where IRQ0 (vector 8) would be mistaken for
a double fault before NoorOS installs its IDT. Interrupts are enabled only at the
end of `interrupts::init`.

## 3. Initialisation order

`kernel/src/kernel/init.rs` is deliberately linear, and the boot screen mirrors
it:

| # | Step | What is actually done |
|---|---|---|
| 1 | Bootloader hand-off | `BootInfo` received; framebuffer and memory map are available |
| 2 | Framebuffer | `BootInfo.framebuffer` is consumed and installed in the driver |
| 3 | Heap | 8 MiB static `.bss` heap registered with `linked_list_allocator` |
| 4 | Memory map | Bootloader regions classified; totals and a snapshot are stored |
| 5 | Interrupts | IDT loaded, PIC remapped, PIT programmed to 100 Hz, interrupts enabled |
| 6 | Input | `pc-keyboard` parser and PS/2 mouse configured; IRQ1/IRQ12 handlers installed |
| 7 | CPU | `CPUID` leaves 0 and `0x8000_0002..4` read vendor, brand and family/model |
| 8 | Desktop | `Desktop::new` builds the window manager and caches the wallpaper |

Serial is initialised before the boot screen exists, so the UART banner is logged
but is not one of the visible `[ OK ]` steps.

Two ordering constraints matter:

- **The heap must exist before the first allocation.** The boot screen stores
  `String` labels and details, and `serial_println!` allocates while formatting.
  `memory::init_heap()` therefore runs before `BootScreen::new` and before any
  `format!`.
- **The boot screen's background must be freed before the desktop allocates.**
  The modelled wallpaper is a full framebuffer-sized buffer. `drop(boot)` runs
  before `Desktop::new` caches its own wallpaper, so the 8 MiB heap holds only
  one of them at a time.

The boot screen's eight `[ OK ]` lines are pushed only after each step returns.
There are no timed or fake messages; the visible boot order is the real call
order in `init.rs`.

## 4. Memory

### 4.1 Physical map

`memory::init_map` walks `BootInfo.memory_regions` and classifies each region:

| Kind | Meaning |
|---|---|
| `Usable` | Conventional RAM reported usable by firmware |
| `Bootloader` | Page tables, boot info, kernel loading structures |
| `Reserved(u32)` | Firmware-reserved, with the raw E820/UEFI type tag |

It computes usable, reserved and total bytes and stores a `Vec<Region>` snapshot
for Noor Monitor, Noor Settings and the `memory` command.

In the verified QEMU configuration the map contains 11 regions, 502 MiB usable,
and a 12 GiB reserved MMIO hole at `0x00FD00000000`. The reported "12 GiB total"
is the map's span, not installed RAM; NoorOS does not pretend otherwise.

### 4.2 Kernel heap

```rust
const HEAP_SIZE: usize = 8 * 1024 * 1024;

#[repr(align(4096))]
struct HeapStorage(UnsafeCell<[u8; HEAP_SIZE]>);

static HEAP: HeapStorage = HeapStorage(UnsafeCell::new([0; HEAP_SIZE]));

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();
```

`HEAP` lives in `.bss`, so the bootloader maps it and no page allocator is
needed. `linked_list_allocator` provides a first-fit free list with coalescing.
The heap backs every `String`, `Vec`, `format!`, window state and wallpaper
buffer. It is protected by a spin lock, and NoorOS never allocates from an
interrupt handler (the keyboard IRQ only writes into a fixed-size queue).

At runtime the heap reports 8 MiB total and about 2 MiB used with one terminal
open; wallpapers and scrollback account for most of it.

### 4.3 No virtual memory

NoorOS has no page-table manager, no address-space abstraction, no user mode and
no paging. The bootloader's mappings are used as-is. This is the single biggest
limitation of the project and is stated in the `memory` command, Noor Settings,
the report and this document.

Consequences:

- every pointer is physical/identity-mapped, and the kernel can read all RAM,
- there is no memory protection between applications,
- a wild write corrupts the kernel rather than faulting,
- the heap is fixed at 8 MiB.

The IDT still handles page faults and other exceptions, so a fault that does
happen prints a diagnostic and a fatal screen instead of silently resetting.

## 5. Interrupts and devices

### 5.1 IDT and PIC

`interrupts::init` builds an IDT with handlers for the standard x86_64
exceptions (divide error, breakpoint, invalid opcode, double fault, general
protection, page fault, ...) plus IRQ0 (timer), IRQ1 (keyboard) and IRQ12
(mouse). The 8259 PIC is remapped from vectors 8-15 to 32-47 so hardware IRQs no
longer collide with CPU exceptions. The master PIC unmasks IRQ0/IRQ1 and IRQ2
(the cascade into the slave); the slave unmasks only IRQ12, i.e. masks `0xF8`
and `0xEF`.

Each IRQ handler sends an end-of-interrupt to the PIC. Exceptions print the
vector, error code and, for page faults, CR2 to the serial port and attempt to
draw a fatal screen through `framebuffer::with_try`.

### 5.2 PIT

Channel 0 is programmed in rate-generator mode with:

```
divisor = 1_193_182 / 100 = 11_931
```

giving approximately 100 ticks per second. `timer::ticks()` is the monotonic
clock used by uptime, the status bar and `timer::sleep_ms`. `timer::interrupt_count()`
is reported by `info`.

### 5.3 Keyboard

`pc-keyboard` 0.9 decodes scancode set 1 with a US 104-key layout and ignores
control keys at the decoder level. IRQ1 pushes decoded events into a fixed
64-entry ring queue. The desktop drains the queue in its event loop with
`without_interrupts`, so the handler never blocks and never allocates.

`InputEvent` carries both the key and shift/ctrl/alt state. The queue counts
drops; Noor Monitor shows them (`0 dropped` in the verified run).

### 5.4 Mouse

`drivers/mouse.rs` programs the 8042 auxiliary device through ports `0x64`
(command/status) and `0x60` (data):

1. read the controller configuration byte, set bit 1 (auxiliary interrupt) and
   clear bit 5 (auxiliary clock disable), preserving the keyboard bits;
2. send `0xD4` + `0xF6` to restore the mouse defaults,
3. send `0xD4` + `0xF4` to enable data reporting.

Every wait is bounded, so a machine without an auxiliary device still boots;
`mouse::present()` reports whether a device answered, and the boot screen says
"PS/2 keyboard IRQ1, mouse IRQ12" or "PS/2 keyboard IRQ1, no mouse found".

The IRQ12 handler is vector `PIC_2_OFFSET + 4` = `0x2C`. It reads the status
port first and only consumes a byte when the output buffer is full, so a
spurious IRQ cannot inject garbage into the parser. Three-byte packets are
assembled with the standard layout: byte 0 carries the button bits, the two sign
bits and the overflow bits; bytes 1-2 are 9-bit signed X and Y deltas. Because
PS/2 Y grows upwards, screen Y subtracts `dy`. The pointer is clamped to the
framebuffer.

Two kinds of state leave the driver:

- `pointer()` returns the newest position and button snapshot. Motion is
  **coalesced**: a slow frame never replays a backlog of stale positions.
- `read_event()` returns queued button transitions. A press or release is never
  coalesced, because losing a click would lose a user action.

Both readers take the same lock as the handler, so they run with interrupts
masked; NoorOS is single-CPU and a nested IRQ while holding the lock would
spin forever. The handler itself only reads a port and updates a fixed-size
struct: no allocation, no blocking.

### 5.5 RTC and CPUID

- **RTC:** CMOS ports `0x70/0x71`, register B used to detect binary vs BCD mode
  and 12/24-hour mode. The kernel reads the date and time for the status bars
  and the `uptime` command.
- **CPUID:** leaf 0 gives the vendor; leaves `0x8000_0002..4` give the 48-byte
  brand string (when supported, checked via leaf `0x8000_0000`); leaf 1 gives
  family, model and the hypervisor-present bit. No CPU facts are hard-coded.
  In QEMU the result is `AuthenticAMD`, `QEMU Virtual CPU version 2.5+`,
  family 15, model 107, hypervisor yes.

### 5.6 Serial

The kernel writes a UART 16550 log at COM1, 115200 8N1. It is the primary
diagnostic channel during boot and can be redirected to a file by the launcher.
The log includes the boot banner, framebuffer mode, heap location, every
classified memory region, and every completed boot step.

## 6. Graphics

NoorOS has its own immediate-mode renderer; there is no GPU, no hardware
compositor and no graphics library.

### 6.1 Canvas

`Canvas` wraps a mutable framebuffer slice plus `FrameBufferInfo` and provides:

- bounds-checked `put_pixel` and `pixel_at`,
- pixel-format encoding for RGB, BGR and greyscale,
- `blend_pixel` for per-pixel alpha against the framebuffer,
- `clear` and `fast_hline` fast paths for 3 and 4 bytes per pixel,
- `blit` for cached background buffers.

QEMU's `-vga std` framebuffer is 1280x720 at 3 bytes per pixel, BGR. The 3-byte
path is the hot path: `fast_hline` fills a row with `as_chunks_mut::<3>()`
instead of recomputing the format per pixel.

### 6.2 Primitives and text

`graphics/primitives.rs` provides rectangles, rounded rectangles, circles,
rings, lines, alpha variants, vertical gradients, and procedural radial glows.
`graphics/text.rs` renders `font8x8` glyphs with integer scaling. A glyph row is
coalesced into runs of set bits and drawn as one rectangle per run, which is much
cheaper than one rectangle per pixel. Text helpers cover normal, alpha, tracked
(letter-spaced), centred, right-aligned, wrapping and ellipsis.

The whole UI uses one bitmap font. There is no font rasteriser, no anti-aliasing
and no Unicode shaping; `font8x8`'s `unicode` feature supplies the basic glyphs.

### 6.3 Branding

`graphics/branding.rs` draws the Noor mark procedurally: a gold ring, an
eight-point star and a small sparkle, plus the `NOOROS` wordmark and the tagline
`A Light Beyond Computing.`. The wallpaper uses a vertical gradient, two radial
glows (gold above, blue at the edge) and a faint grid/ring motif. It is
expensive, so it is rendered once into a framebuffer-sized buffer and blitted on
every repaint.

The palette is fixed:

| Token | Hex | Use |
|---|---|---|
| `BG` | `#080B14` | window content, page background |
| `DESKTOP` | `#0B1120` | desktop base |
| `SURFACE` | `#111827` | panels, taskbar |
| `SURFACE_2` | `#1E293B` | raised rows, selection |
| `GOLD` | `#F5C76A` | brand, focus, prompt |
| `BLUE` | `#38BDF8` | Monitor accent |
| `VIOLET` | `#8B5CF6` | Settings accent |
| `SUCCESS` | `#10B981` | boot OK, About accent |
| `TEXT` | `#F8FAFC` | primary text |
| `DIM` | `#94A3B8` | secondary text |
| `BORDER` | `#334155` | 1 px borders |

## 7. Desktop and window manager

`desktop/` is a small immediate-mode window system:

- `Desktop` owns `Vec<Window>`, the focused index, the launcher, the pointer
  snapshot, the active drag, the cursor's save-under buffer, the cached
  wallpaper and the last tick.
- `Window` owns an `AppId`, a `Rect` and an `AppState`. It draws its shadow,
  body, title bar, close glyph and content, then delegates content rendering to
  the application. `title_rect()` and `close_rect()` are shared by drawing and
  hit-testing, so the button a user sees is the button the manager reacts to.
- `AppId` supplies title, description, accent colour, default geometry, icon
  drawing and state construction. Windows cascade slightly so they do not stack
  exactly.
- Focus is drawn with a brighter title bar, an accent dot and a 2 px strip at
  the bottom of the content.
- The top status bar shows brand, uptime, kernel state, architecture, date and
  time. The floating taskbar shows the launcher button, one item per application,
  uptime and a larger clock. `taskbar::bar_rect`/`item_rect` are shared by the
  renderer and `item_at`, so slots cannot drift apart.

The launcher is modal: an opaque scrim covers the desktop, the panel lists the
four applications with icons, descriptions and keycaps, and `Up/Down/Enter/Esc`
drive it. `launcher::panel_rect`/`row_rect` define both the drawing and the hit
boxes. The top bar and taskbar are drawn after the scrim so the clock and
running applications stay visible around the modal.

### Mouse and the cursor

The cursor is a 12x19 arrow plus a one-pixel alpha shadow. NoorOS has no second
framebuffer, so the cursor uses **save-under**:

1. before painting, copy the 13x21 pixels it will cover into a `Cursor` buffer,
2. paint the shadow and the outlined arrow,
3. before the next move or repaint, copy those pixels back.

`Cursor::erase` must run before anything else repaints the region, so
`Desktop::render` erases first and draws the cursor last. The saved pixels always
describe the last *drawn* position, not the last requested one. This is what
keeps the screen free of trails; the demo script verifies it by moving the
pointer away and back and comparing the two frames byte for byte.

Movement takes the cheap path. `Desktop::set_pointer` returns a `Repaint` value:

| Value | When | Work |
|---|---|---|
| `None` | The pointer did not move | nothing |
| `Cursor` | Pointer moved, no drag | restore and repaint 13x21 pixels |
| `Full` | Pointer moved while dragging a title bar | repaint the whole desktop |

### Hit-testing and z-order

`Desktop::handle_mouse` consumes button edges in the same order the screen is
painted:

1. taskbar slots (`taskbar::item_at`) - launcher button and application icons;
2. the modal launcher (`Launcher::hit`) - a row launches, a click outside
   dismisses;
3. windows from the top of the z-order down.

Inside a window, `close_rect()` wins over `title_rect()`; the title bar focuses
the window first (moving it to the top) and then records the grab offset for the
drag; any other content click only focuses. Dragging clamps the window so at
least 120 px stays on screen horizontally and the title bar never goes under the
top status bar. Only the left button acts in 0.1.0; right and middle are tracked
by the driver but unused.

### Rendering model

`desktop::run` is event-driven:

```rust
loop {
    let ticks = timer::ticks();
    let mut full = false;
    let mut cursor = false;

    // Repaint once per minute for the clock.
    // Drain keyboard events; each event handles the key and marks a full frame.
    // Coalesce pointer motion into the newest position.
    // Drain mouse button edges and hit-test them.

    if full {
        framebuffer::with(|canvas| desktop.render(canvas));
    } else if cursor {
        framebuffer::with(|canvas| desktop.render_cursor(canvas));
    }

    hlt();
}
```

There is no 60 Hz timer and no animation loop. A full frame is expensive under
QEMU TCG, so NoorOS repaints only when something changes. Pointer motion that
does not drag a window repaints only the cursor footprint. The terminal cursor
blinks at 2 Hz, but only while the terminal is focused; the desktop does not
wake up to blink it. The percentage of time spent drawing is therefore low, at
the cost of a static-looking desktop between events.

### Why the wallpaper is cached

The procedural wallpaper touches all 1280x720 pixels with several gradients and
glows. Rendering it on every frame dominated the frame time, so `Desktop` and
`BootScreen` each keep one off-screen copy in the heap and `blit` it. This is the
main reason a repaint is usable at all under TCG.

## 8. Applications

| Application | Behaviour |
|---|---|
| **Noor Terminal** | 400-line scrollback, command history, line editing, `PgUp/PgDn`, `clear`, 54x18 character viewport |
| **Noor Monitor** | OS, processor, memory and system cards; physical memory map; `Up/Down` scroll |
| **Noor Settings** | Five read-only sections: General, Appearance, Display, System, About NoorOS |
| **About NoorOS** | Logo, version, project description, crate credits |
| **Noor Launcher** | Modal application list with selection and keycaps |

Each application is a state machine with `render(&mut self, canvas, rect, tick,
focused)` and `handle_key(InputEvent) -> AppAction`. `AppAction` is `None`,
`Close`, `Shutdown` or `Reboot`; the desktop acts on the last two, so
applications never touch power hardware directly.

The terminal is the most complete application:

- the shell parser splits the command name from the untouched argument text,
- `shell::execute` returns `Output { lines, clear, action, error }`,
- output lines keep a colour, so errors render in red,
- wrapping is column-aware, preserving the prompt indentation,
- unknown input produces `Command not found. Type 'help' for available commands.`

## 9. Shell

`shell/commands.rs` implements ten commands. They read live kernel state:

- `info` combines CPUID, framebuffer info, memory totals, heap stats, uptime and
  RTC clock. Unimplemented features are labelled, e.g.
  `Virtual memory  not implemented (no paging)`.
- `memory` prints the full physical map (11 rows in QEMU), the heap line and the
  paging limitation. The output is sized to fit the terminal viewport.
- `version` prints NoorOS, Noor Kernel, target triple and allocator.
- `uptime` prints uptime, PIT ticks, timer interrupts and the RTC clock.
- `shutdown` writes the ACPI sleep type + enable bit to `PM1a_CNT` (`0x604`),
  tries the older `0xB004` port, then QEMU's `isa-debug-exit` at `0xF4`.
- `reboot` pulses the 8042 reset line (`0x64 <- 0xFE`) and falls back to a
  triple fault.

## 10. Power and exit

QEMU is started with:

```
-device isa-debug-exit,iobase=0xf4,iosize=0x04
```

so the kernel has a deterministic exit path for tests. ACPI shutdown makes QEMU
exit with status 0; the `isa-debug-exit` fallback writes a distinctive status
code. The launcher prints any non-zero QEMU exit status; a kernel panic paints a
fatal screen and halts so the machine stays available for inspection.

`reboot` resets the guest. The host launcher passes `-no-reboot`, so a reboot in
the demo ends QEMU; the test harness omits `-no-reboot` to observe a real reset.

## 11. Failure handling

- `panic!` goes through a custom panic handler that prints the message, file and
  line to serial and tries to paint a fatal screen.
- CPU exceptions use the same path. `framebuffer::with_try` uses `try_lock`, so a
  fault inside the renderer cannot deadlock the fatal screen.
- The heap must be initialised before any allocation; this was a real deadlock
  during development and is now enforced by ordering and documented in
  `init.rs`.
- Interrupts are disabled at entry and enabled only after the IDT and PIC are
  ready.

## 12. Performance notes

Measured during development on an Apple Silicon host with QEMU TCG:

| Operation | Approximate cost |
|---|---|
| Full repaint, one window | ~250 ms |
| Full repaint, four windows | ~900 ms |
| Full repaint with launcher | ~1.2 s |

These are emulator costs, not native costs. Optimisations that made the UI
usable:

1. cache the boot background and desktop wallpaper, then `blit`,
2. draw text as horizontal runs rather than per-pixel rectangles,
3. add a 3-byte-per-pixel `fast_hline`,
4. use block-sampled radial glows (6x6 blocks) instead of per-pixel distance,
5. repaint on events only, not on a timer,
6. repaint only the 13x21 cursor footprint while the pointer moves,
7. use an opaque launcher scrim instead of full-screen alpha blending.

The screenshots were taken with a 2.5 s settle time after each action for this
reason.

## 13. Limitations and future work

**Limitations in 0.1.0**

- no paging, user mode, processes or scheduling,
- no GPU/compositor, no general damage tracking, no double buffering of windows
  (only the cursor footprint is repainted separately),
- no filesystem, storage, networking, sound or SMP,
- one bitmap font, ASCII/Latin glyphs only,
- PS/2 keyboard and mouse only; no USB HID, no wheel scrolling,
- the 8 MiB heap is fixed at compile time,
- UEFI support depends on OVMF and was verified with the pinned prebuilt.

**Natural next steps**

1. a page-frame allocator and a real virtual memory manager,
2. user-mode processes and a syscall boundary,
3. damage rectangles and partial window redraws,
4. a small ramdisk filesystem and a text editor,
5. SMP bring-up with per-CPU data,
6. UEFI runtime services and a proper boot menu.

Each would be a project in itself; 0.1.0 deliberately stops at a coherent,
demonstrable single-address-space desktop.
