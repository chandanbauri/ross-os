# ROSS Roadmap: Phase 10 (Standardization & GUI)

Phase 10 transforms ROSS into a standardized platform capable of running ported software and rendering a graphical interface.

## 1. The Standard Library (libc)
- [ ] Create a `x86_64-ross` target specification.
- [ ] Port `newlib` or `relibc` by implementing standard syscall stubs.
- [ ] **Milestone:** Successfully compile and run a standard "Hello World" written in plain C or Rust without `#[no_std]`.

## 2. USB Subsystem (xHCI)
- [ ] Locate the xHCI controller on the PCI bus.
- [ ] Initialize the command ring and device context base address array.
- [ ] **Milestone:** A physical USB mouse movement prints coordinate changes to the serial port.

## 3. Window Manager
- [ ] Implement a back-buffer for double-buffered rendering.
- [ ] Create a `Window` struct with x/y coordinates | z-index | and pixel buffers.
- [ ] **Milestone:** Render a movable square that follows the USB mouse coordinates.
