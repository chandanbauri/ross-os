#![no_std]
#![no_main]

use core::panic::PanicInfo;

// Syscall IDs (Phase 8)
const SYS_WRITE: u64 = 4;
const SYS_EXIT:  u64 = 6;

fn syscall(id: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let ret: u64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") id,
            in("rdi") a1,
            in("rsi") a2,
            in("rdx") a3,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
        );
    }
    ret
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let msg = b"Hello from Phase 8!\n";
    syscall(SYS_WRITE, 1, msg.as_ptr() as u64, msg.len() as u64);
    syscall(SYS_EXIT, 0, 0, 0);
    loop {}
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! { loop {} }
