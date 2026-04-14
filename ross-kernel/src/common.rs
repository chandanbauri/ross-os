
#[repr(C)]
pub struct BootInfo {
    pub framebuffer_ptr: *mut u8,
    pub framebuffer_size: usize,
    pub screen_width: usize,
    pub screen_height: usize,
    // Future: Add Memory Map here for Windows/Linux paging
}
