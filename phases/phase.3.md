# ROSS Roadmap: Phase 3 (Memory & Resources)

This phase transforms ROSS from a static binary into a dynamic system capable of managing its own resources.

## 1. Physical Memory Management
- [ ] Update `ross-common` `BootInfo` to include `MemoryMap` data.
- [ ] Implement a **Bitmap Allocator** to track used/free 4KB frames.
- [ ] **Milestone:** Log the total amount of "Available" RAM detected from UEFI.

## 2. Virtual Memory (Paging)
- [ ] Implement recursive page table mapping or an identity map for the framebuffer.
- [ ] Map the kernel to the **Higher Half** (Canonical Address Space).
- [ ] **Milestone:** Successfully switch `CR3` to a kernel-managed Page Table.

## 3. Dynamic Memory (The Heap)
- [ ] Initialize a `LockedHeap` starting at a safe virtual address.
- [ ] Enable the `alloc` crate in `ross-kernel`.
- [ ] **Milestone:** Create a `Vec<u32>` and push values to it without crashing.

## 4. Time & Interaction
- [ ] Initialize the PIT (Programmable Interval Timer) to 100Hz.
- [ ] Map IRQ 0 (Timer) and IRQ 1 (Keyboard) in the IDT.
- [ ] **Milestone:** Implement a `sleep(ms)` function and a basic keyboard buffer.