# NoorOS roadmap - from a QEMU demo to a small real operating system

**Status:** planning document. Only M0 is implemented and verified. Everything
else in this file is a plan, not a claim about the current code.

**Baseline:** NoorOS 0.1.0 (see `docs/university-report.md` and
`screenshots/demo/`).

---

## 0. Read this first: three questions that get mixed together

When someone says "I want a real operating system like Ubuntu", they usually
mean three different things. They have very different costs.

| Question | Short answer | Where it is handled |
|---|---|---|
| Can I boot it from a USB stick on a real PC? | **Yes, almost today.** The build already produces a raw disk image with a BIOS boot sector and a UEFI GPT/ESP partition. `dd` it to a stick. | [M2](#m2-boot-media-and-the-vm-matrix-v021-portable) |
| Can I use a mouse instead of the keyboard? | **Yes, one milestone.** PS/2 mouse on IRQ12 plus a software cursor and hit-testing. | [M1](#m1-pointer-input-v02-usable) |
| Can I *install* it to an internal disk like Ubuntu? | **Not yet.** An installer needs storage drivers, a partitioner, a filesystem and a boot-entry writer. It is the last milestone before a usable release, not the first. | [M7](#m7-storage-v06-persistent) - [M8](#m8-installer-v07-installable) |

A fourth question is usually implied and should be answered honestly:

> "Can it be as capable as Ubuntu?"

No. Ubuntu is tens of millions of lines and a large paid engineering
organisation. What is achievable by one person is a **small, coherent, honest
OS**: it boots real hardware, installs to a disk, keeps files, runs several
isolated programs, and has a graphical desktop with a mouse. That is a genuine
operating system, and it is a multi-month project - not a two-day one. This
document is the map for that project.

### What "small OS, not Ubuntu" means concretely

The target (v1.0) is:

- boots on real x86_64 hardware through UEFI **and** legacy BIOS,
- installs from a USB stick to an internal disk, and boots with the stick removed,
- has virtual memory, user mode, and more than one process running at once,
- has a read/write filesystem that a normal PC can also mount,
- has a graphical desktop driven by mouse and keyboard,
- can edit and save a file, reboot, and open it again,
- is fully documented and reproducible from source.

It is **not** the target to run Linux binaries, have a package manager, a
browser, GPU acceleration, audio, or networking in v1.0. Those are listed as
explicit anti-goals in [section 8](#8-anti-goals-scope-discipline).

---

## 1. Where 0.1.0 actually stands

| Subsystem | 0.1.0 | Notes |
|---|---|---|
| Boot | BIOS + UEFI via `bootloader` 0.11.17 | Raw images are already dd-able |
| Display | Linear framebuffer, 24/32 bpp, software rendering | QEMU supplies 1280x720 |
| Interrupts | IDT, 8259 PIC remapped, IRQ0 (PIT 100 Hz), IRQ1 (keyboard), IRQ12 (mouse) | No APIC, no SMP |
| Input | PS/2 keyboard and PS/2 mouse with a graphical pointer | No USB HID, no wheel |
| Memory | Bitmap frame allocator, active page tables, demand-grown heap | M5 core landed; no demand paging or guard pages |
| Concurrency | One address space, one thread of control | No processes, no scheduler |
| Storage | None | No disk, no filesystem |
| Desktop | Immediate-mode windows, taskbar, launcher, mouse hit-testing | Full-frame redraw; cursor-only fast path |
| Shell | 10 commands, scrollback, history | Static command set |
| Power | ACPI port `0x604` + isa-debug-exit; 8042 reset | Shutdown only works under QEMU |
| Tests | Scripted QEMU integration + screenshots | No unit tests for pure logic |

The honest summary: **0.1.0 is a real kernel with a real GUI, but it is a
single-address-space demo, not an installable system.** Every remaining gap is
named below.

M1 (pointer input) was implemented on top of 0.1.0 ahead of the original 0.2
schedule; its status is recorded in the milestone below.

---

## 2. Version plan

| Version | Name | Milestones | What a user can do |
|---|---|---|---|
| 0.1.0 | *First Light* | M0 | Boot in QEMU, keyboard desktop, shell |
| 0.2 | *Usable* | M1 | Use the desktop with a mouse |
| 0.2.1 | *Portable* | M2 | Boot from a USB stick; run in a VM |
| 0.3 | *On Metal* | M3 | Boot, click, and shut down on real PCs |
| 0.4 | *Protected* | M5 | Kernel heap grows; faults are contained |
| 0.5 | *Multitasking* | M6 | Several isolated user programs at once |
| 0.6 | *Persistent* | M7 | Files survive a reboot |
| 0.7 | *Installable* | M8 | Install to an internal disk and boot it |
| 0.8 | *Everyday* | M9 | Edit and save a document with the mouse |
| 1.0 | *NoorOS* | M1-M9 + polish | A small, honest, installable OS |
| 1.x | *Connected* | M10 | Network (optional, high cost) |

Milestones are gated: a version is released only when its acceptance list in
this document passes on the stated test matrix.

---

## 3. Milestones

Effort is given twice: **focused** = 30-40 hours/week on this alone;
**part-time** = ~10 hours/week alongside study. Both are honest ranges for a
single developer working in Rust on an unfamiliar subsystem.

### M1: Pointer input (v0.2 Usable)

**Status:** implemented in 0.1.0. `drivers/mouse.rs` brings up the auxiliary
device, the IRQ12 handler assembles packets, `desktop/cursor.rs` draws a
save-under arrow, and `Desktop::handle_mouse` covers taskbar, launcher,
title-bar drag and `[X]`. The cursor-only repaint path replaces damage
rectangles for pointer motion; general window damage tracking is still open.

**Goal:** the desktop can be operated with a mouse, with no keyboard required
for opening, focusing, moving or closing windows.

**Work items**

1. **PS/2 auxiliary device bring-up** (`drivers/mouse.rs`)
   - enable the aux port: `0x64 <- 0xA8`
   - read the controller config byte (`0x64 <- 0x20`), set bit 1 (aux interrupt
     enable), ensure bit 5 (aux clock disable) is clear, write it back with
     `0x64 <- 0x60`
   - `0x64 <- 0xD4` then `0x60 <- 0xF6` (defaults), then `0x60 <- 0xF4`
     (enable data reporting)
   - unmask IRQ2 (cascade) on the master PIC and IRQ12 on the slave
2. **IRQ12 handler** - assemble 3-byte packets. Byte 0 carries overflow bits,
   sign bits and the three buttons; bytes 1-2 are signed X and Y deltas
   (positive Y is up, so the cursor's Y must be *subtracted*). Resynchronise if
   the "always 1" bit in byte 0 is clear.
3. **Cursor** - a 12x19 arrow drawn as a small alpha-blended sprite. Because
   there is no double buffer, the renderer must save the pixels under the
   cursor and restore them before drawing the next position.
4. **Hit-testing** - `Desktop` gains a `dispatch_mouse(event)`: walk windows from
   top of z-order downward, test title bar (begin drag), close glyph (close),
   content (focus), taskbar items (open/focus), launcher rows (select/activate).
5. **Damage rectangles** - at minimum for cursor move and window drag; ideally
   for window open/close too. This replaces full-frame redraw in the common
   case and is what makes a mouse usable under TCG.
6. **Keep the keyboard path** - every action stays reachable from the keyboard.
   The mouse is additive, not a replacement.

**Acceptance**

- With the keyboard untouched, open Terminal from the launcher, drag it, focus
  Monitor by clicking it, close both with their close buttons.
- Cursor leaves no trails after a slow figure-eight drag.
- A click on a window behind another window focuses the front-most window at
  that point, never one hidden behind it.
- Dragging is smooth enough to be usable in QEMU TCG (damage redraws only).
- `cargo clippy --release` clean; scripted screenshot proves mouse-only operation.

**Risks:** packet desynchronisation (mitigated by the sync-bit check and a
resync counter); TCG slowness (mitigated by damage rectangles).

**Effort:** 3-5 focused days / 1-2 part-time weeks.

---

### M2: Boot media and the VM matrix (v0.2.1 Portable)

**Goal:** anyone can put NoorOS on a USB stick or run it in a VM without
reading the source.

**Work items**

1. `scripts/make-usb.sh` and a `make usb DEVICE=/dev/diskN` target:
   - list candidate devices and refuse anything not removable,
   - require the device name to be typed back as confirmation,
   - image must be at least as large as the target? (no - it is smaller, so
     check the stick is large enough and warn that the rest is unallocated),
   - write with `bs=4m`, `conv=fsync`, then `diskutil eject` on macOS.
2. `make vdi` / `make vhd`: convert the UEFI image with
   `VBoxManage convertfromraw` or `qemu-img convert`, so VirtualBox and
   Hyper-V users have a ready disk.
3. Document the honest hardware requirements: UEFI with Secure Boot **off**,
   or legacy BIOS; a PS/2 keyboard or firmware legacy-USB emulation.
4. Ship two images and say which is which: `nooros-uefi.img` (modern PCs,
   VirtualBox with EFI enabled) and `nooros-bios.img` (legacy BIOS).
5. Add checksums (`sha256`) and a short release note per version.

**Acceptance**

- Stick written on macOS boots to Noor Desktop on at least one real UEFI PC.
- `nooros.vdi` boots in VirtualBox with `--firmware efi` on an x86_64 host.
- The script refuses to write to a non-removable disk.

**Risks:** firmware quirks; mitigated by documenting the matrix and keeping
BIOS and UEFI images separate rather than inventing a hybrid.

**Effort:** 2-3 focused days / ~1 part-time week.

---

### M3: Real hardware hardening (v0.3 On Metal)

**Goal:** the system behaves correctly on physical machines, not just QEMU.

**Work items**

1. **ACPI power control** (replaces the `0x604` guess)
   - `BootInfo` provides the RSDP address; validate the `RSD PTR ` signature and
     checksum, follow XSDT/RSDT to the FADT (`FACP`).
   - Read `PM1a_CNT_BLK`, `PM1b_CNT_BLK`, `PM1_CNT_LEN`, `RESET_REG`/`RESET_VALUE`.
   - Obtain the S5 sleep type from the DSDT's `\_S5` package (a small AML
     scanner), or as a fallback try `SLP_TYP` values 0-7 - writing an invalid
     type is harmless and the correct one powers the machine off.
   - Keep the QEMU fallbacks only when ACPI is absent.
   - Reset via the FADT reset register first, then 8042 `0xFE`, then triple fault.
2. **Framebuffer robustness** - if `BootInfo.framebuffer` is `None`, print a
   clear message on serial and halt with a readable state instead of panicking
   into a renderer that needs a screen. Handle 16 bpp and 32 bpp modes, and
   non-1280x720 modes, with a layout that scales.
3. **Missing input** - detect a controller that does not answer the 8042 self
   test; show "no PS/2 input device - firmware legacy USB may be disabled".
4. **Real-hardware test matrix** as a checked-in document:
   | Machine | Firmware | Result |
   |---|---|---|
   | QEMU `-machine pc` | SeaBIOS | pass |
   | QEMU + OVMF | UEFI | pass |
   | VirtualBox (EFI) | UEFI | ? |
   | physical desktop | UEFI, Secure Boot off | ? |
   | physical laptop | UEFI legacy USB on | ? |
5. **Caching** - map the framebuffer write-combining once paging exists (M5);
   until then note the performance cost.

**Acceptance**

- Boots, accepts mouse and keyboard, and powers off cleanly on two different
  physical machines.
- Pulling the framebuffer out of `BootInfo` produces a diagnostic, not a panic.

**Risks:** ACPI/AML parsing is fiddly; mitigations are the try-each-SLP_TYP
fallback and keeping the QEMU path working. Hardware access for testing is the
main real risk - plan for at least one spare x86 PC.

**Effort:** ~1 focused week / ~1 part-time month.

---

### M4: USB HID input (v0.3.1, optional but recommended)

**Goal:** keyboard and mouse work on machines with no PS/2 emulation.

**Work items**

1. PCI enumeration - legacy `0xCF8/0xCFC` first (simple, works everywhere),
   MMCONFIG/ECAM later.
2. xHCI bring-up: map the MMIO BAR, halt and reset the controller, set up the
   command ring and event ring, enable device slots, reset ports, address the
   device, read configuration descriptors, configure the interrupt IN endpoint.
3. Use **HID boot protocol** (keyboard report 8 bytes, mouse report 3-4 bytes)
   instead of parsing arbitrary report descriptors. Firmware can put the device
   in boot protocol (`SET_PROTOCOL`), which removes most of the work.
4. Feed resulting events into the existing fixed-size event queue, so the
   desktop does not change.

**Acceptance**

- Under `-device qemu-xhci -device usb-kbd -device usb-mouse`, both devices work
  with the PS/2 controller absent.

**Risks:** this is the largest driver in the project and the easiest place to
lose weeks. It is deliberately placed *after* the system is useful on real
hardware, and it can ship as a patch release.

**Effort:** 2-3 focused weeks / 2-3 part-time months.

---

### M5: Virtual memory (v0.4 Protected)

**Status in 0.1.0:** the frame allocator, active 4-level page tables and the
demand-grown heap have landed. Demand paging, guard pages and a double-fault IST
remain.

**Goal:** the kernel owns real page tables, the heap can grow, and a bad access
faults instead of corrupting memory silently.

**Work items**

1. **Frame allocator** - bitmap or buddy over `Usable` regions of the
   bootloader map (a `Vec` of free frames, or a bitmap sized to physical RAM).
   *Done: bitmap sized to the highest usable frame.*
2. **Page tables** - 4-level walker: map/unmap/protect, physical-to-virtual
   helpers, `OffsetPageTable` over the bootloader's physical memory offset.
   *Done: `map_page`, `unmap_page`, `translate_addr`, boot self-test.*
3. **Kernel address space** - map the framebuffer write-combining, map the heap,
   guard pages around kernel stacks, and a guard page under each thread stack.
4. **Growable heap** - replace the fixed 8 MiB array with a heap that reserves
   frames on demand, keeping `linked_list_allocator` (or moving to a slab
   allocator for small kernel objects). *Done for the kernel heap.*
5. **Double fault IST** - a known-good stack for the double fault handler.

**Acceptance**

- A deliberate out-of-bounds write inside a test task triggers a page fault with
  a message, not a silent corruption.
- Heap usage can exceed 64 MiB in a stress test.
- Framebuffer mapping is write-combining (verify attribute bits via a debug dump).

**Effort:** 2-3 focused weeks / 2-3 part-time months.

---

### M6: User mode and processes (v0.5 Multitasking)

**Goal:** more than one program runs at once, isolated from the kernel and from
each other, and one crashing does not kill the system.

**Work items**

1. GDT with user code/data segments, TSS with IST entries.
2. `syscall`/`sysret` entry path (preferred over `int 0x80`).
3. Syscall table: `write`, `read`, `exit`, `spawn`, `wait`, `mmap`, `brk`,
   `open`/`read`/`write`/`close` (last four after M7), `getpid`, `yield`.
4. ELF64 loader for statically linked programs (no dynamic linker).
5. Per-process address space, kernel stack per process, `struct Process` table.
6. Round-robin scheduler driven by the PIT (or the local APIC timer later),
   plus blocking on keyboard input.
7. An `init` process that spawns the shell; the desktop either runs as a kernel
   task or becomes a user process with a framebuffer syscall - decide here.
8. A minimal userspace runtime: a tiny `libnoor` with `_start`, syscall stubs,
   and a `println!`-equivalent, plus a linker script.

**Acceptance**

- Two user programs run concurrently (one printing, one spinning), and `init`
  reaps them.
- Deliberately faulting one program leaves the other running and the kernel healthy.
- A user program cannot write to kernel memory (write to a kernel address in a
  user page table is unmapped and faults).

**Effort:** 4-6 focused weeks / 4-6 part-time months. This is the hardest
milestone; budget for it accordingly.

---

### M7: Storage (v0.6 Persistent)

**Goal:** files survive a reboot and can be read from another machine.

**Work items**

1. **Block device abstraction** - `read_sector`, `write_sector`, `flush`, sector
   size, capacity; one trait, several drivers.
2. **Drivers, in this order** (easiest to test first):
   - `virtio-blk` over PCI (QEMU `-device virtio-blk-pci`),
   - `AHCI` (SATA; most real machines and VirtualBox),
   - `NVMe` (modern laptops).
3. **Partition tables** - read GPT (header at LBA1, entry array, protective MBR)
   and MBR; identify the ESP by its type GUID.
4. **FAT32** - read and write: boot sector/BPB, FAT chains, directory entries
   (including long file names), cluster allocation, free-space search. FAT32 is
   chosen because macOS and Windows can mount it directly, which makes testing
   and the installer dramatically easier.
5. **VFS layer** - `Path`, `File`, `Directory`, mount points, current directory.
6. **Shell commands** - `ls`, `cd`, `cat`, `mkdir`, `touch`, `write`, `rm`.
7. **Write-back policy** - flush on close and on `sync`; document that a pull of
   the plug can lose the last writes.

**Acceptance**

- `write hello.txt` then reboot: `cat hello.txt` returns the content.
- The same image, mounted on macOS, shows `hello.txt` at the expected path.
- A deliberately corrupted FAT chain produces an error, not a crash.

**Effort:** 3-4 focused weeks / 3-4 part-time months.

---

### M8: Installer (v0.7 Installable)

**Goal:** the USB stick becomes an installer; the installed system boots with
the stick removed. This is the milestone that answers "like Ubuntu".

**Work items**

1. Boot the USB image as a **live system**: usable read-only, with a RAM
   filesystem for scratch files.
2. **Installer application** with a keyboard/mouse UI:
   - enumerate block devices and partitions, show sizes and current contents,
   - require the user to select a target disk and type its name to confirm,
   - refuse to touch a disk that holds the running system,
   - partition: GPT with a 100 MiB ESP and a FAT32 root partition,
   - format, copy kernel and userspace files, write `EFI/BOOT/BOOTX64.EFI`,
     write a boot entry, then flush and report success.
3. **Boot entry strategy.** `bootloader` 0.11 hands over after exiting UEFI boot
   services, so `SetVariable` is not directly available. Two options:
   - (a) rely on the firmware's removable-media fallback path
     `\EFI\BOOT\BOOTX64.EFI` on the ESP - works on essentially all UEFI
     firmware with no NVRAM write, and is the recommended v1.0 approach;
   - (b) move to a bootloader that exposes runtime services (Limine, or a custom
     UEFI application) if a real firmware boot menu entry is required.
4. **Legacy BIOS path** - install the BIOS stage to the target's boot sector and
   document that this is destructive to an existing MBR.
5. **Dual-boot safety** - never resize or delete existing partitions in v1.0;
   require a free disk or a free region, and say so plainly.

**Acceptance**

- In QEMU: attach a blank 2 GiB virtio disk plus the USB image, install, detach
  the USB image, boot from the disk alone - desktop appears.
- In VirtualBox: same, with EFI firmware.
- On a real PC: install to a spare disk, remove the stick, boot.

**Effort:** 1-2 focused weeks / 1-2 part-time months, on top of M7.

---

### M9: The everyday desktop (v0.8 Everyday)

**Goal:** the GUI is something a person can actually work in for ten minutes.

**Work items**

1. Window management: resize, minimise, maximise, snap to edges, alt-drag.
2. Damage-based compositor with an off-screen back buffer, so repaints are
   partial and flicker-free.
3. Widget set: buttons, checkboxes, lists with scrollbars, text fields with
   caret, selection and clipboard.
4. **Text editor** - open, edit, save on FAT32; this is the "prove it works"
   application.
5. **Settings that do things** - wallpaper choice, accent colour, and a persisted
   configuration file written through the VFS.
6. **File manager** (optional, if time allows) - browse, open, delete.
7. Replace the placeholder About text with real build information.

**Acceptance**

- Create a file in the editor, save it, reboot, open it, edit it, save again.
- Resizing a window redraws only the affected region (verify with a frame-time
  counter).
- Settings persist across reboot.

**Effort:** 3-4 focused weeks / 3-4 part-time months.

---

### M10: Networking (v0.9/1.x, optional)

**Goal:** the machine can answer a ping and fetch a page. Deliberately last.

**Work items:** `virtio-net` or `e1000` driver, ARP, IPv4, ICMP, UDP, DHCP
client, a tiny DNS resolver, then TCP (the hard part) and an HTTP/1.1 client.

**Effort:** 4-8 focused weeks / 6-12 part-time months. **Recommendation:** do
not start before M9 passes; it does not make the OS more "real", it makes it
bigger.

---

## 4. Cross-cutting workstreams

### 4.1 Real-hardware testing

Every milestone after M2 must be verified on the matrix in M3. Keep a
`docs/evidence/hardware-matrix.md` with machine, firmware version, result and
screenshot per run. A spare x86 PC or an Intel Mac is a project prerequisite.

### 4.2 Boot media

Two images, one documented meaning each. Never ship an image whose origin is
unclear: `build.rs` stamps the git describe (or a short hash) into the kernel
version string, so a screenshot always identifies its build.

### 4.3 Testing and CI

| Layer | What | Tool |
|---|---|---|
| Pure logic unit tests | partition parsing, FAT32 arithmetic, ELF header parsing, page-table index maths | `cargo test` on the host, with the logic factored into a `no_std`-friendly crate |
| Boot smoke test | headless QEMU boot, assert serial contains every `[ OK ]` line | `scripts/boot-check.py` |
| GUI integration | scripted keyboard/mouse injection + screenshots | `scripts/interact.py` |
| VM matrix | VirtualBox/Hyper-V boots | manual, documented |
| Real hardware | the matrix above | manual, documented |
| Static checks | `cargo fmt --check`, `cargo clippy -D warnings` | Makefile / CI |

Add a GitHub Actions workflow on the Linux runner: install QEMU, boot headless,
grep the serial log, fail the build if a boot step is missing. That turns "it
worked on my Mac" into a real gate.

### 4.4 Documentation discipline

Each milestone updates: `README.md` (capability table), `docs/architecture.md`
(new subsystem), `docs/university-report.md` (requirements + results), and
`docs/viva-questions.md` (at least two new questions). Documentation is part of
the definition of done, not an afterthought.

---

## 5. Architecture decisions to settle early

These are cheap now and expensive later.

| # | Decision | Options | Recommendation |
|---|---|---|---|
| 1 | Bootloader | keep `bootloader` 0.11; Limine; custom UEFI app | Keep 0.11 until M8; no NVRAM access is solved by the ESP fallback path |
| 2 | Firmware images | one hybrid; two separate | Two separate; document which is which |
| 3 | Paging layout | identity map; higher-half kernel | Higher-half kernel - it makes user mode and `syscall` clean |
| 4 | Syscall ABI | `int 0x80`; `syscall`/`sysret` | `syscall`/`sysret` (register-based, with a versioned syscall table) |
| 5 | Filesystem | FAT32; ext2; custom | FAT32 - host-mountable, which makes every test easier |
| 6 | Storage driver order | AHCI first; virtio first | virtio first (fastest to test), then AHCI, then NVMe |
| 7 | Allocator | keep linked-list; slab for objects | Keep linked-list for pages, add a small slab for kernel objects at M6 |
| 8 | Graphics | software; GPU | Software with a back buffer and damage rects; no GPU in v1.0 |
| 9 | Input | PS/2 only; add USB HID | PS/2 for v1.0, USB HID as a patch release (M4) |
| 10 | Target CPU | x86_64 only; add aarch64 | x86_64 only; an aarch64 port is a separate project |

---

## 6. Risk register

| Risk | Impact | Mitigation |
|---|---|---|
| xHCI/USB HID takes far longer than planned | months lost | ship PS/2-only v1.0; USB as a patch release |
| ACPI/AML parsing is fiddly | shutdown fails on some machines | try-each-`SLP_TYP` fallback; keep QEMU path |
| No test hardware available | "works on my VM" only | budget one spare x86 PC; document the matrix honestly |
| Demand paging + user mode proves harder than expected | M5/M6 slip | paging core is landed; keep kernel-only until guard pages are stable; user mode is a separate gate |
| Scope creep ("add networking, add a browser") | nothing ships | the anti-goals list below is a contract |
| Firmware variance (Secure Boot, GOP modes, legacy USB) | black screen on real hardware | documented requirements, serial logging, framebuffer fallback |
| Writes without guard pages corrupt memory | mysterious crashes | debug assertions, guard pages at M5, `try_lock` fault path |
| QEMU TCG performance | unusable desktop | damage rectangles (M1), back buffer (M9) |
| No unit tests for hardware logic | silent regressions | factor parsers into host-testable code (section 4.3) |

---

## 7. Effort model and schedule

| Milestone | Focused (30-40 h/wk) | Part-time (10 h/wk) |
|---|---|---|
| M1 pointer input | 3-5 days | 1-2 weeks |
| M2 boot media / VMs | 2-3 days | 1 week |
| M3 real hardware | 1 week | 1 month |
| M4 USB HID (optional) | 2-3 weeks | 2-3 months |
| M5 virtual memory | 2-3 weeks | 2-3 months |
| M6 user mode | 4-6 weeks | 4-6 months |
| M7 storage + FAT32 | 3-4 weeks | 3-4 months |
| M8 installer | 1-2 weeks | 1-2 months |
| M9 everyday desktop | 3-4 weeks | 3-4 months |
| **Total to v1.0 (M1-M9)** | **~5-8 months** | **~18-26 months** |
| M10 networking (optional) | +4-8 weeks | +6-12 months |

These are engineering estimates for one developer, not promises. The two
milestones most likely to overrun are M6 and M4.

---

## 8. Anti-goals (scope discipline)

Explicitly **out of scope** for v1.0. Writing these down is what makes the
project finishable.

- running Linux or ELF binaries built for other systems,
- a package manager, a compiler on the device, or a scripting language,
- a web browser or an HTML/JS engine,
- GPU acceleration, 3D, video decoding, hardware cursors,
- audio,
- networking, Wi-Fi, Bluetooth,
- multiprocessor support (SMP) and CPU frequency/power management,
- suspend/resume and hot-plug,
- filesystem journaling, encryption, permissions beyond a single user,
- Secure Boot signing (documented as "disable Secure Boot" instead),
- Windows/macOS dual-boot management or partition resizing,
- aarch64 or any second architecture.

If a feature is not needed to demonstrate "boots on metal, installs, keeps
files, runs isolated programs, has a usable desktop", it waits.

---

## 9. Immediate next three changes

If work resumes tomorrow, this is the order:

1. **M2** - `make usb` with confirmations, `make vdi`, checksums, and the
   firmware requirements in `README.md`. Turns the project into something a
   person can actually carry to a PC.
2. **M3 first item** - ACPI FADT-based shutdown and reset, replacing the
   `0x604` guess, so `shutdown` works on real hardware and not only under QEMU.
3. **M1 follow-up** - general damage rectangles for window drag/open/close, so a
   full frame is no longer needed when a window moves.

After those three, the next gate is M5 (paging), because everything from M6
onward depends on it.

---

## Appendix A - Commands that will exist after M2

```bash
# Boot media
make usb DEVICE=/dev/disk4      # macOS, prompts for confirmation
make usb DEVICE=/dev/sdX        # Linux

# Virtual machines
make vdi                        # nooros.vdi for VirtualBox (EFI) or Hyper-V
VBoxManage modifyvm NoorOS --firmware efi

# Testing
make boots                      # headless QEMU boot, asserts every [ OK ] line
make demo                       # scripted GUI walkthrough + screenshots
make check                      # clippy -D warnings
```

QEMU device flags that the later milestones will need:

```bash
# virtio storage (M7)
-drive file=disk.img,if=none,id=d0 -device virtio-blk-pci,drive=d0
# AHCI / SATA (M7)
-drive file=disk.img,if=none,id=d0 -device ahci,id=ahci \
  -device ide-hd,drive=d0,bus=ahci.0
# NVMe (M7)
-drive file=disk.img,if=none,id=d0 -device nvme,drive=d0,serial=nooros
# USB input via xHCI (M4)
-device qemu-xhci -device usb-kbd -device usb-mouse
```

---

## Appendix B - Target source layout

```text
kernel/src/
├── main.rs              entry point
├── kernel/              init order, panic, fatal screen
├── boot/                boot screen
├── arch/                x86_64 specifics: GDT, TSS, syscall entry, page tables
├── memory/              frame allocator, paging, heap, user copy helpers
├── interrupts/          IDT, PIC/APIC, handlers
├── drivers/
│   ├── framebuffer.rs
│   ├── serial.rs
│   ├── keyboard.rs      PS/2 + shared event queue
│   ├── mouse.rs         M1
│   ├── usb/             M4: xHCI, HID
│   ├── block/           M7: virtio-blk, AHCI, NVMe
│   ├── timer.rs, rtc.rs, cpu.rs, power.rs
├── fs/                  M7: FAT32, VFS
├── process/             M6: loader, scheduler, process table
├── syscall/             M6: table and handlers
├── graphics/            canvas, primitives, text, branding
├── desktop/             window manager, compositor, taskbar, launcher
├── applications/        terminal, monitor, settings, about, editor, installer
└── shell/               parser and commands
user/                    M6: libnoor, init, shell, editor (user programs)
```

---

## Appendix C - Reading

1. *Writing an OS in Rust* (Philipp Oppermann) - paging, allocators, async.
2. OSDev Wiki - AHCI, NVMe, xHCI, PS/2 mouse, ACPI, GPT, FAT.
3. Intel SDM volumes 1-3 - segmentation, paging, interrupts, `syscall`.
4. ACPI Specification 6.x - FADT, `\_S5`, reset register.
5. UEFI Specification - GOP, boot services, removable-media boot path
   `\EFI\BOOT\BOOTX64.EFI`.
6. Microsoft FAT specification - BPB, directory entries, LFN.
7. `bootloader`, `x86_64`, `pic8259`, `linked_list_allocator`, `pc-keyboard`
   crate documentation.
