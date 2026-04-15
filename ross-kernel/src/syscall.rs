pub mod msr {
    use core::arch::asm;

    pub const EFER_MSR:   u32 = 0xC0000080;
    pub const STAR_MSR:   u32 = 0xC0000081;
    pub const LSTAR_MSR:  u32 = 0xC0000082;
    pub const SFMASK_MSR: u32 = 0xC0000084;

    #[inline]
    pub unsafe fn write_msr(msr: u32, value: u64) {
        let low  = (value & 0xFFFFFFFF) as u32;
        let high = (value >> 32) as u32;
        asm!("wrmsr", in("ecx") msr, in("eax") low, in("edx") high);
    }

    #[inline]
    pub unsafe fn read_msr(msr: u32) -> u64 {
        let low: u32;
        let high: u32;
        asm!("rdmsr", in("ecx") msr, out("eax") low, out("edx") high);
        ((high as u64) << 32) | (low as u64)
    }
}

pub unsafe fn init() {
    unsafe {
        // 1. Enable syscall/sysret in EFER
        let efer = msr::read_msr(msr::EFER_MSR);
        msr::write_msr(msr::EFER_MSR, efer | 1); // Bit 0 is SCE (System Call Enable)

        // 2. Set LSTAR to our assembly entry point
        msr::write_msr(msr::LSTAR_MSR, syscall_handler_stub as *const () as u64);

        // 3. Set STAR (segments)
        // Kernel: STAR[47:32] = 0x08 (Code). SS will be 0x10 (Data).
        // User:   STAR[63:48] = 0x10 (Data-base). 
        //         SS will be (0x10 + 8) | 3 = 0x1B? 
        //         Wait, user data is at 0x18 (Entry 3).
        //         User code is at 0x20 (Entry 4).
        // So base should be 0x10. 
        // SS = 0x10 + 8 = 0x18 (Entry 3).
        // CS = 0x10 + 16 = 0x20 (Entry 4).
        
        let kernel_base = 0x08u64;
        let user_base   = 0x10u64; 
        
        let star = (kernel_base << 32) | (user_base << 48);
        msr::write_msr(msr::STAR_MSR, star);

        // 4. Set SFMASK (flags to clear on syscall)
        msr::write_msr(msr::SFMASK_MSR, 0x200); // Mask IF (bit 9)
    }
}

pub fn do_syscall(id: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    let ret: u64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") id,
            in("rdi") arg1,
            in("rsi") arg2,
            in("rdx") arg3,
            out("rcx") _, // clobbered by syscall (RIP)
            out("r11") _, // clobbered by syscall (RFLAGS)
            lateout("rax") ret,
        );
    }
    ret
}

#[unsafe(no_mangle)]
extern "C" fn syscall_dispatch(id: u64, arg1: u64, arg2: u64, _arg3: u64) -> u64 {
    match id {
        1 => {
            // Log message
            let ptr = arg1 as *const u8;
            let len = arg2 as usize;
            let msg = unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len)) };
            crate::serial::serial_print("[SYSCALL LOG] ");
            crate::serial::serial_print(msg);
            crate::serial::serial_print("\n");
            0
        }
        2 => {
            // Uptime
            crate::pit::ticks()
        }
        _ => 0xFFFFFFFFFFFFFFFF, // Error
    }
}

core::arch::global_asm!(
    ".global syscall_handler_stub",
    "syscall_handler_stub:",
    // When 'syscall' is called, RIP is in RCX and RFLAGS is in R11.
    // RSP is NOT switched.
    // We must manually switch to a kernel stack if we were in user land.
    // For now, since we are in kernel land, we just save registers.
    "push r11", // Save flags
    "push rcx", // Save return address
    
    "push rbp", "push rbx", "push r12", "push r13", "push r14", "push r15",
    
    // Call Rust dispatcher
    // rdi = rax (id), rsi = rdi (arg1), rdx = rsi (arg2), rcx = rdx (arg3)
    "mov rcx, rdx",
    "mov rdx, rsi",
    "mov rsi, rdi",
    "mov rdi, rax",
    "call syscall_dispatch",
    
    "pop r15", "pop r14", "pop r13", "pop r12", "pop rbx", "pop rbp",
    
    "pop rcx", // Restore RIP
    "pop r11", // Restore RFLAGS
    // "sysretq" // Temporary comment out
    "sti",
    "jmp rcx"
);

unsafe extern "C" {
    fn syscall_handler_stub();
}
