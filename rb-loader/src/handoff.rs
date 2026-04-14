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
