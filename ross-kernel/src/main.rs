#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]

extern crate alloc;

mod gdt;
mod heap;
mod idt;
mod kbuf;
mod keyboard;
mod pic;
mod pit;
mod pmm;
mod paging;
mod shell;
mod writer;
mod serial;

use alloc::vec::Vec;
use core::panic::PanicInfo;
use ross_common::BootInfo;

static     GDT:  gdt::Gdt = gdt::Gdt::new();
static mut IDT:  idt::Idt = idt::Idt::new();

#[repr(align(16))]
struct KernelStack([u8; 0x4000]); // 16 KB

#[unsafe(no_mangle)]
#[unsafe(link_section = ".data")]
static mut KERNEL_STACK: KernelStack = KernelStack([0; 0x4000]);


unsafe extern "C" {
    static mut _bss_start: u8;
    static mut _bss_end: u8;
}


static mut SHELL_STATE: shell::State = shell::State::Splash;

core::arch::global_asm!(
    ".section .text._start",
    ".global _start",
    "_start:",
    "cli",
    "lea rsp, [KERNEL_STACK + 0x4000]",
    "mov rdi, rcx",
    "jmp kernel_main"
);



#[unsafe(no_mangle)]
extern "C" fn kernel_main(info: &'static BootInfo) -> ! {
    // ── 1. CPU Fundamentals ─────────────────────────────────────────────────
    unsafe {
        GDT.load();

        // Zero BSS section while on our own writable stack
        let mut curr = core::ptr::addr_of_mut!(_bss_start);
        let end  = core::ptr::addr_of!(_bss_end);
        while (curr as usize) < (end as usize) {
            core::ptr::write_volatile(curr, 0);
            curr = curr.add(1);
        }

        writer::init(info);

        let idt_ptr = core::ptr::addr_of_mut!(IDT);
        idt::init_idt(&mut *idt_ptr);
        (*idt_ptr).load();
    }


    // ── 2. Physical Memory Manager ──────────────────────────────────────────
    pmm::init(info.memory_map, info.memory_map_len);

    // ── 3. Virtual Memory (Paging) ──────────────────────────────────────────
    unsafe { paging::init(); }

    // ── 4. Kernel Heap ──────────────────────────────────────────────────────
    heap::init();

    // ── 5. PIC + PIT + Keyboard ─────────────────────────────────────────────
    unsafe {
        pit::init();  // Programme PIT to 100 Hz before unmasking IRQ0
        pic::init();  // Remap PIC; unmask IRQ0 (timer) and IRQ1 (keyboard)

        // Drain any PS/2 scancodes buffered during UEFI boot
        while pic::inb(0x64) & 0x01 != 0 {
            pic::inb(0x60);
        }

        core::arch::asm!("sti"); // Enable interrupts
    }

    // ── 6. Splash Screen ────────────────────────────────────────────────────
    let w  = info.screen_width;
    let h  = info.screen_height;
    let cx = w / 2;
    let wr = writer::get_writer();

    wr.fill_rect(0, 0, w, h, writer::BG);

    let title       = "R.O.S.S.";
    let title_scale = 5;
    let title_w     = text_width(title, title_scale);
    let title_y     = h / 2 - 80;
    wr.set_pos(cx.saturating_sub(title_w / 2), title_y);
    wr.put_str(title, writer::FG, title_scale);

    let sep_w = (title_w * 6) / 5;
    let sep_y = title_y + 8 * title_scale + 12;
    wr.fill_rect(cx.saturating_sub(sep_w / 2), sep_y, sep_w, 1, writer::FG);

    let sub       = "Rapid Operating System Shell";
    let sub_scale = 2;
    let sub_w     = text_width(sub, sub_scale);
    wr.set_pos(cx.saturating_sub(sub_w / 2), sep_y + 14);
    wr.put_str(sub, writer::DIM, sub_scale);

    let msg       = "Starting...";
    let msg_scale = 2;
    let msg_w     = text_width(msg, msg_scale);
    let msg_y     = h / 2 + 40;
    wr.set_pos(cx.saturating_sub(msg_w / 2), msg_y);
    wr.put_str(msg, writer::FG, msg_scale);

    let bar_w = 320_usize.min(w - 80);
    let bar_x = cx.saturating_sub(bar_w / 2);
    let bar_y = msg_y + 8 * msg_scale + 14;
    wr.fill_rect(bar_x,     bar_y, bar_w,             4, writer::DIM);
    wr.fill_rect(bar_x + 1, bar_y, bar_w * 70 / 100, 4, writer::FG);

    // ── 7. Kernel Log ────────────────────────────────────────────────────────
    wr.set_pos(50, h.saturating_sub(90));
    kprintln!("Memory: {} MiB free  ({} regions)", pmm::free_mib(), info.memory_map_len);
    kprintln!("Paging: CR3 switched to kernel-managed tables");
    kprintln!("Press ENTER to continue...");

    // ── 8. Heap smoke-test (Phase 3 milestone) ───────────────────────────────
    {
        let mut v: Vec<u32> = Vec::new();
        for i in 0u32..8 {
            v.push(i * i);  // 0, 1, 4, 9, 16, 25, 36, 49
        }
        // If we reach here without a crash the heap is working.
        // (kprintln from an alloc context verifies fmt machinery too)
        let _ = v; // drop → heap free
    }

    // ── 9. Main Event Loop ───────────────────────────────────────────────────
    let mut last_ticks = 0;
    loop {
        // 1. Uptime Clock (Top Right Corner)
        let current_ticks = pit::ticks();
        if current_ticks != last_ticks {
            let seconds = current_ticks / 100;
            let old_x = wr.x;
            let old_y = wr.y;

            wr.fill_rect(w - 180, 20, 180, 20, writer::BG);
            wr.set_pos(w - 180, 20);
            
            use core::fmt::Write;
            let _ = write!(wr, "UPTIME: {}s", seconds);
            
            wr.set_pos(old_x, old_y);
            last_ticks = current_ticks;
        }

        // 2. Shell Interaction
        while let Some(sc) = kbuf::pop() {
            if let Some(ascii) = kbuf::scancode_to_ascii(sc) {
                unsafe {
                    let state_ptr = core::ptr::addr_of_mut!(SHELL_STATE);
                    shell::handle_byte(&mut *state_ptr, ascii);
                }
            }
        }


        core::hint::spin_loop();
    }
}

fn text_width(s: &str, scale: usize) -> usize {
    let n = s.len();
    if n == 0 { return 0; }
    n * (8 * scale + scale) - scale
}

#[alloc_error_handler]
fn alloc_error(layout: core::alloc::Layout) -> ! {
    let wr = writer::get_writer();
    wr.fill_rect(0, 0, 3000, 3000, 0x000000CC); // blue screen
    wr.set_pos(80, 100);
    wr.put_str("OUT OF MEMORY", 0x00FFFFFF, 3);
    let _ = layout;
    loop {}
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    use core::fmt::Write;
    let _ = write!(crate::serial::SerialPort, "KERNEL PANIC: {}\n", info);

    let wr = writer::try_get_writer();
    if let Some(wr) = wr {
        wr.fill_rect(0, 0, 3000, 3000, writer::RED); // Red Screen
        wr.set_pos(50, 50);
        let _ = writeln!(wr, "KERNEL PANIC");
        let _ = writeln!(wr, "{}", info);
    }
    loop {}
}

