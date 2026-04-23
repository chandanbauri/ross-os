# ROSS Roadmap: Phase 7 (Hardware & Storage)

Phase 7 transitions ROSS from a memory-resident system to a fully persistent OS by introducing PCI enumeration | native disk drivers | and a read/write filesystem.

## 1. PCI Bus Enumeration
- [ ] Implement port I/O or Memory-Mapped I/O (MMCFG) to read the PCI config space.
- [ ] Scan all buses | devices | and functions to build a hardware registry.
- [ ] **Milestone:** The `ross-sh` command `lspci` prints a list of all connected hardware.

## 2. AHCI Storage Driver
- [ ] Locate the AHCI controller via the PCI registry.
- [ ] Initialize the Host Bus Adapter (HBA) and allocate DMA command tables.
- [ ] **Milestone:** Successfully read Sector 0 of the QEMU hard drive into a kernel buffer.

## 3. Persistent File System
- [ ] Implement a FAT32 (or ext2) driver supporting directories and clusters.
- [ ] Mount the physical drive to `/mnt/disk` in the VFS.
- [ ] **Milestone:** The shell can create | write | and read back a new file from the persistent disk.

## 4. Inter-Process Communication (IPC)
- [ ] Implement a ring buffer in kernel space for IPC Pipes.
- [ ] Add `sys_pipe` | `sys_read` | and `sys_write` support for standard I/O redirection.
- [ ] **Milestone:** Two separate ELF binaries can pass data to each other.
