/// x86_64 4-level identity page table
/// Maps the first 4 GB of physical memory at both:
///   - Lower half:  0x0000_0000_0000_0000 (for execution during the switch)
///   - Higher half: 0xFFFF_8000_0000_0000 (canonical kernel alias)
///
/// Uses 2 MB huge pages — no 1 GB PDPE1GB CPUID needed.

const PRESENT:   u64 = 1 << 0;
const WRITABLE:  u64 = 1 << 1;
const USER:      u64 = 1 << 2;
const HUGE_PAGE: u64 = 1 << 7; // set in PD entry → 2 MB page

/// A single 4 KB page table holding 512 × 8-byte entries.
#[repr(C, align(4096))]
#[repr(align(4096))]
struct PageTable {
    entries: [u64; 512],
}

impl PageTable {
    const fn new() -> Self {
        Self { entries: [0u64; 512] }
    }
}

// ── Static page tables (BSS — zero-initialised) ──────────────────────────────

static mut PML4: PageTable             = PageTable::new();
static mut PDPT_LOW: PageTable         = PageTable::new();
static mut PDPT_HIGH: PageTable        = PageTable::new();
static mut PD:   [PageTable; 4]        = [
    PageTable::new(), PageTable::new(),
    PageTable::new(), PageTable::new(),
];

const KERNEL_VMA_BASE: u64 = 0xFFFFFFFF_80000000;

pub unsafe fn init() {
    unsafe {
        crate::serial::serial_print("paging: setting up tables\n");
        let pdpt_low_phys = (core::ptr::addr_of!(PDPT_LOW) as u64) - KERNEL_VMA_BASE;
        let pdpt_high_phys = (core::ptr::addr_of!(PDPT_HIGH) as u64) - KERNEL_VMA_BASE;

        // Lower half:  0x0000_0000_0000_0000
        PML4.entries[0]   = pdpt_low_phys | PRESENT | WRITABLE | USER;
        // Higher half: 0xFFFFFFFF_0000_0000 (covers -4GB to 0)
        PML4.entries[511] = pdpt_high_phys | PRESENT | WRITABLE | USER;

        for i in 0..4usize {
            let pd_phys = (core::ptr::addr_of!(PD[i]) as u64) - KERNEL_VMA_BASE;
            PDPT_LOW.entries[i] = pd_phys | PRESENT | WRITABLE | USER;
            
            // Map physical 0..4GB to virtual -4GB..0
            // i=0 (P:0-1G)   -> Index 510 (-2G to -1G) -- WAIT, no.
            // Let's do it cleanly:
            // Virtual -2GB..-1GB (Index 510) -> Physical 0..1GB (PD[0])
            // Virtual -1GB.. 0GB (Index 511) -> Physical 1..2GB (PD[1])
            if i == 0 { PDPT_HIGH.entries[510] = pd_phys | PRESENT | WRITABLE | USER; }
            if i == 1 { PDPT_HIGH.entries[511] = pd_phys | PRESENT | WRITABLE | USER; }
            // Optional: Map more for FB etc.
            if i == 2 { PDPT_HIGH.entries[508] = pd_phys | PRESENT | WRITABLE | USER; } // -4G to -3G
            if i == 3 { PDPT_HIGH.entries[509] = pd_phys | PRESENT | WRITABLE | USER; } // -3G to -2G
        }

        // PD entries: 512 × 2 MB = 1 GB per PD, 4 PDs = first 4 GB
        for pd_idx in 0..4usize {
            let base_phys = pd_idx as u64 * 0x4000_0000; // 1 GB per PD
            for entry in 0..512usize {
                let phys = base_phys + entry as u64 * 0x0020_0000; // 2 MB steps
                PD[pd_idx].entries[entry] = phys | PRESENT | WRITABLE | HUGE_PAGE | USER;
            }
        }

        crate::serial::serial_print("paging: moving to cr3\n");
        // Flush TLB by reloading CR3 with our new PML4
        let pml4_phys = (core::ptr::addr_of!(PML4) as u64) - KERNEL_VMA_BASE;
        core::arch::asm!("mov cr3, {0}", in(reg) pml4_phys, options(nostack));
        crate::serial::serial_print("Paging: CR3 switch survived in kernel\n");
    }
}
