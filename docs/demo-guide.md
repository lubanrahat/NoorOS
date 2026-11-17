# NoorOS demo guide

A prepared seven-to-eight-minute demonstration of NoorOS 0.1.0, plus the evidence
to fall back on if QEMU misbehaves.

---

## 1. Before the demo

Run these once, not in front of the audience:

```bash
cd NoorOS
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --release
cargo clippy --release          # optional confidence check
```

Check that the images exist:

```bash
ls target/release/build/nooros/*/out/nooros-bios.img
```

Check QEMU:

```bash
qemu-system-x86_64 --version
```

Open the screenshot folder so the evidence is one click away:

```bash
open screenshots/demo
```

Decide whether to demo BIOS (default, fastest) or UEFI. BIOS is the safe choice;
UEFI needs a one-time OVMF download and is best mentioned rather than attempted
live unless already cached in `target/ovmf/`.

## 2. The demo script

### Step 1 - Boot (about 45 seconds)

```bash
cargo run --release
```

Say:

> NoorOS is a real x86_64 kernel written in Rust. There is no Linux underneath
> and no browser: the kernel is the program. QEMU is emulating the CPU because
> this is an Apple Silicon Mac, so boot takes a few seconds.

Point at the boot screen:

- the gold Noor mark and the wordmark,
- the eight `[ OK ]` lines; explain they appear only after each subsystem
  finishes, in the same order as `kernel/src/kernel/init.rs`,
- the framebuffer line: `1280x720 - 3 bpp - Bgr` is reported by the bootloader,
  not assumed,
- the footer: kernel version, tagline, architecture, uptime.

### Step 2 - Desktop (about 30 seconds)

The welcome card appears.

> This is Noor Desktop. The wallpaper, panels, icons and text are drawn by the
> kernel into the framebuffer. The top bar shows uptime, kernel state,
> architecture and the CMOS clock; the taskbar shows the applications and a
> larger clock.

Mention the design constraint:

> The desktop is driven by a PS/2 mouse on IRQ12 as well as the keyboard: the
> kernel assembles three-byte packets, draws a 12x19 arrow with save-under so it
> leaves no trails, and hit-tests the taskbar, windows and launcher. Every action
> is still reachable from the keyboard.

### Step 3 - Noor Terminal (about 2 minutes)

Press `F2`, wait a moment, then type:

```text
help
info
memory
version
uptime
nonsense
```

For each, briefly say what is real:

- `help` lists the ten commands.
- `info` is live: CPUID brand, framebuffer mode, memory totals, frame usage,
  heap usage, uptime, RTC clock. Point out `Virtual memory  active (bitmap
  frames + page tables)`.
- `memory` shows physical RAM, the bitmap frame allocator, the demand-grown heap
  and live virtual-to-physical translations for the heap, a kernel symbol and an
  unmapped probe.
- `version` shows the target triple and allocator.
- `uptime` shows PIT ticks and timer interrupts.
- `nonsense` produces the required red message:
  `Command not found. Type 'help' for available commands.`

Optional: show `echo A Light Beyond Computing.` and `clear`.

Keyboard extras worth one sentence: `Up/Down` recalls history, `PgUp/PgDn`
scrolls the 400-line scrollback.

### Step 4 - Noor Monitor (about 45 seconds)

Press `F3`. Wait for the repaint.

> Monitor reads the same kernel state through a different view: OS, processor,
> memory and system cards, plus the full physical map. The CPU row comes from
> CPUID; the vendor is `AuthenticAMD` because QEMU reports that.

Press `Down` a few times to scroll the memory table.

### Step 5 - Noor Settings (about 45 seconds)

Press `F4`.

> Settings is deliberately read-only in 0.1.0. It reports the actual
> configuration instead of offering switches that do nothing: version, kernel,
> architecture, display mode, memory, heap, CPUID and palette.

Press `Down` to move through General, Appearance, Display, System and About
NoorOS.

### Step 6 - About and window management (about 45 seconds)

Press `F5`.

> About NoorOS identifies the project, version, language and the crates the
> kernel is built on.

Press `Alt+Tab`:

> Windows are a `Vec`; focus cycles, and the focused window gets the brighter
> title bar and accent strip. `Esc` or `Ctrl+W` closes it.

### Step 7 - Noor Launcher (about 30 seconds)

Press `F1`.

> The launcher is modal: an opaque scrim, a list of applications with icons and
> keycaps. `Up/Down` select and `Enter` opens.

Press `Down` twice, then `Enter` to open Noor Monitor from the launcher. Then
`Esc` to close it.

### Step 8 - The mouse (about 1 minute)

Move the pointer across the desktop:

> Motion is coalesced to the newest position and only the 13x21 cursor footprint
> is repainted, which is why the arrow follows the hand even under TCG. The
> pixels under the cursor were saved before it was drawn and are restored when it
> moves, so there are no trails.

Then, without touching the keyboard:

1. click the `Terminal` icon in the taskbar - it opens and focuses,
2. drag its title bar - the window follows the pointer and stays above the
   top bar,
3. click the red `[X]` - the window closes,
4. click the title bar of a window that is partly behind another - it comes to
   the top of the z-order,
5. click the taskbar `Launcher` icon and then click any row - that application
   opens.

> Hit-testing runs in reverse paint order: taskbar, then the modal launcher, then
> windows from the top of the stack down. The close button is a real rectangle
> shared by drawing and hit-testing, so the button on screen is the button that
> reacts.

### Step 9 - Shutdown (about 20 seconds)

Press `F2` if the terminal is not focused, type:

```text
shutdown
```

QEMU exits with status 0.

> The kernel writes the ACPI sleep command to PM1a_CNT and falls back to QEMU's
> debug-exit device. No host process is left behind.

### Optional - Reboot

The launcher passes `-no-reboot` by default, so a demo reboot ends QEMU. To show
a real reset, start with `cargo run --release -- bios --allow-reboot` (see
`docs/setup-macos.md`) and run `reboot`; the guest returns to the boot screen and
a fresh desktop with uptime around one second.

## 3. If the live demo fails

Stay calm; the evidence is part of the deliverable.

1. Show the boot screenshot: `screenshots/demo/00-boot.png`.
2. Show the terminal and applications: `03-help.png`, `04-info.png`,
   `05-memory.png`, `08-monitor.png`.
3. Show the launcher: `12-launcher.png`.
4. Show the mouse sequence: `13-mouse-launcher.png`, `14-mouse-drag.png`,
   `15-mouse-close.png`, `17-mouse-taskbar.png`.
5. Read the serial excerpt from `docs/university-report.md` §6.4.
6. Explain the failure if it is known: wrong `cargo` in `PATH`, stale image,
   QEMU missing, or TCG slowness mistaken for a hang.

Do not claim a feature works if the live attempt failed; describe it as
implemented and verified during testing, and show the captured evidence.

## 4. Talking points that earn marks

- **Real vs simulated.** The kernel owns the framebuffer; there is no HTML,
  Electron, Linux or userspace process. The host binary only launches QEMU.
- **Init ordering.** Heap before allocation; interrupts disabled until the IDT
  and PIC are ready.
- **Honesty.** Missing demand paging and acceleration are labelled in the UI and
  documented, not faked.
- **Hardware facts.** CPUID, CMOS RTC, PIT, PS/2 devices, framebuffer info and
  the bootloader memory map are all read at runtime.
- **Engineering tradeoffs.** Event-driven rendering, cached wallpapers and a
  cursor-only repaint path exist because TCG is slow; this is a deliberate design
  response.
- **Input design.** Mouse motion is coalesced but button edges are queued, so a
  slow frame cannot make the pointer crawl or swallow a click. The cursor's
  save-under buffer is what removes trails without a compositor.
- **Testability.** Keyboard and mouse events are injected through QEMU's monitor
  and screenshots are captured from the emulated framebuffer, so evidence is
  reproducible.

## 5. Questions to expect

Use `docs/viva-questions.md` for full answers. The most likely ones:

1. Why `no_std`/`no_main`?
2. Why the `bootloader` crate?
3. How does virtual memory work, and what is still missing?
4. How does a key press reach the terminal?
5. Why does the desktop not animate?
6. How do you know the boot screen is not fake?
7. What happens on a page fault or panic?
8. What did you test, and what did you not test?

## 6. Final checklist

- [ ] `cargo build --release` succeeds.
- [ ] `cargo run --release` opens a QEMU window and reaches the desktop.
- [ ] `F2` opens the terminal; `help` and `nonsense` behave.
- [ ] `F1`, `F3`, `F4`, `F5` all open the expected windows.
- [ ] The pointer moves, a taskbar click opens an app, a drag moves a window, and
  `[X]` closes it.
- [ ] `shutdown` exits QEMU.
- [ ] Screenshots are available offline.
- [ ] Serial log excerpt is available.
- [ ] `PATH` points at rustup, not Homebrew.

## 7. Screenshot index

| Image | Use |
|---|---|
| `00-boot.png` | Real boot steps |
| `01-desktop.png` | Hero / welcome |
| `02-terminal.png` | Terminal identity |
| `03-help.png` | Command set |
| `04-info.png` | Live system facts |
| `05-memory.png` | Physical RAM, frames, heap and live translation |
| `06-version-uptime.png` | Version and clock |
| `07-unknown.png` | Required error message |
| `08-monitor.png` | Monitor dashboard |
| `09-settings.png` | Settings sections |
| `10-about.png` | Project identity |
| `11-alt-tab.png` | Window focus |
| `12-launcher.png` | Launcher with chrome |
| `13-mouse-launcher.png` | Pointer over a launcher row |
| `14-mouse-drag.png` | Title-bar drag |
| `15-mouse-close.png` | Window closed with `[X]` |
| `16-mouse-zorder.png` | Click-to-raise z-order |
| `17-mouse-taskbar.png` | App launched from the taskbar |
