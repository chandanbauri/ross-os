#![no_std]
#![no_main]

extern crate alloc;

mod handoff;

use ross_common::font::FONT_BASIC;
use ross_common::{MemoryKind, MemoryRegion};
use uefi::boot::MemoryType;
use uefi::mem::memory_map::MemoryMap;
use uefi::prelude::*;
use uefi::proto::console::gop::GraphicsOutput;

#[global_allocator]
static ALLOCATOR: uefi::allocator::Allocator = uefi::allocator::Allocator;

// Static storage for the memory map — outlives UEFI boot services
const MAX_REGIONS: usize = 128;
static mut MEM_REGIONS: [MemoryRegion; MAX_REGIONS] = [MemoryRegion {
    base: 0,
    size: 0,
    kind: MemoryKind::Reserved,
}; MAX_REGIONS];
static mut MEM_REGION_COUNT: usize = 0;

fn draw_string(ptr: *mut u8, width: usize, mut x: usize, y: usize, text: &str) {
    for c in text.chars() {
        let ascii = c as usize;
        if ascii < 128 {
            let glyph = FONT_BASIC[ascii];
            for gy in 0..8 {
                let row = glyph[gy];
                for gx in 0..8 {
                    if (row >> (7 - gx)) & 1 == 1 {
                        let offset = ((y + gy) * width + (x + gx)) * 4;
                        unsafe {
                            *ptr.add(offset) = 255;
                            *ptr.add(offset + 1) = 255;
                            *ptr.add(offset + 2) = 255;
                        }
                    }
                }
            }
            x += 10;
        }
    }
}

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();

    uefi::println!("Booting ROSS...");

    // Capture UEFI memory map before touching anything else
    let mmap = uefi::boot::memory_map(MemoryType::LOADER_DATA).unwrap();
    let mut count = 0usize;
    for desc in mmap.entries() {
        if count >= MAX_REGIONS { break; }
        let kind = match desc.ty {
            MemoryType::CONVENTIONAL => MemoryKind::Usable,
            MemoryType::LOADER_CODE | MemoryType::LOADER_DATA => MemoryKind::Used,
            _ => MemoryKind::Reserved,
        };
        unsafe {
            MEM_REGIONS[count] = MemoryRegion {
                base: desc.phys_start,
                size: desc.page_count * 4096,
                kind,
            };
        }
        count += 1;
    }
    unsafe { MEM_REGION_COUNT = count; }
    uefi::println!("Memory map: {} regions", count);

    let gop_handle = uefi::boot::get_handle_for_protocol::<GraphicsOutput>().unwrap();
    let mut gop = uefi::boot::open_protocol_exclusive::<GraphicsOutput>(gop_handle).unwrap();

    let (width, height) = gop.current_mode_info().resolution();
    uefi::println!("Resolution: {}x{}", width, height);

    let ptr = gop.frame_buffer().as_mut_ptr();
    uefi::println!("Framebuffer: {:?}", ptr);

    unsafe {
        let fb_u32 = ptr as *mut u32;
        for i in 0..(width * height) {
            *fb_u32.add(i) = 0x00_80_00_00;
        }

        draw_string(ptr, width, 50, 50, "ROSS Loader Active");

        let kernel_ptr = handoff::load_kernel_file();
        uefi::println!("Kernel loaded. Entry point: {:?}", kernel_ptr);

        draw_string(ptr, width, 50, 100, "Jumping to Kernel...");

        let info = ross_common::BootInfo {
            framebuffer_ptr:  ptr,
            framebuffer_size: width * height * 4,
            screen_width:     width,
            screen_height:    height,
            memory_map:       core::ptr::addr_of!(MEM_REGIONS) as *const _,
            memory_map_len:   MEM_REGION_COUNT,
        };

        handoff::map_higher_half();

        let kernel_entry: extern "C" fn(&ross_common::BootInfo) -> ! =
            core::mem::transmute(0xFFFFFFFF_80200000u64);

        kernel_entry(&info);
    }
}
