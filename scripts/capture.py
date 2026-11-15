#!/usr/bin/env python3
"""Boot NoorOS in QEMU, take one or more framebuffer screenshots, then quit.

Usage: scripts/capture.py OUT_DIR [--delay SECONDS ...] [--uefi]

The screenshots are written as PPM files. QEMU's monitor socket is used for
`screendump`, so no graphical window is required (works over SSH and CI).
"""

import argparse
import os
import socket
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def find_images(profile=None):
    # Cargo puts the images under target/*/build/nooros-*/out/. Several stale
    # build-script output directories can exist, so take the newest image.
    found = []
    for current in ([profile] if profile else ["release", "debug"]):
        base = os.path.join(ROOT, "target", profile, "build")
        if not os.path.isdir(base):
            continue
        for name in os.listdir(base):
            if not name.startswith("nooros"):
                continue
            out = os.path.join(base, name)
            for sub in os.listdir(out):
                img = os.path.join(out, sub, "out")
                bios = os.path.join(img, "nooros-bios.img")
                uefi = os.path.join(img, "nooros-uefi.img")
                if os.path.exists(bios) and os.path.exists(uefi):
                    found.append((os.path.getmtime(bios), bios, uefi))
    if found:
        _, bios, uefi = max(found)
        return bios, uefi
    raise SystemExit("disk images not found - run `cargo build` first")


def monitor_command(sock, command, expect_reply=True):
    sock.sendall((command + "\n").encode())
    if not expect_reply:
        return ""
    data = b""
    sock.settimeout(2.0)
    try:
        while True:
            chunk = sock.recv(4096)
            if not chunk:
                break
            data += chunk
            if b"(qemu)" in data:
                break
    except socket.timeout:
        pass
    return data.decode(errors="replace")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("out_dir")
    parser.add_argument("--delay", type=float, action="append", default=None,
                        help="seconds after launch to capture (repeatable)")
    parser.add_argument("--uefi", action="store_true")
    parser.add_argument("--profile", choices=["debug", "release"], default="release")
    parser.add_argument("--serial", default=None)
    args = parser.parse_args()

    os.makedirs(args.out_dir, exist_ok=True)
    bios, uefi = find_images(args.profile)
    image = uefi if args.uefi else bios

    monitor_path = os.path.join(args.out_dir, "monitor.sock")
    serial_path = args.serial or os.path.join(args.out_dir, "serial.log")
    for path in (monitor_path, serial_path):
        if os.path.exists(path):
            os.remove(path)

    cmd = [
        "qemu-system-x86_64",
        "-machine", "pc",
        "-m", "512M",
        "-smp", "1",
        "-vga", "std",
        "-no-reboot",
        "-display", "none",
        "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
        "-serial", f"file:{serial_path}",
        "-monitor", f"unix:{monitor_path},server,nowait",
        "-drive", f"format=raw,file={image}",
    ]
    if args.uefi:
        # UEFI support needs OVMF; use the launcher's downloader instead.
        raise SystemExit("use `cargo run -- uefi` for UEFI boots; capture.py is BIOS-only")

    delays = sorted(args.delay or [1.0, 3.0, 6.0, 10.0])
    proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    start = time.monotonic()

    # Wait for the monitor socket.
    deadline = start + 10
    while not os.path.exists(monitor_path):
        if time.monotonic() > deadline:
            proc.kill()
            raise SystemExit("QEMU monitor socket did not appear")
        if proc.poll() is not None:
            raise SystemExit("QEMU exited before the monitor appeared")
        time.sleep(0.05)

    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    sock.connect(monitor_path)
    time.sleep(0.1)
    monitor_command(sock, "")

    try:
        for index, delay in enumerate(delays):
            wait = start + delay - time.monotonic()
            if wait > 0:
                time.sleep(wait)
            path = os.path.join(args.out_dir, f"screen-{index}-{delay:g}s.ppm")
            monitor_command(sock, f"screendump {path}", expect_reply=False)
            time.sleep(0.35)
            if os.path.exists(path):
                print(f"captured {path}")
            else:
                print(f"warning: {path} was not written", file=sys.stderr)
        monitor_command(sock, "quit", expect_reply=False)
    finally:
        try:
            sock.close()
        except OSError:
            pass
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
    print("serial log:", serial_path)


if __name__ == "__main__":
    main()
