#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]

extern crate alloc;

mod gdt;
mod heap;
mod idt;
mod ipc;
mod kbuf;
mod keyboard;
mod ahci;
mod fat32;
mod pci;
mod pic;
mod pit;
mod pmm;
mod paging;
mod shell;
mod task;
mod syscall;
mod vfs;
mod ramfs;
mod writer;
mod elf;
mod serial;

use alloc::vec::Vec;
use core::panic::PanicInfo;
use ross_common::BootInfo;

static mut  GDT:  gdt::Gdt = gdt::Gdt::new();
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
    serial::serial_print("Entered kernel_main\n");
    // ── 1. CPU Fundamentals ─────────────────────────────────────────────────
    unsafe {
        (*core::ptr::addr_of_mut!(GDT)).load();
        serial::serial_print("GDT loaded\n");

        // Zero BSS section while on our own writable stack
        let mut curr = core::ptr::addr_of_mut!(_bss_start);
        let end  = core::ptr::addr_of!(_bss_end);
        while (curr as usize) < (end as usize) {
            core::ptr::write_volatile(curr, 0);
            curr = curr.add(1);
        }
        serial::serial_print("BSS Cleared\n");

        writer::init(info);

        let idt_ptr = core::ptr::addr_of_mut!(IDT);
        idt::init_idt(&mut *idt_ptr);
        (*idt_ptr).load();
        serial::serial_print("IDT loaded\n");
    }


    // ── 2. Physical Memory Manager ──────────────────────────────────────────
    pmm::init(info.memory_map, info.memory_map_len);
    serial::serial_print("PMM initialized\n");

    // ── 3. Virtual Memory (Paging) ──────────────────────────────────────────
    unsafe { paging::init(); }
    serial::serial_print("Paging: Native tables active\n");

    // ── 4. Kernel Heap ──────────────────────────────────────────────────────
    heap::init();

    // ── 4b. PCI Bus Enumeration ─────────────────────────────────────────────
    pci::enumerate();

    // ── 4c. AHCI Storage (best-effort; ignore absence of a SATA drive) ──────
    match ahci::init() {
        Ok(())   => serial::serial_print("[AHCI] Ready\n"),
        Err(msg) => {
            serial::serial_print("[AHCI] Disabled: ");
            serial::serial_print(msg);
            serial::serial_print("\n");
        }
    }

    // ── 5. Shell state reset ────────────────────────────────────────────────
    unsafe {
        let state_ptr = core::ptr::addr_of_mut!(SHELL_STATE);
        *state_ptr = shell::State::Splash;
    }

    // ── 6. Tasking & Scheduler ──────────────────────────────────────────────
    {
        let mut sched = task::SCHEDULER.lock();
        let _cr3: u64;
        unsafe { core::arch::asm!("mov {}, cr3", out(reg) _cr3); }

        sched.set_main(task::Task::main_task());
        
        // Add a simple heartbeat to confirm scheduler is alive
        let cr3: u64;
        unsafe { core::arch::asm!("mov {}, cr3", out(reg) cr3); }
        sched.add_task(task::Task::new(stable_heartbeat as usize, 0, cr3, false));
    }

    // ── 7. System Calls ─────────────────────────────────────────────────────
    unsafe { syscall::init(); }

    // ── 8. Virtual File System ──────────────────────────────────────────────
    {
        if !info.ramdisk_addr.is_null() && info.ramdisk_size > 0 {
            let ramdisk: &'static [u8] = unsafe {
                core::slice::from_raw_parts(info.ramdisk_addr, info.ramdisk_size)
            };
            let ramfs = alloc::sync::Arc::new(ramfs::TarFileSystem::new(ramdisk));
            vfs::init(ramfs);
            serial::serial_print("VFS: Mounted dynamic Initrd\n");

            // Test reading a file
            if let Ok(file) = vfs::open("motd.txt") {
                let mut buf = [0u8; 64];
                if let Ok(n) = file.read(0, &mut buf) {
                    serial::serial_print("VFS Test: read 'motd.txt' -> ");
                    serial::serial_print(core::str::from_utf8(&buf[..n]).unwrap_or("error"));
                    serial::serial_print("\n");
                }
            } else {
                serial::serial_print("VFS Test: failed to open 'motd.txt'\n");
            }
        } else {
            serial::serial_print("VFS: No Initrd passed from bootloader\n");
        }

        // Mount the persistent FAT32 disk at /mnt/disk, if AHCI is ready.
        if ahci::is_ready() {
            match fat32::Fat32Fs::mount() {
                Ok(fs) => {
                    vfs::mount_disk(fs.root());
                    serial::serial_print("VFS: Mounted FAT32 at /mnt/disk\n");
                }
                Err(msg) => {
                    serial::serial_print("VFS: FAT32 mount failed: ");
                    serial::serial_print(msg);
                    serial::serial_print("\n");
                }
            }
        }
    }

    // ── 9. PIC + PIT + Keyboard ─────────────────────────────────────────────
    unsafe {
        pit::init();  // Program PIT to 100 Hz
        pic::init();  // Remap PIC; unmask IRQ0 (timer) and IRQ1 (keyboard)

        // Drain any PS/2 scancodes
        while pic::inb(0x64) & 0x01 != 0 {
            pic::inb(0x60);
        }

        core::arch::asm!("sti"); // Enable interrupts
    }

    // ── 6. Splash Screen ────────────────────────────────────────────────────
    let w  = info.screen_width;
    let h  = info.screen_height;
    let wr = writer::get_writer();

    wr.fill_rect(0, 0, w, h, writer::BG);

    let title       = "R.O.S.S.";
    let title_scale = 1;
    let title_x     = 20;
    let title_y     = 20;
    wr.set_pos(title_x, title_y);
    wr.put_str(title, writer::FG, title_scale);

    let sub       = "Rapid Operating System Shell";
    let sub_scale = 1;
    let sub_y     = title_y + 12;
    wr.set_pos(title_x, sub_y);
    wr.put_str(sub, writer::DIM, sub_scale);

    let msg       = "Starting...";
    let msg_scale = 1;
    let msg_y     = sub_y + 12;
    wr.set_pos(title_x, msg_y);
    wr.put_str(msg, writer::FG, msg_scale);

    // Display MOTD from ramdisk on the framebuffer
    if let Ok(file) = vfs::open("motd.txt") {
        let mut buf = [0u8; 128];
        if let Ok(n) = file.read(0, &mut buf) {
            let motd = core::str::from_utf8(&buf[..n])
                .unwrap_or("")
                .trim_matches('\n')
                .trim_matches('\0')
                .trim();
            wr.set_pos(title_x, msg_y + 16);
            wr.put_str(motd, writer::ACCENT, 1);
        }
    }

    // ── 7. Kernel Log ────────────────────────────────────────────────────────
    wr.set_pos(50, h.saturating_sub(90));
    kprintln!("Memory: {} MiB free  ({} regions)", pmm::free_mib(), info.memory_map_len);
    kprintln!("Paging: CR3 switched to -2GB native tables");
    kprintln!("Press ENTER to continue...");

    // ── 8. Heap smoke-test ──────────────────────────────────────────────────
    {
        let mut v: Vec<u32> = Vec::new();
        for i in 0u32..8 {
            v.push(i * i);
        }
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

extern "C" fn task_a() -> ! {
    let msg = "Hello from Syscall!";
    loop {
        syscall::do_syscall(1, msg.as_ptr() as u64, msg.len() as u64, 0);
        for _ in 0..1_000_000 { unsafe { core::arch::asm!("nop"); } }
    }
}

extern "C" fn task_b() -> ! {
    loop {
        crate::serial::serial_print("B");
        for _ in 0..1_000_000 { unsafe { core::arch::asm!("nop"); } }
    }
}

extern "C" fn stable_heartbeat() -> ! {
    loop {
        crate::serial::serial_print(".");
        for _ in 0..10_000_000 { unsafe { core::arch::asm!("nop"); } }
    }
}
