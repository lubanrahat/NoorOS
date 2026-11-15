# Building and running NoorOS on macOS (Apple Silicon)

This guide lists the exact commands used to build and run NoorOS 0.1.0 on an
Apple Silicon Mac (`arm64`). The guest is x86_64, so QEMU emulates the CPU with
its TCG engine; hardware virtualisation (HVF) is not available for an x86_64
guest on an arm64 host.

Verified with:

| Tool | Version |
|---|---|
| Host | macOS on Apple Silicon (`arm64`) |
| Rust | `rustc 1.102.0-nightly` |
| Cargo | `cargo 1.102.0-nightly` |
| QEMU | `QEMU emulator version 11.1.3` |
| Python (screenshots) | 3.x from the Xcode/macOS toolchain |

---

## 1. Install the host tools

```bash
# Apple command line tools (linker, make, git, python3)
xcode-select --install

# QEMU (x86_64 system emulator)
brew install qemu

# Rustup, not Homebrew's rust package
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

The installer asks to modify your shell profile. If you decline, export the
path in every new terminal:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

### Why the PATH line matters

Homebrew also provides `rustc` and `cargo`. If `/opt/homebrew/bin` comes before
`$HOME/.cargo/bin`, `cargo` is the Homebrew stable toolchain. NoorOS needs
nightly (for `#![feature(abi_x86_interrupt)]` and Cargo artifact dependencies),
so the build fails with errors such as:

```
error[E0554]: `#![feature]` may not be used on the stable release channel
error: failed to parse manifest ... artifact dependencies are unstable
```

Check which toolchain you are using:

```bash
which cargo          # must print /Users/<you>/.cargo/bin/cargo
cargo --version      # must say nightly
```

The repository's `rust-toolchain.toml` pins the nightly channel and asks for the
components and targets below. Running any `cargo` command inside the repository
installs them on first use. To install them explicitly:

```bash
rustup toolchain install nightly \
  --component rust-src --component llvm-tools-preview \
  --component rustfmt --component clippy \
  --target x86_64-unknown-none \
  --target x86_64-unknown-uefi
```

`x86_64-unknown-none` is the guest target on which the kernel is compiled.
`x86_64-unknown-uefi` is listed for completeness; it is not needed to build the
current kernel.

Verify QEMU:

```bash
qemu-system-x86_64 --version
```

## 2. Build

```bash
cd NoorOS
export PATH="$HOME/.cargo/bin:$PATH"

cargo build --release
```

`build.rs` compiles the kernel through a Cargo artifact dependency, then asks
the `bootloader` crate to create two disk images. The final artifacts are:

```
target/release/build/nooros/<hash>/out/nooros-bios.img
target/release/build/nooros/<hash>/out/nooros-uefi.img
target/release/nooros                    # host launcher
```

The hash directory is normal Cargo behaviour. Use the launcher or the scripts in
`scripts/` rather than typing the path by hand; both pick the newest image.

A clean build takes a few minutes on an M-series Mac. Incremental builds take a
few seconds.

## 3. Run

### BIOS boot (default)

```bash
cargo run --release
```

A QEMU window opens, shows the NoorOS boot screen with eight real `[ OK ]`
steps, then Noor Desktop. The terminal receives the kernel's serial log.

### UEFI boot

```bash
cargo run --release -- uefi
```

On first use the launcher downloads a pinned OVMF firmware build into
`target/ovmf/` (about 35 MiB) with `curl` and `tar`. Subsequent UEFI boots reuse
it. The same NoorOS image and desktop appear; the hand-off firmware differs.

### Headless and scripted use

```bash
# No window, serial log to a file
cargo run --release -- bios --headless --serial /tmp/noor-serial.log

# Expose QEMU's monitor on a UNIX socket (used by scripts/capture.py)
cargo run --release -- bios --headless \
  --serial /tmp/noor-serial.log \
  --monitor /tmp/noor-monitor.sock
```

Extra QEMU arguments can be passed with `--qemu-arg`:

```bash
cargo run --release -- bios --qemu-arg -d --qemu-arg int
```

### Shortcuts

`make run` is equivalent to `cargo run --release` and sets the rustup PATH.
`make run-uefi`, `make headless`, `make demo`, and `make clean` are provided in
the `Makefile`.

## 4. Capturing evidence

The scripts drive QEMU through its monitor socket, so they work over SSH and do
not need a visible window.

```bash
# One boot screenshot at 2.5 s
python3 scripts/capture.py /tmp/noor-shots --delay 2.5

# Full GUI sequence: terminal commands, all apps, launcher
python3 scripts/interact.py screenshots/demo
```

QEMU writes `.ppm` frames. macOS `sips` converts them to PNG:

```bash
cd screenshots/demo
for f in *.ppm; do sips -s format png "$f" --out "${f%.ppm}.png"; done
```

The demo script waits 2.5 s after every action because a full desktop repaint
can take hundreds of milliseconds under TCG. Shortening those waits produces
torn or half-drawn screenshots.

## 5. Troubleshooting

### `error[E0554]: #![feature] may not be used on the stable release channel`

Homebrew's `cargo` is first in `PATH`. Run:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
which cargo
```

### `artifact dependencies are unstable`

The `.cargo/config.toml` in this repository enables `bindeps`, but the feature
is nightly-only. Confirm `cargo --version` reports `nightly`.

### The monitor socket never appears

Check that QEMU is installed for the same architecture:

```bash
which qemu-system-x86_64
```

The launcher deliberately uses `qemu-system-x86_64`, not `qemu-system-aarch64`.

### The kernel does not start after a source change

Older builds of `build.rs` watched only `kernel/src` as a directory; editing an
existing file did not trigger a rebuild. The current script registers every
file, and `scripts/capture.py` chooses the newest image. If you still see stale
behaviour:

```bash
cargo clean
cargo build --release
```

### The QEMU window is blank

- Do not pass `--headless` (it disables the window).
- Give the boot screen a moment: firmware, bootloader and the first frame take
  a few seconds under TCG.
- Look at the serial log; if it stops mid-init, the kernel hit an error and the
  fatal screen should also be visible in the window.

### QEMU is slow

This is expected. The x86_64 guest is fully emulated by TCG on arm64. The UI is
event-driven and caches the wallpaper, but a repaint still costs hundreds of
milliseconds. It is not a hardware-accelerated desktop.

### How to power off

From Noor Terminal:

```text
shutdown
```

The kernel uses ACPI (`PM1a_CNT` at `0x604`) and falls back to QEMU's
`isa-debug-exit` device. If the guest is stuck, close the QEMU window or kill the
process:

```bash
pkill -f qemu-system-x86_64
```

### Reboot exits QEMU instead of restarting

The launcher passes `-no-reboot` by default, so the emulator exits when the guest
resets. That is intentional for reproducible demo runs. To watch a real guest
reset:

```bash
cargo run --release -- bios --allow-reboot
```

then run `reboot` in Noor Terminal. QEMU returns to the boot screen and a fresh
desktop with uptime near one second.

## 6. Uninstall / clean up

```bash
cargo clean                 # removes target/, including downloaded OVMF
rm -rf target/ovmf          # just the UEFI firmware
```

No system-wide state is modified. The project does not write outside its own
directory except temporary files you ask the scripts to create.
