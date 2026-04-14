#![no_std]

pub mod font;

#[repr(C)]
pub struct BootInfo {
    pub framebuffer_ptr: *mut u8,
    pub framebuffer_size: usize,
    pub screen_width: usize,
    pub screen_height: usize,
}
