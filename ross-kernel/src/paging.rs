/// x86_64 4-level identity page table
/// Maps the first 4 GB of physical memory at both:
///   - Lower half:  0x0000_0000_0000_0000 (for execution during the switch)
///   - Higher half: 0xFFFF_8000_0000_0000 (canonical kernel alias)
///
/// Uses 2 MB huge pages — no 1 GB PDPE1GB CPUID needed.

const PRESENT:   u64 = 1 << 0;
const WRITABLE:  u64 = 1 << 1;
const HUGE_PAGE: u64 = 1 << 7; // set in PD entry → 2 MB page

/// A single 4 KB page table holding 512 × 8-byte entries.
#[repr(C, align(4096))]
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
static mut PDPT: PageTable             = PageTable::new(); // index 0 + 256 share this
static mut PD:   [PageTable; 4]        = [
    PageTable::new(), PageTable::new(),
    PageTable::new(), PageTable::new(),
];

/// Set up a kernel-managed identity map and switch CR3.
///
/// After this function returns the kernel continues executing normally—
/// it still runs from its identity-mapped physical address.
pub unsafe fn init() {
    unsafe {
        let pdpt_phys = core::ptr::addr_of!(PDPT) as u64;

        // PML4[0]   → PDPT  (lower half:  0x0000_0000_0000_0000)
        PML4.entries[0]   = pdpt_phys | PRESENT | WRITABLE;
        // PML4[256] → PDPT  (higher half: 0xFFFF_8000_0000_0000)
        PML4.entries[256] = pdpt_phys | PRESENT | WRITABLE;

        // PDPT[0..3]: each entry → one PD covering 1 GB
        for i in 0..4usize {
            let pd_phys = core::ptr::addr_of!(PD[i]) as u64;
            PDPT.entries[i] = pd_phys | PRESENT | WRITABLE;
        }

        // PD entries: 512 × 2 MB = 1 GB per PD, 4 PDs = first 4 GB
        for pd_idx in 0..4usize {
            let base_phys = pd_idx as u64 * 0x4000_0000; // 1 GB per PD
            for entry in 0..512usize {
                let phys = base_phys + entry as u64 * 0x0020_0000; // 2 MB steps
                PD[pd_idx].entries[entry] = phys | PRESENT | WRITABLE | HUGE_PAGE;
            }
        }

        // Flush TLB by reloading CR3 with our new PML4
        let pml4_phys = core::ptr::addr_of!(PML4) as u64;
        core::arch::asm!("mov cr3, {0}", in(reg) pml4_phys, options(nostack));
    }
}
