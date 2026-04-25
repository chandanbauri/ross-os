use crate::elf;
use crate::paging;
use crate::pmm;
use crate::vfs;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskId(usize);

#[repr(C)]
pub struct TaskSwitchResult {
    pub rsp: u64,
    pub cr3: u64,
}

#[repr(C)]
pub struct FullContext {
    // Pushed by us
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rax: u64,

    // Pushed by CPU
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

pub struct Task {
    pub id: TaskId,
    pub stack: Vec<u8>,
    pub kernel_stack_top: u64,
    pub rsp: u64,
    pub cr3: u64,
    pub state: TaskState,
    pub is_user: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Ready,
    Running,
    Blocked,
}

impl Task {
    pub fn new(entry_point: usize, stack_top: u64, cr3: u64, is_user: bool) -> Self {
        let stack_size = 0x2000; // Small kernel stack for context storage
        let mut stack = Vec::with_capacity(stack_size);
        stack.extend(core::iter::repeat(0).take(stack_size));

        let id = TaskId(NEXT_ID.fetch_add(1, Ordering::SeqCst));

        // Initial stack layout for 'iretq'
        // [ss] 0x10 (GDT data)
        // [rsp] stack_top
        // [rflags] 0x202
        // [cs] 0x08 (GDT code)
        // [rip] entry_point
        // [rax..r15] 0

        let internal_stack_top = stack.as_ptr() as usize + stack_size;
        let mut rsp = internal_stack_top as *mut u64;

        let cs = if is_user { 0x23 } else { 0x08 };
        let ss = if is_user { 0x1B } else { 0x10 };

        unsafe {
            rsp = rsp.offset(-1);
            rsp.write(ss); // SS
            rsp = rsp.offset(-1);
            rsp.write(if stack_top == 0 {
                internal_stack_top as u64
            } else {
                stack_top
            }); // RSP
            rsp = rsp.offset(-1);
            rsp.write(0x202); // RFLAGS
            rsp = rsp.offset(-1);
            rsp.write(cs); // CS
            rsp = rsp.offset(-1);
            rsp.write(entry_point as u64); // RIP

            // 15 registers
            for _ in 0..15 {
                rsp = rsp.offset(-1);
                rsp.write(0);
            }
        }

        Task {
            id,
            stack,
            kernel_stack_top: internal_stack_top as u64,
            rsp: rsp as u64,
            cr3,
            state: TaskState::Ready,
            is_user,
        }
    }

    /// Create a dummy task for the already-running main kernel execution.
    pub fn main_task() -> Self {
        let cr3: u64;
        unsafe {
            core::arch::asm!("mov {}, cr3", out(reg) cr3);
        }

        Task {
            id: TaskId(NEXT_ID.fetch_add(1, Ordering::SeqCst)),
            stack: Vec::new(),
            kernel_stack_top: 0, // Main task uses the stack from _start
            rsp: 0,
            cr3,
            state: TaskState::Running,
            is_user: false,
        }
    }
}

use alloc::collections::VecDeque;
use lazy_static::lazy_static;
use spin::Mutex;

lazy_static! {
    pub static ref SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());
}

pub struct Scheduler {
    pub tasks: VecDeque<Task>,
    pub current_task: Option<Task>,
}

impl Scheduler {
    fn new() -> Self {
        Scheduler {
            tasks: VecDeque::new(),
            current_task: None,
        }
    }

    pub fn set_main(&mut self, task: Task) {
        self.current_task = Some(task);
    }

    pub fn add_task(&mut self, task: Task) {
        self.tasks.push_back(task);
    }

    pub fn pick_next(&mut self, current_rsp: u64) -> TaskSwitchResult {
        if let Some(mut current) = self.current_task.take() {
            current.rsp = current_rsp;
            current.state = TaskState::Ready;
            self.tasks.push_back(current);
        }

        if let Some(mut next) = self.tasks.pop_front() {
            next.state = TaskState::Running;
            let next_rsp = next.rsp;
            let next_cr3 = next.cr3;

            if next.is_user {
                crate::gdt::set_tss_stack(next.kernel_stack_top);
            }

            self.current_task = Some(next);
            TaskSwitchResult {
                rsp: next_rsp,
                cr3: next_cr3,
            }
        } else {
            let cr3: u64;
            unsafe {
                core::arch::asm!("mov {}, cr3", out(reg) cr3);
            }
            TaskSwitchResult {
                rsp: current_rsp,
                cr3,
            }
        }
    }
}

pub fn spawn_process(path: &str) -> Result<(), ()> {
    crate::serial::serial_print("[EXEC] Spawning process: ");
    crate::serial::serial_print(path);
    crate::serial::serial_print("\n");

    let file = vfs::open(path)?;
    let stat = file.attribute();
    let mut data = alloc::vec![0u8; stat.size];
    file.read(0, &mut data)?;

    let header_ptr = data.as_ptr() as *const elf::ElfHeader;
    let header = unsafe { &*header_ptr };
    if !header.is_valid() {
        crate::serial::serial_print("[EXEC] Error: Invalid ELF header\n");
        return Err(());
    }

    let cr3 = paging::create_user_address_space();
    crate::serial::serial_print("[EXEC] Created user address space (CR3)\n");

    // Map PT_LOAD segments
    for (i, ph) in header.program_headers(&data).iter().enumerate() {
        if ph.p_type == elf::PT_LOAD {
            let start_vaddr = ph.p_vaddr;
            let mem_size = ph.p_memsz as usize;
            let file_size = ph.p_filesz as usize;

            // Calculate page-aligned boundaries
            let page_start = start_vaddr & !0xFFF;
            let page_end = (start_vaddr + mem_size as u64 + 4095) & !0xFFF;

            let mut current_page = page_start;
            while current_page < page_end {
                // If a previous PT_LOAD segment already mapped this page, reuse
                // its physical frame instead of allocating a new one and zeroing
                // it (which would destroy the already-copied code/data).
                let (phys, already_mapped) = match paging::lookup_page(cr3, current_page) {
                    Some(p) => (p, true),
                    None    => (pmm::alloc_page().ok_or(())?, false),
                };

                let page_ptr = paging::phys_to_virt(phys) as *mut u8;

                if !already_mapped {
                    // Translate ELF p_flags → page flags.
                    // ELF: PF_X=0x1, PF_W=0x2, PF_R=0x4
                    // x86: Present=0x1, Writable=0x2, User=0x4
                    // Always set Present+User; set Writable only when ELF W bit is set.
                    let page_flags = 0x1 | 0x4 | if ph.p_flags & 0x2 != 0 { 0x2 } else { 0 };
                    paging::map_user_page(cr3, current_page, phys as u64, page_flags);
                    unsafe { core::ptr::write_bytes(page_ptr, 0, 4096); }
                }

                // Copy data from file if within range
                // A single page might overlap with the file segment
                let page_offset = if current_page < start_vaddr {
                    (start_vaddr - current_page) as usize
                } else {
                    0
                };

                let vaddr_in_page = if current_page < start_vaddr {
                    start_vaddr
                } else {
                    current_page
                };
                let file_offset_for_page = (vaddr_in_page - start_vaddr) as usize;

                if file_offset_for_page < file_size {
                    let bytes_to_copy =
                        core::cmp::min(file_size - file_offset_for_page, 4096 - page_offset);

                    unsafe {
                        let src = data
                            .as_ptr()
                            .add(ph.p_offset as usize + file_offset_for_page);
                        let dest = page_ptr.add(page_offset);
                        core::ptr::copy_nonoverlapping(src, dest, bytes_to_copy);
                    }
                }

                current_page += 4096;
            }

            use core::fmt::Write;
            let mut serial = crate::serial::SerialPort;
            let _ = writeln!(
                serial,
                "[EXEC] Mapped Segment {}: 0x{:x} -> 0x{:x}",
                i,
                ph.p_vaddr,
                ph.p_vaddr + ph.p_memsz
            );
        }
    }

    // Allocate and map user stack at 0x0000_7000_0000_0000
    // Use a lower stack address for stability (Check #2)
    let stack_vaddr = 0x0000_0000_0050_0000u64;
    let stack_pages = 8;
    for i in 0..stack_pages {
        let phys = pmm::alloc_page().ok_or(())?;
        paging::map_user_page(cr3, stack_vaddr + i * 4096, phys as u64, 0x2 | 0x4);
    }
    let stack_top = (stack_vaddr + stack_pages * 4096) & !0xF;

    // Enqueue the user task.  The preemptive timer will schedule it.
    let user_task = Task::new(header.e_entry as usize, stack_top, cr3, true);
    SCHEDULER.lock().add_task(user_task);

    use core::fmt::Write;
    let mut serial = crate::serial::SerialPort;
    let _ = writeln!(serial, "[EXEC] User task queued — entry=0x{:x} stack=0x{:x}",
                     header.e_entry, stack_top);
    Ok(())
}
