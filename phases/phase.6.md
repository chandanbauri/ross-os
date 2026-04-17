# ROSS Roadmap: Phase 6 (Storage, Userspace & Shell)

Phase 6 focuses on transitioning ROSS from a kernel-only environment to a user-facing system by introducing persistent assets, executable loading, and a terminal interface. 

## Implementation Strategy: Safe Integration
To prevent breaking the existing `rb-loader` to `ross-kernel` handoff, the Initrd (Initial RAM Disk) must be treated as a secondary payload. 

1. **The Bridge (`ross-common`)**: Append `ramdisk_addr: *const u8` and `ramdisk_size: usize` to the end of the `BootInfo` struct. Appending ensures the existing `sysv64` ABI layout remains backward-compatible.
2. **Dual-Loading (`rb-loader`)**: Update `handoff.rs` to load `initrd.tar` using `AllocateType::AnyPages`. If the file is missing, log a warning but proceed to boot the kernel to avoid hard-bricking the boot sequence.
3. **Zero-Allocation Parsing (`ross-kernel`)**: Use a basic `.tar` format for the RAMDisk. This allows the kernel to sequentially parse 512-byte headers directly from physical memory without requiring a complex heap-based Virtual File System right away.
4. **Build Automation (`boot.sh`)**: Update the script to package a dummy `initrd` directory into `initrd.tar` and place it in the `build/` root alongside `kernel.elf`.

---

## 1. Initial RAM Disk (Initrd)
- [x] Update `ross_common::BootInfo` to include `ramdisk_addr` and `ramdisk_size`.
- [x] Update `boot.sh` to compile a `build/initrd.tar` archive from a local directory.
- [x] Modify `rb-loader/src/handoff.rs` to find and allocate pages for `initrd.tar` from the FAT partition.
- [x] **Milestone:** Kernel logs the RAMDisk memory address and size upon successful handoff.

## 2. Virtual File System (VFS) & TarFS
- [x] Implement a lightweight TarFS parser in the kernel to iterate over 512-byte blocks.
- [x] Abstract file operations into a basic `read_file(name: &str) -> Option<&[u8]>` function.
- [x] **Milestone:** The kernel can successfully print the contents of a text file (e.g., `motd.txt`) bundled inside the RAMDisk to the framebuffer.

## 3. ELF Process Loading
- [x] Implement an ELF parser to validate headers and read Program Headers.
- [x] Create a `spawn_process` function that sets up a new Page Table for an ELF binary, mapping its segments into an isolated address space.
- [x] **Milestone:** Successfully load and execute a compiled "User-land" `hello_world.elf` binary from the RAMDisk.

## 4. Minimal Shell (ROSS-SH)
- [x] Implement a basic line-buffer to capture PS/2 keyboard input.
- [x] Create a command parser mapping strings to kernel functions (e.g., `help`, `version`, `clear`, `reboot`).
- [x] **Milestone:** The user can type `reboot` and press Enter to restart the QEMU environment.