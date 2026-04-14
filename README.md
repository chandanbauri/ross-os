# ROSS (Rapid Operating System Shell)

ROSS is a minimal, bare-metal operating system written in Rust. It utilizes UEFI for booting and provides a clean, professional loading environment.

## Features

- **UEFI Bootloader**: Custom-built `rb-loader` for modern hardware compatibility.
- **Rust-powered Kernel**: Memory-safe and highly efficient core logic.
- **Maroon Visual Identity**: Professional, high-contrast boot environment.
- **Bare-metal Graphics**: Direct framebuffer access for custom splash screens.

## Project Structure

- `rb-loader/`: The UEFI bootloader implementation.
- `ross-kernel/`: The core operating system kernel.
- `ross-common/`: Shared data structures and utilities (e.g., fonts, BootInfo).
- `boot.sh`: Automation script for building and launching in QEMU.

## Prerequisites

- [Rust](https://rustup.rs/) (nightly toolchain recommended)
- [QEMU](https://www.qemu.org/)
- `rust-objcopy` (available via `llvm-tools-preview`)

## Getting Started

To build and run ROSS in QEMU, simply execute the boot script:

```bash
./boot.sh
```

## License

This project is licensed under the MIT License.
