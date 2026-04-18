# ROSS OS Project Architecture

This document provides a detailed overview of the ROSS (Rapid Operating System Shell) architecture, documenting the implementation details and the relationship between various components.

## High-Level Overview

ROSS is a 64-bit, bare-metal operating system written in Rust, targeting the x86_64 architecture. It follows a monolithic kernel design where major system services (memory management, multitasking, file systems) run in kernel space.

### Core Components

- **`rb-loader/`**: A UEFI bootloader that initializes hardware, sets up a basic execution environment, and loads the kernel.
- **`ross-kernel/`**: The core of the OS, responsible for hardware abstraction, process management, and resource allocation.
- **`ross-common/`**: Shared types and utilities used by both the kernel and user-land applications.
- **`ross-user/`**: User-mode applications (e.g., the `init` process).

---

## 1. Bootloader (`rb-loader`)

The bootloader is the first point of entry. It is a UEFI application that:
- Transitions the CPU to 64-bit Long Mode (if not already there).
- Retrieves the Memory Map from UEFI.
- Initializes the Framebuffer for graphics output.
- Loads the kernel binary from the disk.
- Sets up a `BootInfo` structure to pass hardware information to the kernel.
- Hands over control to the kernel entry point.

---

## 2. Kernel Architecture (`ross-kernel`)

The kernel is organized into several modules, each handling a specific subsystem.

### A. Memory Management
- **Physical Memory Manager (`pmm.rs`)**: Manages physical pages using a bitmap or stack of free frames based on the UEFI memory map.
- **Paging (`paging.rs`)**: Implements 4-level paging. It handles mapping virtual addresses to physical frames and provides isolation between kernel and user address spaces.
- **Heap Allocator (`heap.rs`)**: Provides dynamic memory allocation (`alloc`, `free`) for kernel-level data structures using a linked-list or buddy allocator.

### B. Hardware Abstraction & CPU State
- **GDT (`gdt.rs`)**: Defines the Global Descriptor Table, including Kernel Code/Data segments, User Code/Data segments, and the Task State Segment (TSS).
- **IDT (`idt.rs`)**: Sets up the Interrupt Descriptor Table to handle exceptions (e.g., Page Faults, GPFs) and hardware interrupts.
- **PIC/PIT (`pic.rs`, `pit.rs`)**: Handles the legacy Programmable Interrupt Controller and sets up the Programmable Interval Timer for system clock ticks.

### C. Multitasking & Scheduling
- **Task Management (`task.rs`)**: Defines the `Task` structure and the scheduler. ROSS support preemptive multitasking.
- **Context Switching**: Implemented in assembly or naked functions to save and restore CPU registers during task transitions.
- **ELF Loader (`elf.rs`)**: Parses ELF files to load user-mode programs into memory, setting up their stack and segments.

### D. System Calls (`syscall.rs`)
The system call interface allows user-land applications to request kernel services.
- **Mechanism**: Uses the `SYSCALL` and `SYSRET` instructions for fast transitions.
- **Configuration**: Sets up the `STAR`, `LSTAR`, and `SFMASK` Model Specific Registers (MSRs).

### E. File System & I/O
- **VFS (`vfs.rs`)**: An abstraction layer for file operations.
- **RAMFS (`ramfs.rs`)**: A temporary file system stored in memory, used for the initial root filesystem.
- **Drivers**:
  - `keyboard.rs`: PS/2 keyboard driver.
  - `serial.rs`: UART serial communication for debugging/logging.
  - `writer.rs`: Graphics output following the UEFI framebuffer specification.

---

## 3. Implementation Details

### Page Table Isolation
To protect the kernel from user-mode tasks, each user process has its own set of page tables. Kernel mappings are kept at the top of the address space (`0xFFFFFFFF80000000` onwards) and are marked as "Supervisor Only" in user page tables.

### The `init` Process
The first user-mode process, `init.elf`, is loaded by the kernel. It serves as the parent of all other processes and is responsible for initializing user-space services.

### Transitions (Ring 0 to Ring 3)
Transitions to Ring 3 (user mode) involve:
1. Setting up User-mode stack and code selectors in the GDT.
2. Loading the user-mode page table.
3. Using `sysretq` or `iretq` with the appropriate stack frame to jump to the user-mode entry point.

---

## 4. Build & Deployment

The project uses a custom automation script `boot.sh` which:
1. Builds the `ross-common` library.
2. Compiles the kernel using a custom linker script (`linker.ld`).
3. Compiles user-land programs into ELF binaries.
4. Packages the kernel and user binaries into a disk image.
5. Launches QEMU with the appropriate UEFI firmware.
