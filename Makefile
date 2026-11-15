# NoorOS developer entry points.
#
# The rustup toolchain must win over Homebrew's rustc/cargo, so the cargo
# directory is prepended to PATH for every recipe.

SHELL := /bin/bash
export PATH := $(HOME)/.cargo/bin:$(PATH)
CARGO ?= cargo
PYTHON ?= python3

.DEFAULT_GOAL := help

.PHONY: help build run run-uefi headless demo screenshots check fmt clean

help:
	@echo "NoorOS 0.1.0 - A Light Beyond Computing."
	@echo
	@echo "  make build        Build kernel, BIOS/UEFI images and host launcher"
	@echo "  make run          Boot the BIOS image in a QEMU window"
	@echo "  make run-uefi     Boot the UEFI image (downloads OVMF once)"
	@echo "  make headless     Boot without a window, serial log to serial.log"
	@echo "  make demo         Drive the full GUI and refresh screenshots/demo"
	@echo "  make screenshots  Capture boot frames into screenshots/boot"
	@echo "  make check        Run cargo clippy --release"
	@echo "  make fmt          Format the workspace"
	@echo "  make clean        Remove target/"

build:
	$(CARGO) build --release

run: build
	$(CARGO) run --release

run-uefi: build
	$(CARGO) run --release -- uefi

headless: build
	$(CARGO) run --release -- bios --headless --serial serial.log

demo: build
	$(PYTHON) scripts/interact.py screenshots/demo
	@for f in screenshots/demo/*.ppm; do sips -s format png "$$f" --out "$${f%.ppm}.png" >/dev/null; done
	@rm -f screenshots/demo/*.ppm
	@echo "screenshots refreshed in screenshots/demo"

screenshots: build
	$(PYTHON) scripts/capture.py screenshots/boot --delay 2.5
	@for f in screenshots/boot/*.ppm; do sips -s format png "$$f" --out "$${f%.ppm}.png" >/dev/null; done
	@rm -f screenshots/boot/*.ppm

check:
	$(CARGO) clippy --release

fmt:
	$(CARGO) fmt --all

clean:
	$(CARGO) clean
