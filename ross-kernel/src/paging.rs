/// x86_64 4-level identity page table
/// Maps the first 4 GB of physical memory at both:
///   - Lower half:  0x0000_0000_0000_0000 (for execution during the switch)
///   - Higher half: 0xFFFF_8000_0000_0000 (canonical kernel alias)
///
/// Uses 2 MB huge pages — no 1 GB PDPE1GB CPUID needed.

const PRESENT: u64 = 1 << 0;
const WRITABLE: u64 = 1 << 1;
const USER: u64 = 1 << 2;
const HUGE_PAGE: u64 = 1 << 7; // set in PD entry → 2 MB page

/// A single 4 KB page table holding 512 × 8-byte entries.
#[repr(C, align(4096))]
#[repr(align(4096))]
struct PageTable {
    entries: [u64; 512],
}

impl PageTable {
    const fn new() -> Self {
        Self {
            entries: [0u64; 512],
        }
    }
}

// ── Static page tables (BSS — zero-initialised) ──────────────────────────────

static mut PML4: PageTable = PageTable::new();
static mut PDPT_LOW: PageTable = PageTable::new();
static mut PDPT_HIGH: PageTable = PageTable::new();
static mut PD: [PageTable; 4] = [
    PageTable::new(),
    PageTable::new(),
    PageTable::new(),
    PageTable::new(),
];

pub const KERNEL_VMA_BASE: u64 = 0xFFFFFFFF_80000000;

pub fn phys_to_virt(phys: usize) -> u64 {
    phys as u64 + KERNEL_VMA_BASE
}

pub unsafe fn init() {
    // ... rest of init stays the same
    unsafe {
        crate::serial::serial_print("paging: setting up tables\n");
        let pdpt_low_phys = (core::ptr::addr_of!(PDPT_LOW) as u64) - KERNEL_VMA_BASE;
        let pdpt_high_phys = (core::ptr::addr_of!(PDPT_HIGH) as u64) - KERNEL_VMA_BASE;

        // Lower half:  0x0000_0000_0000_0000
        PML4.entries[0] = pdpt_low_phys | PRESENT | WRITABLE | USER;
        // Higher half: 0xFFFFFFFF_0000_0000 (covers -4GB to 0)
        PML4.entries[511] = pdpt_high_phys | PRESENT | WRITABLE | USER;

        for i in 0..4usize {
            let pd_phys = (core::ptr::addr_of!(PD[i]) as u64) - KERNEL_VMA_BASE;
            PDPT_LOW.entries[i] = pd_phys | PRESENT | WRITABLE | USER;

            if i == 0 {
                PDPT_HIGH.entries[510] = pd_phys | PRESENT | WRITABLE | USER;
            }
            if i == 1 {
                PDPT_HIGH.entries[511] = pd_phys | PRESENT | WRITABLE | USER;
            }
            if i == 2 {
                PDPT_HIGH.entries[508] = pd_phys | PRESENT | WRITABLE | USER;
            }
            if i == 3 {
                PDPT_HIGH.entries[509] = pd_phys | PRESENT | WRITABLE | USER;
            }
        }

        for pd_idx in 0..4usize {
            let base_phys = pd_idx as u64 * 0x4000_0000;
            for entry in 0..512usize {
                let phys = base_phys + entry as u64 * 0x0020_0000;
                PD[pd_idx].entries[entry] = phys | PRESENT | WRITABLE | HUGE_PAGE | USER;
            }
        }

        let pml4_phys = (core::ptr::addr_of!(PML4) as u64) - KERNEL_VMA_BASE;
        core::arch::asm!("mov cr3, {0}", in(reg) pml4_phys, options(nostack));
    }
}

pub fn create_user_address_space_old() -> u64 {
    let pml4_phys = crate::pmm::alloc_page().expect("OOM: PML4");
    let pml4 = unsafe { &mut *(phys_to_virt(pml4_phys) as *mut PageTable) };
    pml4.entries.fill(0);

    // 1. Copy Higher half kernel alias (covers kernel code/data/heap)
    unsafe {
        pml4.entries[511] = PML4.entries[511];
    }

    // 2. Identity map only the VERY FIRST 2MB (Kernel Entry Window)
    // This allows the kernel to finish the CR3 switch without crashing.
    // The rest of the kernel (stack/heap) is reached via the High Alias (PML4[511]).
    let pdpt_phys = crate::pmm::alloc_page().expect("OOM: PDPT");
    let pdpt = unsafe { &mut *(phys_to_virt(pdpt_phys) as *mut PageTable) };
    pdpt.entries.fill(0);

    let pd_phys = crate::pmm::alloc_page().expect("OOM: PD");
    let pd = unsafe { &mut *(phys_to_virt(pd_phys) as *mut PageTable) };
    pd.entries.fill(0);

    // Identity map only index 0 (0 to 2MB).
    // This provides enough runway for the switch.
    pd.entries[0] = 0x0000_0000 | PRESENT | WRITABLE | HUGE_PAGE;

    pdpt.entries[0] = pd_phys as u64 | PRESENT | WRITABLE;
    pml4.entries[0] = pdpt_phys as u64 | PRESENT | WRITABLE;

    pml4_phys as u64
}

pub fn create_user_address_space() -> u64 {
    let pml4_phys = crate::pmm::alloc_page().expect("OOM: PML4");
    let pml4 = unsafe { &mut *(paging::phys_to_virt(pml4_phys) as *mut PageTable) };
    pml4.entries.fill(0);

    // Keep the Higher-Half alias if you have one
    unsafe {
        pml4.entries[511] = PML4.entries[511];
    }

    let pdpt_phys = crate::pmm::alloc_page().expect("OOM: PDPT");
    let pdpt = unsafe { &mut *(paging::phys_to_virt(pdpt_phys) as *mut PageTable) };
    pdpt.entries.fill(0);

    // Map the first 4 Gigabytes of memory (4 PDPT entries * 1GB each)
    // This ensures the Framebuffer and UEFI MMIO are definitely mapped.
    for pdpt_idx in 0..4 {
        let pd_phys = crate::pmm::alloc_page().expect("OOM: PD");
        let pd = unsafe { &mut *(paging::phys_to_virt(pd_phys) as *mut PageTable) };
        pd.entries.fill(0);

        for pd_idx in 0..512 {
            // PUNCH THE HOLE: Skip the 4MB mark (PDPT 0, PD 2)
            // This leaves 0x400000 entirely blank for map_user_page() to use
            if pdpt_idx == 0 && pd_idx == 2 {
                continue;
            }

            let phys_addr = (pdpt_idx as u64 * 512 + pd_idx as u64) * 0x200000;
            // 0x80 = Huge Page (2MB), 0x2 = R/W, 0x1 = Present
            pd.entries[pd_idx] = phys_addr | 0x80 | 0x2 | 0x1;
        }

        // Link the PD to the PDPT (0x4 = USER access allowed through this directory)
        pdpt.entries[pdpt_idx] = pd_phys as u64 | 0x4 | 0x2 | 0x1;
    }

    pml4.entries[0] = pdpt_phys as u64 | 0x4 | 0x2 | 0x1;

    pml4_phys as u64
}

pub fn map_user_page(pml4_phys: u64, vaddr: u64, paddr: u64, flags: u64) {
    let pml4 = unsafe { &mut *(phys_to_virt(pml4_phys as usize) as *mut PageTable) };

    let pml4_idx = (vaddr >> 39) & 0x1FF;
    let pdpt_idx = (vaddr >> 30) & 0x1FF;
    let pd_idx = (vaddr >> 21) & 0x1FF;
    let pt_idx = (vaddr >> 12) & 0x1FF;

    // Get or create PDPT
    if pml4.entries[pml4_idx as usize] == 0 {
        let phys = crate::pmm::alloc_page().expect("OOM: PDPT");
        let ptr = phys_to_virt(phys) as *mut PageTable;
        unsafe {
            (*ptr).entries.fill(0);
        }
        pml4.entries[pml4_idx as usize] = phys as u64 | PRESENT | WRITABLE | USER;
    }
    let pdpt_phys = pml4.entries[pml4_idx as usize] & !0xFFF;
    let pdpt = unsafe { &mut *(phys_to_virt(pdpt_phys as usize) as *mut PageTable) };

    // Get or create PD
    if pdpt.entries[pdpt_idx as usize] == 0 {
        let phys = crate::pmm::alloc_page().expect("OOM: PD");
        let ptr = phys_to_virt(phys) as *mut PageTable;
        unsafe {
            (*ptr).entries.fill(0);
        }
        pdpt.entries[pdpt_idx as usize] = phys as u64 | PRESENT | WRITABLE | USER;
    }
    let pd_phys = pdpt.entries[pdpt_idx as usize] & !0xFFF;
    let pd = unsafe { &mut *(phys_to_virt(pd_phys as usize) as *mut PageTable) };

    // Get or create PT
    if pd.entries[pd_idx as usize] == 0 {
        let phys = crate::pmm::alloc_page().expect("OOM: PT");
        let ptr = phys_to_virt(phys) as *mut PageTable;
        unsafe {
            (*ptr).entries.fill(0);
        }
        pd.entries[pd_idx as usize] = phys as u64 | PRESENT | WRITABLE | USER;
    }
    let pt_phys = pd.entries[pd_idx as usize] & !0xFFF;
    let pt = unsafe { &mut *(phys_to_virt(pt_phys as usize) as *mut PageTable) };

    pt.entries[pt_idx as usize] = paddr | flags | PRESENT | USER;
}
