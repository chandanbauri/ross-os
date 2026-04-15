use uefi::boot::MemoryType;
pub fn setup_higher_half_map() {
    unsafe {
        let mut cr3: u64;
        core::arch::asm!("mov {}, cr3", out(reg) cr3);
        let pml4 = (cr3 & !0xFFF) as *mut u64;

        let pdpt_phys = uefi::boot::allocate_pages(
            uefi::boot::AllocateType::AnyPages,
            MemoryType::LOADER_DATA,
            1,
        ).unwrap();
        let pdpt = pdpt_phys.as_ptr() as *mut u64;

        let pd_phys = uefi::boot::allocate_pages(
            uefi::boot::AllocateType::AnyPages,
            MemoryType::LOADER_DATA,
            1,
        ).unwrap();
        let pd = pd_phys.as_ptr() as *mut u64;

        core::ptr::write_bytes(pdpt, 0, 512);
        core::ptr::write_bytes(pd, 0, 512);

        pml4.add(511).write((pdpt_phys.as_ptr() as u64) | 0x3);
        pdpt.add(510).write((pd_phys.as_ptr() as u64) | 0x3);

        for i in 0..512 {
            let phys = i as u64 * 0x200000;
            pd.add(i).write(phys | 0x3 | 0x80);
        }

        core::arch::asm!("mov cr3, {}", in(reg) cr3);
    }
}
