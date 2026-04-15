use linked_list_allocator::LockedHeap;

const HEAP_SIZE: usize = 2 * 1024 * 1024; // 2 MB

/// Physical backing store for the kernel heap.
/// Aligned to a page boundary so the allocator can sub-divide freely.
#[repr(align(4096))]
struct HeapStorage([u8; HEAP_SIZE]);

// Zero-initialised (BSS).  The linked_list_allocator writes its internal
// free-list header over whatever bytes are here during init(), so the
// value before init doesn't matter.
static mut HEAP_STORAGE: HeapStorage = HeapStorage([0; HEAP_SIZE]);

/// Global allocator instance — backing the `alloc` crate (Vec, Box, String…).
#[global_allocator]
static HEAP: LockedHeap = LockedHeap::empty();

/// Initialise the kernel heap.  Must be called after `paging::init()`.
pub fn init() {
    unsafe {
        let start = core::ptr::addr_of_mut!(HEAP_STORAGE.0) as *mut u8;
        HEAP.lock().init(start, HEAP_SIZE);
    }
}
