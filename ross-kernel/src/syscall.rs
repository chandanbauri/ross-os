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
        let efer = msr::read_msr(msr::EFER_MSR);
        msr::write_msr(msr::EFER_MSR, efer | 1);
        msr::write_msr(msr::LSTAR_MSR, syscall_handler_stub as *const () as u64);
        let star = (0x08u64 << 32) | (0x10u64 << 48);
        msr::write_msr(msr::STAR_MSR, star);
        msr::write_msr(msr::SFMASK_MSR, 0x200); // mask IF
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Syscall table (Phase 8)
//
//  1  sys_log    (ptr, len, –)          → 0        serial debug
//  2  sys_uptime (–, –, –)              → ticks    PIT ticks
//  3  sys_pipe   (–, –, –)              → fd       create IPC pipe, return fd
//  4  sys_write  (fd, ptr, len)         → n        fd 1=stdout 2=stderr …
//  5  sys_read   (fd, ptr, len)         → n        fd 0=stdin …
//  6  sys_exit   (code, –, –)           → !        mark task Dead
//  7  sys_brk    (addr, –, –)           → addr     0 = query, else extend
//  8  sys_mmap   (len, –, –)            → addr     anonymous alloc
//  9  sys_open   (path_ptr, flags, –)   → fd
// 10  sys_close  (fd, –, –)             → 0
// ─────────────────────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
extern "C" fn syscall_dispatch(id: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    match id {
        // ── 1: sys_log ────────────────────────────────────────────────────────
        1 => {
            let ptr = arg1 as *const u8;
            let len = arg2 as usize;
            let msg = unsafe {
                core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
            };
            crate::serial::serial_print("[SYSCALL LOG] ");
            crate::serial::serial_print(msg);
            crate::serial::serial_print("\n");
            0
        }

        // ── 2: sys_uptime ─────────────────────────────────────────────────────
        2 => crate::pit::ticks(),

        // ── 3: sys_pipe ───────────────────────────────────────────────────────
        3 => {
            let pipe_id = match crate::ipc::create() {
                Some(id) => id,
                None => return u64::MAX,
            };
            let mut sched = crate::task::SCHEDULER.lock();
            if let Some(task) = sched.current_task.as_mut() {
                task.fd_table.alloc(crate::fd::FdEntry::Pipe(pipe_id))
                    .map(|fd| fd as u64)
                    .unwrap_or(u64::MAX)
            } else {
                u64::MAX
            }
        }

        // ── 4: sys_write(fd, ptr, len) ────────────────────────────────────────
        4 => {
            let fd  = arg1 as usize;
            let ptr = arg2 as *const u8;
            let len = arg3 as usize;
            if len == 0 { return 0; }

            let data = unsafe { core::slice::from_raw_parts(ptr, len) };

            match fd {
                1 => { // stdout → framebuffer
                    let wr = crate::writer::get_writer();
                    for &b in data {
                        if b == b'\n' {
                            wr.put_char(b'\n', crate::writer::FG, 1);
                        } else if b.is_ascii_graphic() || b == b' ' {
                            wr.put_char(b, crate::writer::FG, 1);
                        }
                    }
                    len as u64
                }
                2 => { // stderr → serial
                    for &b in data {
                        crate::serial::serial_print_byte(b);
                    }
                    len as u64
                }
                _ => {
                    let pipe_id = {
                        let sched = crate::task::SCHEDULER.lock();
                        sched.current_task.as_ref().and_then(|t| {
                            match t.fd_table.get(fd) {
                                Some(crate::fd::FdEntry::Pipe(id)) => Some(*id),
                                _ => None,
                            }
                        })
                    };
                    if let Some(id) = pipe_id {
                        crate::ipc::write(id, data).map(|n| n as u64).unwrap_or(u64::MAX)
                    } else {
                        u64::MAX
                    }
                }
            }
        }

        // ── 5: sys_read(fd, ptr, len) ─────────────────────────────────────────
        5 => {
            let fd  = arg1 as usize;
            let ptr = arg2 as *mut u8;
            let len = arg3 as usize;
            if len == 0 { return 0; }

            match fd {
                0 => { // stdin ← keyboard ring buffer
                    let buf = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
                    let mut n = 0;
                    while n < buf.len() {
                        match crate::kbuf::pop() {
                            Some(b) => { buf[n] = b; n += 1; }
                            None    => break,
                        }
                    }
                    n as u64
                }
                _ => {
                    // Pull info out of fd_table while holding lock briefly.
                    enum ReadTarget { Pipe(usize), File(alloc::sync::Arc<dyn crate::vfs::VfsNode>, usize) }
                    let target = {
                        let sched = crate::task::SCHEDULER.lock();
                        sched.current_task.as_ref().and_then(|t| {
                            match t.fd_table.get(fd) {
                                Some(crate::fd::FdEntry::Pipe(id)) => Some(ReadTarget::Pipe(*id)),
                                Some(crate::fd::FdEntry::VfsFile(node, off)) =>
                                    Some(ReadTarget::File(node.clone(), *off)),
                                _ => None,
                            }
                        })
                    };

                    match target {
                        Some(ReadTarget::Pipe(id)) => {
                            let buf = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
                            crate::ipc::read(id, buf).map(|n| n as u64).unwrap_or(u64::MAX)
                        }
                        Some(ReadTarget::File(node, offset)) => {
                            let buf = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
                            match node.read(offset, buf) {
                                Ok(n) => {
                                    // Update offset in fd_table.
                                    let mut sched = crate::task::SCHEDULER.lock();
                                    if let Some(task) = sched.current_task.as_mut() {
                                        if let Some(entry) = task.fd_table.get_mut(fd) {
                                            if let crate::fd::FdEntry::VfsFile(_, off) = entry {
                                                *off += n;
                                            }
                                        }
                                    }
                                    n as u64
                                }
                                Err(_) => u64::MAX,
                            }
                        }
                        None => u64::MAX,
                    }
                }
            }
        }

        // ── 6: sys_exit(code) ─────────────────────────────────────────────────
        6 => {
            use core::fmt::Write;
            let mut serial = crate::serial::SerialPort;
            let _ = writeln!(serial, "[SYSCALL] sys_exit({})", arg1);
            let mut sched = crate::task::SCHEDULER.lock();
            if let Some(task) = sched.current_task.as_mut() {
                task.state = crate::task::TaskState::Dead;
            }
            // The timer ISR will call pick_next and drop this task.
            // sysretq returns to the (dead) user binary which loops until next tick.
            0
        }

        // ── 7: sys_brk(addr) ──────────────────────────────────────────────────
        7 => {
            let new_brk = arg1;
            let mut sched = crate::task::SCHEDULER.lock();
            if let Some(task) = sched.current_task.as_mut() {
                if new_brk == 0 {
                    // Query current break.
                    return task.heap_end;
                }
                if new_brk > task.heap_end {
                    // Extend: map pages from heap_end up to new_brk.
                    let cr3 = task.cr3;
                    let mut addr = task.heap_end;
                    while addr < new_brk {
                        if let Some(phys) = crate::pmm::alloc_page() {
                            let kptr = crate::paging::phys_to_virt(phys) as *mut u8;
                            unsafe { core::ptr::write_bytes(kptr, 0, 4096); }
                            crate::paging::map_user_page(cr3, addr, phys as u64, 0x7);
                            addr += 4096;
                        } else {
                            break; // OOM — return what we managed to map
                        }
                    }
                    task.heap_end = addr;
                }
                task.heap_end
            } else {
                0
            }
        }

        // ── 8: sys_mmap(len) ──────────────────────────────────────────────────
        // Phase 8: anonymous-only. Allocates `len` bytes after heap_end.
        8 => {
            let len = arg1 as usize;
            if len == 0 { return u64::MAX; }
            let pages = (len + 4095) / 4096;

            let mut sched = crate::task::SCHEDULER.lock();
            if let Some(task) = sched.current_task.as_mut() {
                let base = task.heap_end;
                let cr3  = task.cr3;
                drop(sched); // release lock before calling alloc_anon

                match crate::mmap::alloc_anon(cr3, base, pages) {
                    Ok(()) => {
                        // Re-lock to update heap_end.
                        let mut sched = crate::task::SCHEDULER.lock();
                        if let Some(task) = sched.current_task.as_mut() {
                            task.heap_end = base + pages as u64 * 4096;
                        }
                        base
                    }
                    Err(_) => u64::MAX,
                }
            } else {
                u64::MAX
            }
        }

        // ── 9: sys_open(path_ptr, flags) ──────────────────────────────────────
        9 => {
            let path_ptr = arg1 as *const u8;
            // Safely copy the null-terminated path from user memory.
            let path = {
                let mut s = alloc::string::String::new();
                let mut p = path_ptr;
                loop {
                    let b = unsafe { p.read() };
                    if b == 0 || s.len() >= 512 { break; }
                    s.push(b as char);
                    p = unsafe { p.add(1) };
                }
                s
            };

            match crate::vfs::open(&path) {
                Ok(node) => {
                    let mut sched = crate::task::SCHEDULER.lock();
                    if let Some(task) = sched.current_task.as_mut() {
                        task.fd_table.alloc(crate::fd::FdEntry::VfsFile(node, 0))
                            .map(|fd| fd as u64)
                            .unwrap_or(u64::MAX)
                    } else {
                        u64::MAX
                    }
                }
                Err(_) => u64::MAX,
            }
        }

        // ── 10: sys_close(fd) ─────────────────────────────────────────────────
        10 => {
            let fd = arg1 as usize;
            let mut sched = crate::task::SCHEDULER.lock();
            if let Some(task) = sched.current_task.as_mut() {
                if task.fd_table.close(fd) { 0 } else { u64::MAX }
            } else {
                u64::MAX
            }
        }

        _ => u64::MAX,
    }
}

core::arch::global_asm!(
    ".global syscall_handler_stub",
    "syscall_handler_stub:",
    // Debug: send 'S' to COM1.
    "push rax", "push rdx",
    "mov al, 0x53", "mov dx, 0x3F8", "out dx, al",
    "pop rdx", "pop rax",

    "push r11",  // save RFLAGS
    "push rcx",  // save return RIP

    "push rbp", "push rbx", "push r12", "push r13", "push r14", "push r15",

    // syscall_dispatch(id=rax, arg1=rdi, arg2=rsi, arg3=rdx) — System V ABI
    "mov rcx, rdx",
    "mov rdx, rsi",
    "mov rsi, rdi",
    "mov rdi, rax",
    "call syscall_dispatch",

    "pop r15", "pop r14", "pop r13", "pop r12", "pop rbx", "pop rbp",

    "pop rcx",   // restore RIP
    "pop r11",   // restore RFLAGS
    "sysretq"
);

unsafe extern "C" { fn syscall_handler_stub(); }
