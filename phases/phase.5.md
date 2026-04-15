# ROSS Roadmap: Phase 5 (Advanced Services)

Phase 5 focuses on memory isolation and the transition from a single-tasking kernel to a multitasking environment.

## 1. Higher-Half Mapping
- [x] Update `linker.ld` to use a Higher-Half virtual address (e.g., `0xFFFFFFFF80000000`).
- [x] Implement a Page Table Manager to map physical frames to the higher-half.
- [x] **Milestone:** Kernel successfully boots and runs at a high virtual address while identity mapping the framebuffer.

## 2. Kernel Threading & Scheduling
- [x] Define a `Task` struct to store CPU register state (RIP, RSP, RAX, etc.).
- [x] Implement a basic Round-Robin scheduler.
- [x] **Milestone:** Two independent kernel functions running concurrently via timer-based context switching.

## 3. System Call Interface
- [x] Set up the EFER (Extended Feature Enable Register) for syscall support.
- [x] Create a syscall entry point in the IDT/MSRs.
- [x] **Milestone:** A "User-land" stub successfully requests the kernel to draw a character on screen.

## 4. Virtual File System (VFS)
- [ ] Define a generic `INode` and `File` trait.
- [ ] Implement a `TarFS` or `RamFS` to load initial system assets from a ramdisk.
- [ ] **Milestone:** Kernel can "read" a text file bundled with the boot image.