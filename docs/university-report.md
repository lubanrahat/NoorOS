# NoorOS 0.1.0 - A Light Beyond Computing

**A bare-metal x86_64 educational operating system with a framebuffer desktop, written in Rust**

| | |
|---|---|
| Author | *<your name>* |
| Module | *<Operating Systems / Systems Programming>* |
| Institution | *<your university>* |
| Supervisor | *<supervisor>* |
| Date | November 2026 |
| Artefact | NoorOS 0.1.0 (`nooros` host crate + `kernel` guest crate) |

---

## Abstract

NoorOS is a small operating system for the x86_64 architecture written from
scratch in Rust. It boots through the `bootloader` 0.11 firmware interface on
BIOS and UEFI, takes ownership of a linear framebuffer, and draws a complete
graphical desktop without any underlying operating system, browser engine or
graphics library. The kernel implements a real initialisation sequence,
exception and IRQ handling, a PIT-driven clock, PS/2 keyboard and mouse input, a
bitmap physical-frame allocator, active x86_64 page tables and a demand-grown
kernel heap. On top of these it provides a
window manager with a graphical pointer, a terminal with a ten-command shell, a
system monitor, a settings application, an about window and a modal launcher. The
project was verified by booting the release image in QEMU, scripting keyboard and
mouse input, and capturing framebuffer evidence; shutdown and reboot were both
confirmed. NoorOS deliberately omits demand paging, user mode, scheduling and
hardware acceleration, and documents these omissions rather than simulating
them.

---

## 1. Introduction

### 1.1 Motivation

Operating systems courses often explain processes, paging, scheduling and file
systems, but students rarely see the layer underneath: the code that runs
immediately after firmware, owns the CPU, and turns a framebuffer into
something a person can use. NoorOS was built to make that layer tangible.

The project's rule from the start was: **the pixels must be produced by the
kernel**. A web page, an Electron shell, an HTML canvas or a Linux userspace
process drawing a window would not demonstrate operating-system work. NoorOS
therefore contains its own boot hand-off, interrupt table, timer, keyboard and
mouse drivers, memory reporting, allocator, text renderer, window manager and
applications.

### 1.2 Aims

1. Boot a custom x86_64 kernel in QEMU on an Apple Silicon Mac.
2. Display a branded NoorOS boot screen whose steps reflect real completed
   initialisation.
3. Transition to a graphical Noor Desktop drawn entirely by the kernel.
4. Provide a working terminal with the required command set and an honest
   unknown-command message.
5. Report real hardware and memory facts, clearly labelling anything that is
   unavailable or unimplemented.
6. Make the desktop operable with a PS/2 mouse without losing the keyboard path.
7. Keep the implementation small enough to explain in a viva and to complete
   within a short project window.

### 1.3 Objectives

| ID | Objective | Evidence |
|---|---|---|
| O1 | Real boot chain, BIOS and UEFI images | §6.2, boot screenshots, serial logs |
| O2 | Framebuffer desktop rendered by the kernel | `01-desktop.png` |
| O3 | Boot screen with only completed steps | `00-boot.png`, `init.rs` |
| O4 | Terminal and required commands | `03-help.png` ... `07-unknown.png` |
| O5 | Monitor, Settings, About, Launcher | `08` ... `12` |
| O6 | Honest memory and hardware reporting | `04-info.png`, `05-memory.png` |
| O7 | Mouse: cursor, launch, focus, drag, close | `13` ... `17` |
| O8 | Reproducible build and demo scripts | `Makefile`, `scripts/` |
| O9 | Full documentation and viva preparation | this report, `docs/` |

---

## 2. Background

### 2.1 From firmware to long mode

On x86, the CPU starts in 16-bit real mode. Firmware (SeaBIOS or UEFI) selects a
boot device, loads a first stage, and eventually a bootloader must:

- discover or set a video mode,
- load the kernel image into memory,
- build page tables and enable long mode,
- set up a stack,
- pass a machine-readable description of memory and framebuffer,
- jump to the kernel entry point.

NoorOS does not reimplement this chain. It uses the `bootloader` crate 0.11,
which is the maintained successor to the older `bootimage` workflow. The kernel
declares `entry_point!(kernel_main)` and receives `&'static mut BootInfo`.

### 2.2 Framebuffers

A linear framebuffer is a region of memory where each pixel occupies a fixed
number of bytes. The bootloader reports width, height, stride (pixels per row,
which may exceed width), bytes per pixel, and pixel order (RGB, BGR or
greyscale). Writing a colour means computing

```
offset = (y * stride + x) * bytes_per_pixel
```

and encoding the colour in the reported order. NoorOS uses this directly. There
is no 2D acceleration, no GPU command stream and no page flipping.

### 2.3 Interrupts, the PIC and the PIT

The 8259 programmable interrupt controller (PIC) maps hardware IRQ lines onto
CPU interrupt vectors and must be remapped away from the CPU exception vectors.
The programmable interval timer (PIT) channel 0 generates periodic IRQs; with a
base frequency of 1,193,182 Hz and divisor 11,931 it produces approximately
100 ticks per second. NoorOS uses that tick as its monotonic clock.

The interrupt descriptor table (IDT) tells the CPU where each handler lives.
NoorOS installs handlers for the standard exceptions, page faults, the timer,
keyboard and mouse IRQs, and sends end-of-interrupt to the PIC after each IRQ.

### 2.4 `no_std` Rust

A kernel cannot use the Rust standard library: there is no OS underneath to
provide files, threads, a heap or panics. `#![no_std]` removes that dependency.
`#![no_main]` replaces the C runtime entry point with a function the bootloader
calls. The `alloc` crate is still available once a global allocator is
registered, which lets the kernel use `String`, `Vec` and `format!` while
remaining freestanding.

---

## 3. Requirements

### 3.1 Functional requirements

| ID | Requirement |
|---|---|
| F1 | Boot on x86_64 in QEMU from a disk image built by the project |
| F2 | Show an NoorOS graphical boot screen, then Noor Desktop |
| F3 | Draw all UI in the kernel framebuffer; no web or simulation layer |
| F4 | Provide Noor Terminal with `help`, `clear`, `about`, `info`, `memory`, `version`, `echo`, `uptime`, `shutdown`, `reboot` |
| F5 | Print `Command not found. Type 'help' for available commands.` for unknown input |
| F6 | Provide Noor Monitor, Noor Settings, About NoorOS and Noor Launcher |
| F7 | Accept keyboard and mouse input and switch/close windows |
| F8 | Power off and reboot on request |
| F9 | Report real memory, CPU, framebuffer and clock facts |
| F10 | Draw a mouse cursor and launch, focus, drag and close windows with it |

### 3.2 Non-functional requirements

| ID | Requirement |
|---|---|
| N1 | Written in Rust for x86_64, `no_std`, `no_main` |
| N2 | Boot on macOS Apple Silicon using QEMU |
| N3 | Build reproducibly from documented commands |
| N4 | Honest about limitations; no fabricated hardware statistics |
| N5 | Source structured for explanation in a viva |
| N6 | Visual identity fixed: NoorOS, Noor Kernel, Noor Desktop, palette, tagline |
| N7 | Remain usable despite QEMU TCG's slow emulation |

### 3.3 Constraints

- No Linux kernel, no existing OS, no browser or HTML.
- No GPU acceleration or external graphics stack.
- The display mode must be read from `BootInfo`, not assumed.
- Boot steps may only appear after the corresponding subsystem succeeds.
- The project must be demonstrable in a few minutes.

---

## 4. Design

### 4.1 Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                       Noor Desktop                           │
│  top bar · windows · taskbar · launcher                      │
│  ┌────────────┬────────────┬───────────┬──────────┬────────┐ │
│  │  Terminal  │  Monitor   │ Settings  │  About   │Launcher│ │
│  └────────────┴────────────┴───────────┴──────────┴────────┘ │
│  graphics: canvas · primitives · text/fonts · branding       │
│  shell: parser · commands                                    │
├──────────────────────────────────────────────────────────────┤
│                    Noor Kernel                               │
│  init · panic/fatal · interrupts (IDT/PIC/PIT)               │
│  drivers: framebuffer · serial · keyboard · mouse · RTC · CPUID · power│
│  memory: frame allocator · page tables · growable heap       │
├──────────────────────────────────────────────────────────────┤
│              bootloader 0.11 + BootInfo                      │
├──────────────────────────────────────────────────────────────┤
│                    BIOS / UEFI firmware                      │
└──────────────────────────────────────────────────────────────┘
```

### 4.2 Module responsibilities

| Module | Responsibility |
|---|---|
| `kernel/init` | Ordered initialisation, boot-screen steps, desktop hand-off |
| `kernel/panic`, `kernel/fatal` | Panic and CPU-exception reporting |
| `boot` | Boot screen, cached background, step panel |
| `drivers/framebuffer` | Global framebuffer and drawing access |
| `drivers/serial` | COM1 logging |
| `drivers/keyboard` | PS/2 IRQ queue and scancode decoding |
| `drivers/mouse` | 8042 auxiliary device, IRQ12 packet parser, pointer state |
| `drivers/timer` | PIT programming, ticks, uptime, sleep |
| `drivers/rtc` | CMOS date/time |
| `drivers/cpu` | CPUID vendor, brand, family/model |
| `drivers/power` | ACPI shutdown, 8042 reset |
| `interrupts` | IDT, exception handlers, PIC remap, IRQ0/IRQ1/IRQ12 |
| `memory` | Frame allocator, active page tables, growable heap, map statistics |
| `graphics` | Canvas, primitives, text, fonts, branding |
| `desktop` | Window manager, cursor, taskbar, launcher, event loop |
| `applications` | Terminal, Monitor, Settings, About, widgets |
| `shell` | Command parser and implementations |
| `util` | Byte formatting, uptime formatting, scanning helpers |

### 4.3 Boot flow

```
firmware
  └─ bootloader 0.11
       └─ kernel_main
            ├─ disable interrupts
            ├─ serial init + banner
            ├─ framebuffer from BootInfo
            ├─ frame allocator + active page tables
            ├─ initial heap pages mapped   ← before any allocation
            ├─ memory-map snapshot
            ├─ keyboard init
            ├─ mouse init (aux device, streaming)
            ├─ PIT init (100 Hz)
            └─ IDT + PIC init, enable interrupts
                 ├─ boot.ok("Interrupts initialized", ...)
                 ├─ boot.ok("Input subsystem initialized", ...)
                 ├─ boot.ok("CPU identified", ...)
                 ├─ desktop init
                 ├─ "Starting Noor Desktop..." + 420 ms
                 └─ drop boot background, enter desktop event loop
```

Each `boot.ok` call pushes the step and repaints the screen, so the animation
visualises the actual call sequence.

### 4.4 Desktop and input model

Windows are entries in a `Vec<Window>`. Each window owns an `AppId`, a rectangle
and an `AppState`. Rendering is immediate-mode: every repaint clears the
framebuffer from the cached wallpaper, draws windows bottom-to-top, then the
chrome.

Keyboard routing is centralised:

- `F1` toggles the launcher; `F2..F5` open applications.
- `Alt/Ctrl+Tab` cycles focus; `Esc` and `Ctrl+W` close the focused window.
- With a window focused, all other keys go to that application.
- The launcher is modal while open: it consumes keys and can launch or close.

Mouse input has its own path:

- IRQ12 assembles three-byte packets; `pointer()` exposes the newest position
  (motion is coalesced) while `read_event()` queues button edges (never lost).
- The desktop saves the pixels under the 13x21 cursor, draws the arrow and
  restores them when it moves, so no compositor or double buffer is needed.
- Button events are hit-tested in reverse paint order: taskbar, modal launcher,
  then windows from the top of the stack down. A title-bar press focuses and
  raises the window, then starts a drag; `[X]` closes it.

The desktop loop blocks in `hlt` and repaints only when an event or a
minute-boundary clock update marks it dirty. Pointer motion that does not drag a
window repaints only the cursor footprint. This is a deliberate performance
decision explained in §5.7.

---

## 5. Implementation

### 5.1 Build system

The repository is a Cargo workspace. The host crate `nooros` depends at build
time on the `kernel` crate:

```toml
[build-dependencies]
kernel = { path = "kernel", artifact = "bin", target = "x86_64-unknown-none" }
bootloader = "0.11.17"
```

`build.rs` reads `CARGO_BIN_FILE_KERNEL_kernel`, configures a `BootConfig`, and
builds BIOS and UEFI images. The host binary embeds the image paths through
`cargo:rustc-env` and starts QEMU with `-machine pc -m 512M -vga std`, an
`isa-debug-exit` device and either a BIOS or OVMF drive.

A subtle build issue was found and fixed: registering `kernel/src` as a
`rerun-if-changed` directory only notices added or removed files, not edits to
existing ones. A stale disk image could therefore be booted after a code change.
`build.rs` now walks the tree and registers every file, and the capture scripts
select the newest image by modification time.

### 5.2 Memory

The memory subsystem has three layers.

**Bitmap frame allocator.** `memory::frame_allocator` sizes a bitmap from the
highest usable frame in `BootInfo.memory_regions`, places the bitmap itself in a
reserved run of usable RAM, and marks every non-usable frame (kernel, bootloader,
firmware reservations, MMIO) as used. The first MiB and the bitmap's own frames
are reserved before the first allocation. Allocation returns a zeroed 4 KiB frame,
which is required for new page tables.

**Active page tables.** The bootloader maps all physical memory at
`0x0000_1000_0000_0000`. `memory::paging` reads CR3 and uses that offset to edit
the active PML4 with the `x86_64` crate's `OffsetPageTable` and `Mapper` traits.
The public operations are `map_page`, `map_page_alloc`, `unmap_page` and
`translate_addr`; missing intermediate tables are allocated on demand and TLB
entries are flushed. A boot-time self-test maps, translates and unmaps a scratch
page. All table access is serialised by one spin mutex because NoorOS is
single-core and no IRQ handler maps pages.

**Demand-grown heap.** The heap occupies a contiguous virtual range at
`0x0000_4000_0000_0000`; only 2 MiB are mapped at boot. When
`linked_list_allocator` cannot satisfy an allocation, `grow()` maps enough frames
directly after the existing heap, calls `Heap::extend` and retries. A partial
mapping is rolled back. Lock order is heap → page tables → frame allocator, and
no path takes them in the opposite order.

This design was learned partly from failure. A static 8 MiB `.bss` heap had a hard
ceiling, and an earlier formatted log call inside the allocator's free path
re-entered the allocator and deadlocked. The current code never formats while the
heap lock is held, and it releases the lock before growing. The boot screen and
wallpaper now grow the heap from 2 MiB to about 4.6 MiB, which the `memory`
command displays.

The bootloader memory map is classified into `Usable`, `Bootloader` and
`Reserved(tag)` regions. In QEMU the snapshot contains 11 regions, 506 MiB
usable, and a 12 GiB reserved PCI MMIO hole. "Total RAM" ends at the highest
usable address, so the MMIO hole is listed in the region table but is not counted
as installed RAM.

There is no demand paging, guard-page swapping or user-mode address space yet.
Page-fault handling exists so that a fault is reported rather than silently
resetting the machine.

### 5.3 Interrupts and devices

The IDT is built with `x86_64` 0.15.5 and the PIC remapped with `pic8259` 0.11.
IRQ0 calls `timer::on_interrupt`, IRQ1 pushes a decoded key. Exceptions format a
message for serial and attempt a fatal screen using `try_lock`, avoiding a
deadlock if the fault happened while the framebuffer lock was held.

Keyboard decoding uses `pc-keyboard` 0.9 with scancode set 1 and a US 104-key
map. Events are queued in a fixed 64-slot ring; the IRQ never allocates. The
desktop drains the queue with interrupts disabled, so queue access is safe
without a separate lock.

The mouse uses the same 8042 controller but the auxiliary channel. During
initialisation the kernel enables the aux port (`0xA8`), sets the aux-interrupt
bit in the controller configuration byte while preserving the keyboard bits, and
sends `0xF6`/`0xF4` through `0xD4` to restore defaults and start streaming. IRQ12
is the slave PIC's line 4, mapped to vector `0x2C`; the master unmasks IRQ2 so
the cascade can reach the CPU. The handler checks the status port before reading
`0x60`, which makes a spurious interrupt harmless. Packets are three bytes: the
first carries button and sign bits, the next two the X/Y deltas (PS/2 Y is
inverted relative to screen Y). Overflow packets are discarded, and a first byte
without the "always 1" bit restarts synchronisation.

The RTC is read from CMOS, including the status-B binary/BCD and 12/24-hour
bits. CPU facts come from CPUID; the brand string is only read if leaf
`0x8000_0000` reports support for the required leaves.

### 5.4 Graphics engine

`Canvas` is the only component that knows the framebuffer. It handles bounds,
stride, 3/4-byte pixels, RGB/BGR/greyscale encoding and alpha blending against
the existing contents.

The renderer uses:

- coalesced text runs (one rectangle per set-bit run per glyph row),
- a 3-byte `fast_hline` using `as_chunks_mut::<3>()`,
- rounded rectangles built from horizontal spans with corner tests,
- block-sampled radial glows (6x6 sample blocks),
- cached boot and desktop wallpapers blitted from heap buffers,
- a save-under software cursor: the 13x21 pixels under the arrow are copied
  out, the arrow is drawn, and those pixels are restored before the next move.

The palette is centralised in `graphics/colors.rs`, so the visual identity is
consistent across boot, desktop and applications.

### 5.5 Boot screen honesty

`BootScreen` has no timer that fabricates progress. `ok(label, detail)` is called
after the step completed and immediately repaints. The eight steps in 0.1.0 are:

1. Bootloader hand-off
2. Framebuffer initialised
3. Kernel heap initialised
4. Memory map loaded
5. Interrupts initialised
6. Input subsystem initialised
7. CPU identified
8. Desktop environment initialised

The header prints the real framebuffer mode reported by the bootloader. The
footer prints the kernel version, tagline, host architecture label and uptime.

### 5.6 Applications

**Terminal.** Owns a 400-line scrollback of coloured lines, an input buffer, a
history index and a scroll offset. It wraps text to the available columns,
rendering the prompt only on the first physical line of a logical line. It
supports Backspace, Delete, Left/Right, Home/End, Up/Down history, `PgUp/PgDn`
scrollback and `clear`. The viewport is 54 columns by 18 rows.

**Monitor.** A dashboard of four cards (Operating System, Processor, Memory,
System) plus a scrollable physical-memory table. Values are pulled live from the
kernel; the keyboard IRQ count includes dropped events.

**Settings.** Five sidebar sections, all read-only in 0.1.0. The General page
declares "Noor Settings is informational in this release" rather than offering
switches that do nothing.

**About.** Branding, version, project description, and the crates that make the
system possible.

**Launcher.** A modal list with icons, descriptions and F-key hints, driven by
`Up/Down/Enter/Esc`.

### 5.7 Performance work

QEMU TCG emulates every x86_64 instruction on the arm64 host, so a naive
immediate-mode desktop was unusable. The following changes were made and
measured qualitatively:

| Change | Effect |
|---|---|
| Cache wallpaper in a heap buffer and `blit` | Removes gradients/glows from every frame |
| Coalesce text pixels into row runs | Large reduction in per-character rectangles |
| 3-byte-per-pixel `fast_hline` | Faster window bodies and taskbar |
| Block-sample radial glows | Cheaper wallpaper generation |
| Event-driven loop + `hlt` | No idle repainting at 60 Hz |
| Opaque launcher scrim | Avoids full-screen per-pixel alpha |

Approximate full-frame times observed: ~250 ms with one window, ~900 ms with
four, ~1.2 s with the launcher. Consequently screenshots wait 2.5 s after each
keyboard action.

### 5.8 Power control

Shutdown writes ACPI `SLP_TYP | SLP_EN` to `PM1a_CNT` at port `0x604`, then tries
`0xB004` for older chipsets, then QEMU's `isa-debug-exit` device at `0xF4`.
Reboot writes `0xFE` to the 8042 command port and falls back to a triple fault.
Both were tested from the running terminal.

---

## 6. Testing and evidence

### 6.1 Method

Because the artefact is an operating system, testing is behaviour-driven:

1. build the release image,
2. boot it in QEMU with a monitor socket and serial log,
3. send real keyboard events through QEMU's `sendkey` and real mouse events
   through `mouse_move`/`mouse_button`,
4. capture the framebuffer with `screendump`,
5. inspect the serial log for boot steps and failures.

`scripts/interact.py` automates the full sequence and writes seventeen PNGs.
`scripts/capture.py` handles boot screenshots and UEFI/BIOS experiments. These
are integration tests against the actual machine, not mocks.

### 6.2 Results

| Test | Method | Result |
|---|---|---|
| BIOS boot | QEMU `-machine pc`, BIOS image | Pass: boot screen → desktop |
| UEFI boot | OVMF + UEFI image | Pass: boot screen → desktop |
| Boot screen steps | Serial log cross-check | Pass: 8 real steps, no fake lines |
| Framebuffer mode | Serial + boot screen | Pass: 1280x720, 3 bpp, Bgr |
| Terminal launch | `F2` | Pass |
| `help` | type + Enter | Pass: all ten commands listed |
| `info` | type + Enter | Pass: CPUID, memory, heap, uptime, RTC |
| `memory` | type + Enter | Pass: 11 regions, heap, paging note |
| `version` / `uptime` | type + Enter | Pass |
| Unknown command | type `nonsense` | Pass: exact required message in red |
| Monitor | `F3` | Pass |
| Settings | `F4` | Pass |
| About | `F5` | Pass |
| Window cycling | `Alt+Tab` | Pass: focus moves |
| Launcher | `F1` | Pass: modal list, chrome visible |
| Cursor motion | `mouse_move` | Pass: tracks injected deltas 1:1 |
| No cursor trails | move away and back, compare frames | Pass: frames identical |
| Taskbar launch | click the Terminal icon | Pass |
| Title-bar drag | press, move (+100, +80), release | Pass: window follows |
| Close button | click `[X]` | Pass: window removed |
| Click-to-raise | click a partly covered title bar | Pass: window moves to front |
| Launcher by mouse | click taskbar Launcher, then a row | Pass |
| Shutdown | `shutdown` | Pass: QEMU exits 0 |
| Reboot | `reboot` | Pass: fresh desktop, uptime 1 s |
| Clippy | `cargo clippy --release` | Pass: no warnings |

### 6.3 Screenshot evidence

| File | Content |
|---|---|
| `00-boot.png` | Boot screen after all eight real steps |
| `01-desktop.png` | Welcome card, top bar, taskbar |
| `02-terminal.png` | Terminal welcome and prompt |
| `03-help.png` | Command list |
| `04-info.png` | Live system information |
| `05-memory.png` | Physical map and heap |
| `06-version-uptime.png` | Version and uptime |
| `07-unknown.png` | Unknown-command error |
| `08-monitor.png` | Monitor cards and memory table |
| `09-settings.png` | Settings General page |
| `10-about.png` | About NoorOS |
| `11-alt-tab.png` | Focus cycling |
| `12-launcher.png` | Launcher with desktop chrome |
| `13-mouse-launcher.png` | Pointer over a launcher row |
| `14-mouse-drag.png` | Title-bar drag in progress |
| `15-mouse-close.png` | Window closed with `[X]`, focus moved |
| `16-mouse-zorder.png` | Click-to-raise z-order |
| `17-mouse-taskbar.png` | App launched from the taskbar |

### 6.4 Serial evidence (excerpt)

```
==============================================
 NoorOS 0.1.0 - Noor Kernel
 A Light Beyond Computing.
==============================================
[boot] entered kernel; bootloader_api 0.11.17, kernel ELF at 0x1000000
[boot] framebuffer 1280x720, stride 1280, 3 bpp, Bgr
[kernel] memory init: physical memory offset 0x100000000000
[kernel] frame bitmap: 131040 frames, 16384 bytes at physical 0x14EA000
[kernel] frame allocator ready
[kernel] frame allocator self-test: allocate/deallocate OK
[kernel] active page tables captured from CR3
[kernel] initial heap pages mapped
[kernel] paging: active PML4 at 0x10A9000, physical offset 0x100000000000
[kernel] frames: 129523 usable of 131040 addressable, 518 used, 129005 free
[kernel] heap: 2 MiB mapped at 0x400000000000, 2 MiB usable, 0 B used
[kernel] paging self-test: map/translate/unmap OK
[ OK ] Framebuffer initialized - 1280x720 - 3 bpp - Bgr
[ OK ] Virtual memory online - 129005 frames free
[kernel] memory map: 11 regions, 506 MiB usable, 5 MiB reserved, 511 MiB total
[ OK ] Memory map loaded - 11 regions - 506 MiB usable
[mouse] PS/2 auxiliary device streaming on IRQ12
[ OK ] Interrupts initialized - IDT loaded, PIC remapped, PIT at 100 Hz
[ OK ] Input subsystem initialized - PS/2 keyboard IRQ1, mouse IRQ12
[ OK ] CPU identified - AuthenticAMD - family 15 model 107
[ OK ] Desktop environment initialized - Noor Desktop 0.1.0
[desktop] handing over to Noor Desktop
```

The UEFI run produced the same boot and desktop with OVMF as firmware. Full
captures are stored as [`docs/evidence/bios-boot.log`](evidence/bios-boot.log)
and [`docs/evidence/uefi-boot.log`](evidence/uefi-boot.log).

### 6.5 What was not tested

- Physical hardware. NoorOS has only been booted in QEMU; real firmware,
  monitors, keyboards and mice may expose assumptions.
- Secure Boot, network boot, USB boot and non-OVMF UEFI implementations.
- Long-run stability beyond the demo session.
- Non-US keyboard layouts and non-ASCII input.
- PS/2 mice with wheels, extra buttons or unusual packet modes; QEMU's
  emulated three-button mouse is the only device exercised.
- SMP, suspend/resume and hot-plug.
- The UEFI OVMF download path in an offline environment (it falls back to BIOS
  with a warning).

No formal unit tests exist, because almost everything meaningful requires the
machine. The integration scripts are the test suite.

---

## 7. Evaluation

### 7.1 Against objectives

| Objective | Outcome |
|---|---|
| O1 boot chain | Met: BIOS and UEFI images boot |
| O2 kernel-drawn desktop | Met: all UI pixels come from `Canvas` |
| O3 honest boot screen | Met: steps follow real calls |
| O4 terminal commands | Met: all required commands, exact error text |
| O5 applications | Met: Monitor, Settings, About, Launcher |
| O6 honest reporting | Met: CPUID/RTC/framebuffer/memory are real; limits labelled |
| O7 mouse | Met: cursor, taskbar launch, focus, drag, `[X]`, launcher rows |
| O8 reproducibility | Met: `Makefile`, scripts, docs, clean releases |
| O9 documentation | Met: this report plus setup, architecture, viva and demo docs |

### 7.2 Strengths

- A genuine `no_std` kernel with its own boot chain, IDT, timer and input path.
- A consistent visual identity implemented procedurally, not with image assets.
- An honest boundary: missing features are labelled, not simulated.
- Reproducible evidence: every screenshot comes from the running image.
- A codebase small enough to explain line by line in a viva.

### 7.3 Weaknesses

- No demand paging or user mode; this is a single-address-space kernel.
- Full-frame redraws limit headroom; only the cursor has a partial-repaint path.
- The heap grows but never shrinks, and page-table frames are not reclaimed.
- The mouse supports three buttons and relative motion only: no wheel, no hover
  feedback and no cursor acceleration.
- Rendering speed is dominated by QEMU TCG.
- No automated assertions over pixels; evidence is reviewed visually.

### 7.4 Lessons learned

- Initialisation ordering is a correctness property, not a style choice. The
  heap-before-allocation rule was learned from a real deadlock.
- Build systems can lie. A stale image looked like a rendering bug until the
  `rerun-if-changed` behaviour was understood.
- Performance must be designed for the deployment target. A desktop that is fine
  natively was unusable under emulation until the wallpaper was cached and
  repainting became event-driven.
- A pointer needs two different policies: coalesce motion, because only the
  newest position matters, but queue button edges, because every click carries
  an intention.
- Honest reporting is more work than fake data, but it makes the demo credible.

---

## 8. Future work

1. **Demand paging.** Guard pages and lazy mapping on top of the new frame
   allocator and page tables.
2. **User mode.** Separate address spaces, a syscall interface and at least two
   cooperating processes.
3. **Damage tracking.** Repaint only changed rectangles; add double buffering.
4. **Storage and files.** A ramdisk or virtio-blk driver and a small read-only
   filesystem, then a text editor.
5. **SMP.** APIC, per-CPU data and a scheduler.
6. **Better firmware support.** A boot menu, UEFI runtime services and physical
   hardware testing.

[`docs/roadmap.md`](roadmap.md) turns this list into ten gated milestones (USB
boot media, real-hardware hardening, USB HID, demand paging, user mode, storage,
installer, everyday desktop, networking) with acceptance tests, effort estimates
and a risk register.

---

## 9. Conclusion

NoorOS 0.1.0 demonstrates that a complete, branded, interactive graphical
environment can be built directly on bare x86_64 hardware in a small Rust
kernel. It boots through real firmware, installs real interrupt handling,
allocates from a real heap, reads real hardware facts, and draws every pixel of
its desktop, terminal and applications itself. It also states plainly what it
does not do. The result is a coherent teaching artefact: small enough to
understand, large enough to be a real operating system, and honest enough to
survive a viva.

---

## References

1. rust-osdev. *bootloader* crate, version 0.11.17.
   <https://github.com/rust-osdev/bootloader>
2. rust-osdev. *bootloader_api* documentation, version 0.11.17.
3. Rust project. *The Rust Programming Language*, `no_std` and `alloc` chapters.
4. Gerd C. *x86_64* crate, version 0.15.5.
5. *pic8259* crate, version 0.11.
6. *pc-keyboard* crate, version 0.9.
7. *linked_list_allocator* crate, version 0.10.6.
8. *font8x8* crate, version 0.3.1.
9. Intel Corporation. *Intel 64 and IA-32 Architectures Software Developer's
   Manual*, volumes 1-3.
10. OSDev Wiki. *Interrupt Descriptor Table*, *Programmable Interval Timer*,
    *PS/2 Keyboard*, *Framebuffer*.
11. QEMU project. *QEMU Emulator User Documentation* and `sendkey`,
    `screendump`, `isa-debug-exit`.
12. ACPI Specification. *Fixed ACPI Description Table* and PM1 control.

---

## Appendix A - Build and run

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --release
cargo run --release             # BIOS in a QEMU window
cargo run --release -- uefi     # UEFI with OVMF
```

## Appendix B - Demo shell session

```text
noor@nooros:~$ help
noor@nooros:~$ info
noor@nooros:~$ memory
noor@nooros:~$ version
noor@nooros:~$ uptime
noor@nooros:~$ nonsense
Command not found. Type 'help' for available commands.
noor@nooros:~$ shutdown
```

## Appendix C - Repository tree

```
Cargo.toml, build.rs, src/main.rs
kernel/Cargo.toml
kernel/src/{main.rs, kernel/, boot/, drivers/, interrupts/, memory/,
            graphics/, desktop/, applications/, shell/, util.rs}
scripts/{capture.py, interact.py, run.sh}
docs/{setup-macos.md, architecture.md, university-report.md,
      viva-questions.md, demo-guide.md, roadmap.md}
docs/evidence/{bios-boot.log, uefi-boot.log}
screenshots/demo/*.png
```
