#![no_std]

pub mod font;

/// A physical memory region as reported by UEFI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MemoryKind {
    Usable   = 0, // Free conventional RAM
    Used     = 1, // Kernel / loader pages
    Reserved = 2, // Firmware, MMIO, ACPI, etc.
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MemoryRegion {
    pub base: u64,
    pub size: u64,
    pub kind: MemoryKind,
}

/// Passed from the loader to the kernel at handoff.
#[repr(C)]
pub struct BootInfo {
    pub framebuffer_ptr:    *mut u8,
    pub framebuffer_size:   usize,
    pub screen_width:       usize,
    pub screen_height:      usize,
    pub memory_map:         *const MemoryRegion,
    pub memory_map_len:     usize,
}
