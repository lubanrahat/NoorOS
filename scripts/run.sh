#!/usr/bin/env bash
# Boot NoorOS through the host launcher.
#
# Usage: scripts/run.sh [bios|uefi] [--headless] [--serial PATH] [--monitor PATH]
#
# Keeps the rustup toolchain ahead of Homebrew's rustc/cargo on macOS.

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
export PATH="$HOME/.cargo/bin:$PATH"

if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo not found; install rustup from https://rustup.rs" >&2
    exit 1
fi

if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
    echo "error: qemu-system-x86_64 not found; run 'brew install qemu'" >&2
    exit 1
fi

exec cargo run --release -- "$@"
