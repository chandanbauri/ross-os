#!/bin/bash
UEFI_FIRMWARE="/opt/homebrew/share/qemu/edk2-x86_64-code.fd"
export PATH="/opt/homebrew/bin:$PATH"

# Build everything
cargo build -p rb-loader --target x86_64-unknown-uefi
cargo build -p ross-kernel --target x86_64-unknown-none

# Prep disk
mkdir -p build/EFI/BOOT
cp target/x86_64-unknown-uefi/debug/rb-loader.efi build/EFI/BOOT/BOOTX64.EFI

# Convert Kernel to flat binary (strips ELF headers)
rust-objcopy -O binary target/x86_64-unknown-none/debug/ross-kernel build/kernel.elf

# Run
qemu-system-x86_64 \
    -drive if=pflash,format=raw,readonly=on,file="$UEFI_FIRMWARE" \
    -drive format=raw,file=fat:rw:build \
    -m 512M -vga std -net none -serial stdio -no-reboot \
    -display cocoa,zoom-to-fit=off
