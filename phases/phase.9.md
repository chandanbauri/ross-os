# ROSS Roadmap: Phase 9 (Symmetric Multiprocessing)

Phase 9 transitions ROSS from a single-threaded environment to a modern, multi-core architecture by waking up Application Processors (APs) and implementing bare-metal synchronization.

## 1. ACPI Discovery
- [ ] Read the RSDP (Root System Description Pointer) passed from `rb-loader`.
- [ ] Parse the RSDT/XSDT to find the MADT.
- [ ] **Milestone:** The kernel logs the exact number of CPU cores available on the host machine.

## 2. Advanced Programmable Interrupt Controller (APIC)
- [ ] Disable the legacy 8259 PIC.
- [ ] Identity map and initialize the Local APIC (LAPIC) for the BSP.
- [ ] **Milestone:** The system timer (IRQ0) successfully fires using the APIC Timer instead of the legacy PIT.

## 3. Core Activation (Trampoline)
- [ ] Write a 16-bit to 64-bit trampoline in assembly and copy it below the 1MB memory mark.
- [ ] Send the INIT-SIPI-SIPI sequence to an Application Processor.
- [ ] **Milestone:** Core #1 successfully wakes up, transitions to Long Mode, and prints "AP 1 Online" to the serial port.

## 4. Concurrency & Spinlocks
- [ ] Implement an atomic `Spinlock` wrapper for shared global state.
- [ ] Ensure the Physical Memory Manager (PMM) and Framebuffer are thread-safe.
- [ ] **Milestone:** Two separate CPU cores can simultaneously push values to the same kernel `Vec` without memory corruption.
