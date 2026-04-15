use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskId(usize);

#[repr(C)]
pub struct FullContext {
    // Pushed by us
    pub r15: u64, pub r14: u64, pub r13: u64, pub r12: u64,
    pub rbp: u64, pub rbx: u64, pub r11: u64, pub r10: u64,
    pub r9:  u64, pub r8:  u64, pub rsi: u64, pub rdi: u64,
    pub rdx: u64, pub rcx: u64, pub rax: u64,
    
    // Pushed by CPU
    pub rip: u64,
    pub cs:  u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss:  u64,
}

pub struct Task {
    pub id: TaskId,
    pub stack: Vec<u8>,
    pub rsp: u64,
    pub state: TaskState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Ready,
    Running,
    Blocked,
}

impl Task {
    pub fn new(entry_point: usize, stack_size: usize) -> Self {
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
        
        let stack_top = stack.as_ptr() as usize + stack_size;
        let mut rsp = stack_top as *mut u64;

        unsafe {
            rsp = rsp.offset(-1); rsp.write(0x10); // SS
            rsp = rsp.offset(-1); rsp.write(stack_top as u64); // RSP
            rsp = rsp.offset(-1); rsp.write(0x202); // RFLAGS
            rsp = rsp.offset(-1); rsp.write(0x08); // CS
            rsp = rsp.offset(-1); rsp.write(entry_point as u64); // RIP
            
            // 15 registers
            for _ in 0..15 {
                rsp = rsp.offset(-1);
                rsp.write(0);
            }
        }

        Task {
            id,
            stack,
            rsp: rsp as u64,
            state: TaskState::Ready,
        }
    }

    /// Create a dummy task for the already-running main kernel execution.
    pub fn main_task() -> Self {
        Task {
            id: TaskId(NEXT_ID.fetch_add(1, Ordering::SeqCst)),
            stack: Vec::new(), // Not used for main task
            rsp: 0,            // Will be set on first switch
            state: TaskState::Running,
        }
    }
}

use alloc::collections::VecDeque;
use spin::Mutex;
use lazy_static::lazy_static;

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

    pub fn pick_next(&mut self, current_rsp: u64) -> u64 {
        if let Some(mut current) = self.current_task.take() {
            current.rsp = current_rsp;
            current.state = TaskState::Ready;
            if current.stack.len() > 0 { // Don't re-queue main if it's special? No, we can re-queue it.
                self.tasks.push_back(current);
            } else {
                // Special handling for the very first main task if it has no stack vec
                self.tasks.push_back(current);
            }
        }

        if let Some(mut next) = self.tasks.pop_front() {
            next.state = TaskState::Running;
            let next_rsp = next.rsp;
            self.current_task = Some(next);
            next_rsp
        } else {
            current_rsp // No other tasks, continue current
        }
    }
}
