use uefi::boot::MemoryType;
use uefi::proto::media::file::{File, FileAttribute, FileInfo, FileMode};
use uefi::proto::media::fs::SimpleFileSystem;

pub fn load_kernel_file() -> *const u8 {
    let fs_handle = uefi::boot::get_handle_for_protocol::<SimpleFileSystem>().unwrap();
    let mut fs = uefi::boot::open_protocol_exclusive::<SimpleFileSystem>(fs_handle).unwrap();
    let mut root = fs.open_volume().unwrap();

    let handle = root
        .open(
            uefi::cstr16!("kernel.elf"),
            FileMode::Read,
            FileAttribute::empty(),
        )
        .expect("Kernel not found");

    let mut file = match handle.into_type().unwrap() {
        uefi::proto::media::file::FileType::Regular(f) => f,
        _ => panic!("Not a regular file"),
    };

    let mut info_buf = [0u8; 128];
    let size = file
        .get_info::<FileInfo>(&mut info_buf)
        .unwrap()
        .file_size() as usize;
    let pages = (size + 4095) / 4096;

    let addr = uefi::boot::allocate_pages(
        uefi::boot::AllocateType::Address(0x200000),
        MemoryType::LOADER_CODE,
        pages,
    )
    .unwrap_or_else(|_| {
        uefi::boot::allocate_pages(
            uefi::boot::AllocateType::AnyPages,
            MemoryType::LOADER_CODE,
            pages,
        )
        .unwrap()
    });

    let buffer = unsafe { core::slice::from_raw_parts_mut(addr.as_ptr(), size) };
    file.read(buffer).unwrap();

    addr.as_ptr() as *const u8
}

#[repr(align(4096))]
struct PageTable([u64; 512]);

static mut PML4: PageTable = PageTable([0; 512]);
static mut PDPT: PageTable = PageTable([0; 512]);
static mut PD:   PageTable = PageTable([0; 512]);

pub fn map_higher_half() {
    unsafe {
        let mut cr3: u64;
        core::arch::asm!("mov {}, cr3", out(reg) cr3);
        let old_pml4 = (cr3 & !0xFFF) as *const u64;

        // Copy Entry 0 for identity map
        PML4.0[0] = old_pml4.read();

        // Setup -2GB mapping
        PML4.0[511] = (core::ptr::addr_of!(PDPT) as u64) | 0x3;
        PDPT.0[510] = (core::ptr::addr_of!(PD)   as u64) | 0x3;

        for i in 0..512 {
            let phys = i as u64 * 0x200000;
            PD.0[i] = phys | 0x3 | 0x80;
        }

        core::arch::asm!("mov cr3, {}", in(reg) core::ptr::addr_of!(PML4) as u64);
    }
}
