use ross_common::{MemoryKind, MemoryRegion};

const PAGE_SIZE:    usize = 4096;
const BITMAP_BYTES: usize = 32 * 1024;          // 32 KB
const MAX_PAGES:    usize = BITMAP_BYTES * 8;   // 262 144 pages → 1 GB

// Initialized to 0xFF (all allocated); init() marks usable pages free.
static mut BITMAP:     [u8; BITMAP_BYTES] = [0xFF; BITMAP_BYTES];
static mut FREE_COUNT: usize = 0;
static mut TOTAL_COUNT: usize = 0;

pub fn init(map: *const MemoryRegion, count: usize) {
    if map.is_null() || count == 0 {
        return;
    }
    let regions = unsafe { core::slice::from_raw_parts(map, count) };
    for region in regions {
        if region.kind == MemoryKind::Usable {
            let start = region.base as usize / PAGE_SIZE;
            let pages = region.size as usize / PAGE_SIZE;
            for i in start..(start + pages).min(MAX_PAGES) {
                unsafe {
                    let byte = i / 8;
                    let bit  = i % 8;
                    if BITMAP[byte] & (1 << bit) != 0 {
                        BITMAP[byte] &= !(1 << bit);
                        FREE_COUNT += 1;
                        TOTAL_COUNT += 1;
                    }
                }
            }
        }
    }
}

/// Allocate one 4 KB physical page. Returns its physical address.
#[allow(dead_code)]
pub fn alloc_page() -> Option<usize> {
    unsafe {
        for byte_idx in 0..BITMAP_BYTES {
            let byte = BITMAP[byte_idx];
            if byte == 0xFF { continue; }
            for bit in 0..8u8 {
                if byte & (1 << bit) == 0 {
                    BITMAP[byte_idx] |= 1 << bit;
                    FREE_COUNT = FREE_COUNT.saturating_sub(1);
                    return Some((byte_idx * 8 + bit as usize) * PAGE_SIZE);
                }
            }
        }
    }
    None
}

/// Free a previously-allocated page.
#[allow(dead_code)]
pub fn free_page(addr: usize) {
    let page = addr / PAGE_SIZE;
    if page >= MAX_PAGES { return; }
    unsafe {
        let byte = page / 8;
        let bit  = page % 8;
        if BITMAP[byte] & (1 << bit) != 0 {
            BITMAP[byte] &= !(1 << bit);
            FREE_COUNT += 1;
        }
    }
}

/// Free pages remaining in the allocator.
pub fn free_pages()  -> usize { unsafe { FREE_COUNT } }

/// Free memory in mebibytes.
pub fn free_mib() -> usize { free_pages() * PAGE_SIZE / (1024 * 1024) }

/// Total usable memory in mebibytes.
pub fn total_mib() -> usize { (unsafe { TOTAL_COUNT }) * PAGE_SIZE / (1024 * 1024) }

