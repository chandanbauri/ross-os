use crate::keyboard;
use crate::pic;
use crate::pit;
use crate::writer;
use core::mem::size_of;

#[repr(C, packed)]
pub struct IdtDescriptor {
    pub limit: u16,
    pub base: u64,
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct IdtEntry {
    pub offset_low: u16,
    pub selector: u16,
    pub ist: u8,
    pub attributes: u8,
    pub offset_mid: u16,
    pub offset_high: u32,
    pub _reserved: u32,
}

impl IdtEntry {
    pub const fn empty() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            ist: 0,
            attributes: 0,
            offset_mid: 0,
            offset_high: 0,
            _reserved: 0,
        }
    }

    pub fn set_handler(&mut self, handler: *const u8) {
        let addr = handler as u64;
        self.offset_low = addr as u16;
        self.offset_mid = (addr >> 16) as u16;
        self.offset_high = (addr >> 32) as u32;
        self.selector = 0x08; // Kernel Code Selector
        self.attributes = 0x8E; // Present, Ring 0, Interrupt Gate
    }
}

pub struct Idt {
    pub entries: [IdtEntry; 256],
}

impl Idt {
    pub const fn new() -> Self {
        Self {
            entries: [IdtEntry::empty(); 256],
        }
    }

    pub fn load(&'static self) {
        let descriptor = IdtDescriptor {
            limit: (size_of::<Self>() - 1) as u16,
            base: self as *const _ as u64,
        };

        unsafe {
            core::arch::asm!("lidt [{0}]", in(reg) &descriptor);
        }
    }
}

#[repr(C)]
pub struct InterruptStackFrame {
    pub instruction_pointer: u64,
    pub code_segment: u64,
    pub cpu_flags: u64,
    pub stack_pointer: u64,
    pub stack_segment: u64,
}

pub fn init_idt(idt: &mut Idt) {
    // CPU exception handlers
    idt.entries[0].set_handler(divide_error_handler as *const u8);
    idt.entries[3].set_handler(breakpoint_handler as *const u8);
    idt.entries[6].set_handler(invalid_opcode_handler as *const u8);
    idt.entries[8].set_handler(double_fault_handler as *const u8);
    idt.entries[13].set_handler(general_protection_fault_handler as *const u8);
    idt.entries[14].set_handler(page_fault_handler as *const u8);

    // PIC IRQ stubs (32-47): prevents GP from unhandled spurious IRQs
    for i in 32usize..=47 {
        idt.entries[i].set_handler(irq_stub_handler as *const u8);
    }
    // IRQ0 (PIT timer) → vector 32
    idt.entries[32].set_handler(timer_handler_stub as *const u8);
    // IRQ1 (PS/2 keyboard) → vector 33
    idt.entries[33].set_handler(keyboard::handler as *const u8);
}

core::arch::global_asm!(
    ".global timer_handler_stub",
    "timer_handler_stub:",
    "push rax", "push rcx", "push rdx", "push rdi", "push rsi",
    "push r8",  "push r9",  "push r10", "push r11",
    "push rbx", "push rbp", "push r12", "push r13", "push r14", "push r15",
    "mov rdi, rsp",
    "call task_timer_handler",
    "mov rsp, rax",
    "pop r15", "pop r14", "pop r13", "pop r12", "pop rbp", "pop rbx",
    "pop r11", "pop r10", "pop r9",  "pop r8",
    "pop rsi", "pop rdi", "pop rdx", "pop rcx", "pop rax",
    "iretq"
);

unsafe extern "C" {
    fn timer_handler_stub();
}

#[unsafe(no_mangle)]
extern "C" fn task_timer_handler(rsp: u64) -> u64 {
    pit::tick();
    let next_rsp = crate::task::SCHEDULER.lock().pick_next(rsp);
    unsafe { pic::send_eoi(0); }
    next_rsp
}

/// PIT timer handler (IRQ0 → vector 32).
// Replaced by timer_handler_stub
/*
extern "x86-interrupt" fn timer_handler(_frame: InterruptStackFrame) {
    pit::tick();
    unsafe { pic::send_eoi(0); }
}
*/

/// Generic stub for all unused hardware IRQs — just sends EOI and returns.
extern "x86-interrupt" fn irq_stub_handler(_frame: InterruptStackFrame) {
    unsafe { pic::send_eoi_both(); }
}

extern "x86-interrupt" fn divide_error_handler(stack_frame: InterruptStackFrame) {
    panic_screen("DIVIDE BY ZERO", &stack_frame);
}

extern "x86-interrupt" fn breakpoint_handler(_stack_frame: InterruptStackFrame) {
    let writer = writer::get_writer();
    writer.put_str("\n[ROSS] BREAKPOINT HIT! System Halting.", 0x00FF0000, 2);
    loop {}
}

extern "x86-interrupt" fn invalid_opcode_handler(stack_frame: InterruptStackFrame) {
    panic_screen("INVALID OPCODE", &stack_frame);
}

extern "x86-interrupt" fn double_fault_handler(stack_frame: InterruptStackFrame, _error_code: u64) -> ! {
    panic_screen("DOUBLE FAULT", &stack_frame);
    loop {}
}

extern "x86-interrupt" fn general_protection_fault_handler(stack_frame: InterruptStackFrame, _error_code: u64) {
    panic_screen("GENERAL PROTECTION FAULT", &stack_frame);
}

extern "x86-interrupt" fn page_fault_handler(stack_frame: InterruptStackFrame, _error_code: u64) {
    panic_screen("PAGE FAULT", &stack_frame);
}

fn panic_screen(name: &str, _stack_frame: &InterruptStackFrame) {
    crate::serial::serial_print("panic_screen TRIGGERED: ");
    crate::serial::serial_print(name);
    crate::serial::serial_print("\n");
    let writer = writer::get_writer();
    writer.fill_rect(0, 0, 10000, 10000, 0x000000FF); // Red Screen
    writer.set_pos(100, 100);
    writer.put_str("!!!!!!!! KERNEL PANIC !!!!!!!!", 0x00FFFFFF, 3);
    writer.set_pos(100, 150);
    writer.put_str("EXCEPTION: ", 0x00FFFFFF, 2);
    writer.put_str(name, 0x00FFFF00, 2);
    writer.set_pos(100, 200);
    writer.put_str("System halted to prevent damage.", 0x00FFFFFF, 2);
    
    // We could print registers here in the future
    loop {}
}
