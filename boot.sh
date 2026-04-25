#!/bin/bash
UEFI_FIRMWARE="/opt/homebrew/share/qemu/edk2-x86_64-code.fd"
export PATH="/opt/homebrew/bin:$PATH"

# Build everything
cargo build -p rb-loader --target x86_64-unknown-uefi
cargo build -p ross-kernel --target x86_64-unknown-none
RUSTFLAGS="-C relocation-model=static -C link-arg=-Tross-user/linker.ld" \
    cargo build -p ross-init --target x86_64-unknown-none

# Prep disk
mkdir -p build/EFI/BOOT
cp target/x86_64-unknown-uefi/debug/rb-loader.efi build/EFI/BOOT/BOOTX64.EFI

# Convert Kernel to flat binary (strips ELF headers)
rust-objcopy -O binary target/x86_64-unknown-none/debug/ross-kernel build/kernel.elf

# Create RAMDisk (Initrd)
mkdir -p build/initrd
cp assets/hello.txt build/initrd/
echo "Welcome to ROSS OS Phase 6!" > build/initrd/motd.txt
cp target/x86_64-unknown-none/debug/ross-init build/initrd/init.elf
tar -cf build/initrd.tar -C build/initrd .
rm -rf build/initrd

# Prepare persistent AHCI disk (FAT32).
# FAT32 requires >= ~32 MB; we use 64 MB to be safe.
# Recreate if missing or if the image is blank (all-zero first sector = no BPB).
DISK_NEEDS_FORMAT=0
if [ ! -f build/disk.img ]; then
    DISK_NEEDS_FORMAT=1
elif [ "$(dd if=build/disk.img bs=1 count=3 2>/dev/null | xxd -p)" = "000000" ]; then
    echo "build/disk.img has no BPB — recreating as 64 MB FAT32"
    DISK_NEEDS_FORMAT=1
fi

if [ "$DISK_NEEDS_FORMAT" = "1" ]; then
    echo "Creating 64 MB FAT32 disk image at build/disk.img"
    dd if=/dev/zero of=build/disk.img bs=1m count=64 2>/dev/null
    if command -v mkfs.fat >/dev/null 2>&1; then
        mkfs.fat -F 32 -n ROSSDISK build/disk.img
    elif command -v mkfs.vfat >/dev/null 2>&1; then
        mkfs.vfat -F 32 -n ROSSDISK build/disk.img
    elif command -v hdiutil >/dev/null 2>&1 && command -v newfs_msdos >/dev/null 2>&1; then
        # macOS: newfs_msdos needs a block device, attach via hdiutil first
        DEV=$(hdiutil attach -imagekey diskimage-class=CRawDiskImage -nomount build/disk.img 2>/dev/null | head -1 | awk '{print $1}')
        newfs_msdos -F 32 -v ROSSDISK "$DEV"
        hdiutil detach "$DEV" 2>/dev/null
    elif command -v newfs_msdos >/dev/null 2>&1; then
        newfs_msdos -F 32 -v ROSSDISK build/disk.img
    else
        echo "WARNING: No FAT32 formatter found — disk will be blank (mount will fail)"
    fi
fi

# Run
qemu-system-x86_64 \
    -drive if=pflash,format=raw,readonly=on,file="$UEFI_FIRMWARE" \
    -drive format=raw,file=fat:rw:build \
    -drive id=rossdisk,format=raw,if=none,file=build/disk.img \
    -device ahci,id=ahci0 \
    -device ide-hd,drive=rossdisk,bus=ahci0.0 \
    -m 512M -vga std -net none -serial stdio \
    -display cocoa,zoom-to-fit=off

