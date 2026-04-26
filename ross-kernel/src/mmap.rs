/// Anonymous page allocator for user-space mmap/brk.
/// Maps `pages` contiguous virtual pages starting at `base` into `cr3`.
/// All pages are zeroed and mapped Present|Writable|User.
pub fn alloc_anon(cr3: u64, base: u64, pages: usize) -> Result<(), &'static str> {
    for i in 0..pages {
        let vaddr = base + (i as u64) * 4096;
        let phys  = crate::pmm::alloc_page().ok_or("mmap: OOM")?;
        let kptr  = crate::paging::phys_to_virt(phys) as *mut u8;
        unsafe { core::ptr::write_bytes(kptr, 0, 4096); }
        crate::paging::map_user_page(cr3, vaddr, phys as u64, 0x7); // P|W|U
    }
    Ok(())
}
