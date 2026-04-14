# ROSS Architecture

## Boot Sequence

1. **UEFI Firmware**: Loads `EFI/BOOT/BOOTX64.EFI` from the FAT partition.
2. **rb-loader**:
    - Initializes UEFI services.
    - Locates the Graphics Output Protocol (GOP).
    - Clears the screen to the maroon background.
    - Loads `kernel.elf` from the root of the volume into memory at `0x200000`.
    - Hands off control to the kernel with a `BootInfo` structure.
3. **ross-kernel**:
    - Receives `BootInfo` via the `sysv64` calling convention.
    - Renders the "Starting..." splash screen.
    - Enters an infinite spin loop (current state).

## Memory Map

- `0x200000`: Kernel entry point (linked and loaded here).
- Framebuffer: Passed via `BootInfo`, resolution detected at runtime.

## Graphics

ROSS uses a simple 8x8 bitmap font scaled up for readability. The framebuffer uses a 32-bit BGRx format.
