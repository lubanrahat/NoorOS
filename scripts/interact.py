#!/usr/bin/env python3
"""Drive NoorOS with the QEMU monitor and capture screenshots.

Usage: scripts/interact.py OUT_DIR [--profile release]

The sequence exercises both input paths end to end: the keyboard (desktop ->
Noor Terminal -> shell commands -> Noor Monitor -> Noor Launcher) and then the
mouse (click a launcher row, drag a title bar, close a window with [X], raise a
window by clicking its title bar, launch an app from the taskbar).

QEMU clamps a single PS/2 relative event, so long pointer jumps are split into
small `mouse_move` steps and the script tracks the pointer itself.
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
MOUSE_STEP = 100
# A full repaint (launcher, several windows) can take over a second under TCG.
SETTLE = 2.5


class Monitor:
    def __init__(self, path, screen=(1280, 720)):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(path)
        self.sock.settimeout(0.5)
        self._drain()
        # The kernel centres the pointer at boot; keep the same model here.
        self.x, self.y = screen[0] // 2, screen[1] // 2

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

    def move_by(self, dx, dy):
        """Relative pointer motion, split to respect the per-packet limit."""
        steps = max(1, (max(abs(dx), abs(dy)) + MOUSE_STEP - 1) // MOUSE_STEP)
        done_x = done_y = 0
        for i in range(1, steps + 1):
            next_x = dx * i // steps
            next_y = dy * i // steps
            self.command(f"mouse_move {next_x - done_x} {next_y - done_y}")
            done_x, done_y = next_x, next_y
            time.sleep(0.18)
        self.x += dx
        self.y += dy
        time.sleep(0.25)

    def move_to(self, x, y):
        self.move_by(x - self.x, y - self.y)

    def click(self):
        self.command("mouse_button 1")
        time.sleep(0.15)
        self.command("mouse_button 0")
        time.sleep(0.15)

    def click_at(self, x, y):
        self.move_to(x, y)
        time.sleep(0.25)
        self.click()
        time.sleep(SETTLE)

    def drag(self, start, end):
        self.move_to(*start)
        time.sleep(0.25)
        self.command("mouse_button 1")
        time.sleep(0.2)
        self.move_by(end[0] - start[0], end[1] - start[1])
        self.command("mouse_button 0")
        time.sleep(SETTLE)


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
        # Park the pointer in empty desktop space for the hero shot.
        mon.move_by(300, 200)
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

        # --- Mouse -------------------------------------------------------
        # Launcher is open with the keyboard selection on Noor Terminal.
        mon.move_to(640, 260)
        mon.shot(os.path.join(out, "13-mouse-launcher.ppm"))
        mon.click_at(640, 260)  # click the Noor Terminal row

        # Drag the focused terminal by its title bar.
        mon.drag((500, 120), (600, 200))
        mon.shot(os.path.join(out, "14-mouse-drag.ppm"))

        # The window moved with the pointer: its close button is now at
        # (290 + 900 - 33) + 10, (187 + 7) + 10.
        mon.click_at(1167, 204)
        mon.shot(os.path.join(out, "15-mouse-close.ppm"))

        # Raise Noor Monitor by clicking its (mostly hidden) title bar.
        mon.click_at(250, 90)
        mon.shot(os.path.join(out, "16-mouse-zorder.ppm"))

        # Launch Noor Terminal from the taskbar icon.
        mon.click_at(435, 675)
        mon.shot(os.path.join(out, "17-mouse-taskbar.ppm"))

        mon.command("quit")
    finally:
        proc.wait(timeout=5)
    print("serial log:", serial_path)


if __name__ == "__main__":
    main()
