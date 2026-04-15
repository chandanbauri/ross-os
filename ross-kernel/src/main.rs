#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

mod gdt;
mod idt;
mod keyboard;
mod pic;
mod pmm;
mod writer;

use core::panic::PanicInfo;
use ross_common::BootInfo;

static     GDT:          gdt::Gdt = gdt::Gdt::new();
static mut IDT:          idt::Idt = idt::Idt::new();

#[repr(align(16))]
struct KernelStack([u8; 0x4000]); // 16 KB
static KERNEL_STACK: KernelStack = KernelStack([0; 0x4000]);

#[unsafe(no_mangle)]
pub extern "sysv64" fn _start(info: &'static BootInfo) -> ! {
    // ── 1. CPU Fundamentals ─────────────────────────────────────────────────
    unsafe {
        GDT.load();
        core::arch::asm!("mov rsp, {0}", in(reg) KERNEL_STACK.0.as_ptr().add(0x4000));

        writer::init(info);

        let idt_ptr = core::ptr::addr_of_mut!(IDT);
        idt::init_idt(&mut *idt_ptr);
        (*idt_ptr).load();
    }

    // ── 2. Physical Memory Manager ──────────────────────────────────────────
    pmm::init(info.memory_map, info.memory_map_len);

    // ── 3. 8259 PIC + Keyboard ──────────────────────────────────────────────
    unsafe {
        pic::init();

        // Drain any PS/2 scancodes that arrived before the kernel took control
        // (e.g. the Enter pressed to run ./boot.sh is left in the hardware buffer).
        // Status port 0x64 bit 0 = output buffer full; read 0x60 until it's empty.
        while pic::inb(0x64) & 0x01 != 0 {
            pic::inb(0x60); // discard buffered scancode
        }

        core::arch::asm!("sti"); // enable interrupts only after buffer is clean
    }

    // ── 4. Splash Screen ────────────────────────────────────────────────────
    let w  = info.screen_width;
    let h  = info.screen_height;
    let cx = w / 2;
    let wr = writer::get_writer();

    wr.fill_rect(0, 0, w, h, writer::BG);

    // Title
    let title       = "R.O.S.S.";
    let title_scale = 5;
    let title_w     = text_width(title, title_scale);
    let title_y     = h / 2 - 80;
    wr.set_pos(cx.saturating_sub(title_w / 2), title_y);
    wr.put_str(title, writer::FG, title_scale);

    // Separator
    let sep_w = (title_w * 6) / 5;
    let sep_y = title_y + 8 * title_scale + 12;
    wr.fill_rect(cx.saturating_sub(sep_w / 2), sep_y, sep_w, 1, writer::FG);

    // Subtitle
    let sub       = "Rapid Operating System Shell";
    let sub_scale = 2;
    let sub_w     = text_width(sub, sub_scale);
    wr.set_pos(cx.saturating_sub(sub_w / 2), sep_y + 14);
    wr.put_str(sub, writer::DIM, sub_scale);

    // "Starting..."
    let msg       = "Starting...";
    let msg_scale = 2;
    let msg_w     = text_width(msg, msg_scale);
    let msg_y     = h / 2 + 40;
    wr.set_pos(cx.saturating_sub(msg_w / 2), msg_y);
    wr.put_str(msg, writer::FG, msg_scale);

    // Progress bar
    let bar_w = 320_usize.min(w - 80);
    let bar_x = cx.saturating_sub(bar_w / 2);
    let bar_y = msg_y + 8 * msg_scale + 14;
    wr.fill_rect(bar_x,     bar_y, bar_w,         4, writer::DIM);
    wr.fill_rect(bar_x + 1, bar_y, bar_w * 70 / 100, 4, writer::FG);

    // ── 5. Kernel Log (bottom of screen) ────────────────────────────────────
    wr.set_pos(50, h.saturating_sub(90));
    kprintln!("Memory: {} MiB free  ({} regions)", pmm::free_mib(), info.memory_map_len);
    kprintln!("GDT loaded  |  IDT active  |  PIC initialised");
    kprintln!("Press ENTER to continue...");

    loop {
        core::hint::spin_loop();
    }
}

fn text_width(s: &str, scale: usize) -> usize {
    let n = s.len();
    if n == 0 { return 0; }
    n * (8 * scale + scale) - scale
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
