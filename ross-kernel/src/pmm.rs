use ross_common::{MemoryKind, MemoryRegion};
use spin::Mutex;

const PAGE_SIZE:    usize = 4096;
const BITMAP_BYTES: usize = 32 * 1024;        // 32 KB
const MAX_PAGES:    usize = BITMAP_BYTES * 8;  // 262 144 pages → 1 GB

struct PmmState {
    bitmap:      [u8; BITMAP_BYTES],
    free_count:  usize,
    total_count: usize,
}

impl PmmState {
    const fn new() -> Self {
        Self {
            bitmap:      [0xFF; BITMAP_BYTES], // all allocated until init() runs
            free_count:  0,
            total_count: 0,
        }
    }
}

static PMM: Mutex<PmmState> = Mutex::new(PmmState::new());

pub fn init(map: *const MemoryRegion, count: usize) {
    if map.is_null() || count == 0 { return; }
    let regions = unsafe { core::slice::from_raw_parts(map, count) };
    let mut state = PMM.lock();
    for region in regions {
        if region.kind == MemoryKind::Usable {
            let start = region.base as usize / PAGE_SIZE;
            let pages = region.size as usize / PAGE_SIZE;
            for i in start..(start + pages).min(MAX_PAGES) {
                let byte = i / 8;
                let bit  = i % 8;
                if state.bitmap[byte] & (1 << bit) != 0 {
                    state.bitmap[byte] &= !(1 << bit);
                    state.free_count  += 1;
                    state.total_count += 1;
                }
            }
        }
    }
}

/// Allocate one 4 KB physical page. Returns its physical address.
pub fn alloc_page() -> Option<usize> {
    let mut state = PMM.lock();
    for byte_idx in 0..BITMAP_BYTES {
        let byte = state.bitmap[byte_idx];
        if byte == 0xFF { continue; }
        for bit in 0..8u8 {
            if byte & (1 << bit) == 0 {
                state.bitmap[byte_idx] |= 1 << bit;
                state.free_count = state.free_count.saturating_sub(1);
                return Some((byte_idx * 8 + bit as usize) * PAGE_SIZE);
            }
        }
    }
    None
}

/// Free a previously-allocated page.
pub fn free_page(addr: usize) {
    let page = addr / PAGE_SIZE;
    if page >= MAX_PAGES { return; }
    let mut state = PMM.lock();
    let byte = page / 8;
    let bit  = page % 8;
    if state.bitmap[byte] & (1 << bit) != 0 {
        state.bitmap[byte] &= !(1 << bit);
        state.free_count   += 1;
    }
}

pub fn free_pages() -> usize { PMM.lock().free_count }
pub fn free_mib()   -> usize { free_pages() * PAGE_SIZE / (1024 * 1024) }
pub fn total_mib()  -> usize { PMM.lock().total_count * PAGE_SIZE / (1024 * 1024) }
