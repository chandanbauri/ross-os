# ROSS Roadmap: Phase 6 (Storage & Shell)

Phase 6 focuses on the user-land experience and persistent (or semi-persistent) storage.

## 1. Initial RAM Disk (Initrd)
- [ ] Update `rb-loader` to find and load `initrd.tar` from the FAT partition.
- [ ] Add `ramdisk_addr` and `ramdisk_size` to `ross_common::BootInfo`.
- [ ] **Milestone:** Kernel logs "Found RAMDisk with 5 files" during initialization.

## 2. ELF Process Loading
- [ ] Implement an ELF parser to read Program Headers.
- [ ] Create a `spawn_process` function that creates a new Page Table for an ELF binary.
- [ ] **Milestone:** Successfully run a "User-land" binary that triggers a `print` syscall.

## 3. Minimal Shell (ROSS-SH)
- [ ] Implement a basic line-buffer for keyboard input.
- [ ] Create a command parser (e.g., `help`, `version`, `reboot`).
- [ ] **Milestone:** Type "reboot" in the shell to restart the QEMU machine.

## 4. Virtual File System (VFS)
- [ ] Abstruct file operations into `open`, `read`, and `close`.
- [ ] Mount the RAMDisk as the root directory (`/`).
- [ ] **Milestone:** `cat /hello.txt` prints the file content to the framebuffer.