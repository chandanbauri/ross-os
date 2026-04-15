# ROSS Roadmap: Phase 2 (System Initialization)

This document outlines the next engineering milestones for the Rapid Operating System Shell (ROSS) following a successful UEFI handoff.

## 1. Segmentation & Stack (GDT)
- [ ] Define the Global Descriptor Table (GDT) structure in `ross-kernel`.
- [ ] Create segments for Kernel Code (index 1) and Kernel Data (index 2).
- [ ] Load the new GDT using the `lgdt` instruction.
- [ ] **Milestone:** Successfully reload the stack pointer (`rsp`) to point to a kernel-owned memory region.

## 2. Exception Handling (IDT)
- [ ] Implement the Interrupt Descriptor Table (IDT) to catch CPU exceptions.
- [ ] Map the first 32 x86_64 exceptions (e.g., #DE, #BP, #PF).
- [ ] Create a "Panic Screen" function that renders the exception name and register state to the framebuffer.
- [ ] **Milestone:** Trigger a manual breakpoint (`int3`) and see a "Breakpoint Hit" message on screen without a reboot.

## 3. Physical Memory Manager (PMM)
- [ ] Update `BootInfo` in `ross-common` to include the UEFI Memory Map.
- [ ] In `rb-loader`, capture the memory map just before exiting boot services.
- [ ] Implement a **Bitmap Page Allocator** in the kernel.
- [ ] **Milestone:** Kernel can successfully allocate and free a 4KB page of physical RAM.

## 4. Kernel Logging Infrastructure
- [ ] Move `FONT_BASIC` and `put_str` logic into a dedicated `writer` module.
- [ ] Implement `core::fmt::Write` for the global kernel logger.
- [ ] **Milestone:** Use `println!("Memory Map: {} regions found", count)` during boot.

## 5. Keyboard Input (PS/2)
- [ ] Initialize the 8259 PIC (Programmable Interrupt Controller) or APIC.
- [ ] Unmask the keyboard interrupt (IRQ 1).
- [ ] Create a scancode-to-ASCII translation table.
- [ ] **Milestone:** "Starting..." splash screen disappears when the user presses 'Enter'.