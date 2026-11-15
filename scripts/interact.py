#!/usr/bin/env python3
"""Drive NoorOS with QEMU monitor `sendkey` and capture screenshots.

Usage: scripts/interact.py OUT_DIR [--profile release]

The sequence exercises the keyboard path end to end: desktop -> Noor Terminal
-> shell commands -> Noor Monitor -> Noor Launcher. No mouse is required.
"""

import argparse
import os
import socket
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from capture import find_images  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
KEY_DELAY = 0.12


class Monitor:
    def __init__(self, path):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(path)
        self.sock.settimeout(0.5)
        self._drain()

    def _drain(self):
        data = b""
        try:
            while True:
                chunk = self.sock.recv(65536)
                if not chunk:
                    break
                data += chunk
        except socket.timeout:
            pass
        return data

    def command(self, text):
        self.sock.sendall((text + "\n").encode())
        time.sleep(0.15)
        self._drain()

    def key(self, key):
        self.command(f"sendkey {key}")
        time.sleep(KEY_DELAY)

    def type(self, text):
        for ch in text:
            if ch == " ":
                key = "spc"
            elif ch == "-":
                key = "minus"
            elif ch.isupper():
                key = f"shift-{ch.lower()}"
            else:
                key = ch
            self.key(key)

    def shot(self, path):
        self.command(f"screendump {path}")
        time.sleep(0.3)
        print("captured", path)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("out_dir")
    parser.add_argument("--profile", choices=["debug", "release"], default="release")
    args = parser.parse_args()

    out = args.out_dir
    os.makedirs(out, exist_ok=True)
    bios, _ = find_images(args.profile)
    monitor_path = os.path.join(out, "monitor.sock")
    serial_path = os.path.join(out, "serial.log")
    for path in (monitor_path, serial_path):
        if os.path.exists(path):
            os.remove(path)

    cmd = [
        "qemu-system-x86_64", "-machine", "pc", "-m", "512M", "-smp", "1",
        "-vga", "std", "-no-reboot", "-display", "none",
        "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
        "-serial", f"file:{serial_path}",
        "-monitor", f"unix:{monitor_path},server,nowait",
        "-drive", f"format=raw,file={bios}",
    ]
    proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    deadline = time.monotonic() + 15
    while not os.path.exists(monitor_path):
        if time.monotonic() > deadline or proc.poll() is not None:
            proc.kill()
            raise SystemExit("QEMU did not start")
        time.sleep(0.05)
    mon = Monitor(monitor_path)

    try:
        time.sleep(6.0)  # boot screen + desktop hand-off
        mon.shot(os.path.join(out, "01-desktop.ppm"))

        mon.key("f2")  # Noor Terminal
        time.sleep(2.5)
        mon.shot(os.path.join(out, "02-terminal.ppm"))

        mon.type("help")
        mon.key("ret")
        time.sleep(2.5)
        mon.shot(os.path.join(out, "03-help.ppm"))

        mon.type("info")
        mon.key("ret")
        time.sleep(2.5)
        mon.shot(os.path.join(out, "04-info.ppm"))

        mon.type("memory")
        mon.key("ret")
        time.sleep(2.5)
        mon.shot(os.path.join(out, "05-memory.ppm"))

        mon.type("version")
        mon.key("ret")
        time.sleep(1.5)
        mon.type("uptime")
        mon.key("ret")
        time.sleep(2.5)
        mon.shot(os.path.join(out, "06-version-uptime.ppm"))

        mon.type("nonsense")
        mon.key("ret")
        time.sleep(2.5)
        mon.shot(os.path.join(out, "07-unknown.ppm"))

        mon.key("f3")  # Noor Monitor
        time.sleep(2.5)
        mon.shot(os.path.join(out, "08-monitor.ppm"))

        mon.key("f4")  # Noor Settings
        time.sleep(2.5)
        mon.shot(os.path.join(out, "09-settings.ppm"))

        mon.key("f5")  # Noor About
        time.sleep(2.5)
        mon.shot(os.path.join(out, "10-about.ppm"))

        mon.key("alt-tab")
        time.sleep(2.5)
        mon.shot(os.path.join(out, "11-alt-tab.ppm"))

        mon.key("f1")  # Noor Launcher
        time.sleep(2.5)
        mon.shot(os.path.join(out, "12-launcher.ppm"))

        mon.command("quit")
    finally:
        proc.wait(timeout=5)
    print("serial log:", serial_path)


if __name__ == "__main__":
    main()
