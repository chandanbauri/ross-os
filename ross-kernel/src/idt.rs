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
    idt.entries[8].ist = 1; // Use IST1 for stable DF handling
    idt.entries[13].set_handler(general_protection_fault_handler as *const u8);
    idt.entries[14].set_handler(page_fault_handler as *const u8);

    // PIC IRQ stubs (32-47): prevents GP from unhandled spurious IRQs
    for i in 32usize..=47 {
        idt.entries[i].set_handler(irq_stub_handler as *const u8);
    }
    // IRQ0 (PIT timer) / APIC timer → vector 32
    idt.entries[32].set_handler(timer_handler_stub as *const u8);
    // IRQ1 (PS/2 keyboard) → vector 33
    idt.entries[33].set_handler(keyboard::handler as *const u8);
    // APIC spurious vector → 0xFF
    idt.entries[0xFF].set_handler(lapic_spurious_handler as *const u8);
}

extern "x86-interrupt" fn lapic_spurious_handler(_frame: InterruptStackFrame) {
    // Spurious LAPIC interrupt — no EOI needed per spec.
}

#[unsafe(no_mangle)]
extern "C" fn task_timer_handler(rsp: u64) -> u128 {
    pit::tick();
    let res = crate::task::SCHEDULER.lock().pick_next(rsp);

    // Acknowledge interrupt: use LAPIC EOI when LAPIC is active, else PIC EOI.
    if crate::lapic::is_enabled() {
        crate::lapic::eoi();
    } else {
        unsafe { pic::send_eoi(0); }
    }

    // Combine RSP and CR3 into a single u128 for register return (RAX/RDX)
    (res.cr3 as u128) << 64 | (res.rsp as u128)
}

core::arch::global_asm!(
    ".global timer_handler_stub",
    "timer_handler_stub:",
    // Save registers
    "push rax", "push rcx", "push rdx", "push rbx",
    "push rbp", "push rsi", "push rdi",
    "push r8",  "push r9",  "push r10", "push r11",
    "push r12", "push r13", "push r14", "push r15",

    // Call scheduler
    "mov rdi, rsp",
    "call task_timer_handler",

    // RAX = new RSP, RDX = new CR3
    "mov rsp, rax",
    "mov rcx, cr3",
    "cmp rcx, rdx",
    "je 1f",
    "mov cr3, rdx",
    "1:",

    // Restore registers
    "pop r15", "pop r14", "pop r13", "pop r12",
    "pop r11", "pop r10", "pop r9",  "pop r8",
    "pop rdi", "pop rsi", "pop rbp", "pop rbx",
    "pop rdx", "pop rcx",
    // Before restoring RAX, peek at the saved CS in the iretq frame.
    // Layout after pop rcx: [rsp+0]=rax, [rsp+8]=RIP, [rsp+16]=CS.
    // If CPL==3 we must set DS/ES/FS/GS to the user data selector (0x1B)
    // because iretq does NOT restore data segments.
    "mov eax, [rsp + 16]",
    "and eax, 3",
    "cmp eax, 3",
    "jne 2f",
    "mov ax, 0x1b",
    "mov ds, ax",
    "mov es, ax",
    "mov fs, ax",
    "mov gs, ax",
    "2:",
    "pop rax",
    "iretq"
);

unsafe extern "C" {
    fn timer_handler_stub();
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
    panic_screen("DIVIDE BY ZERO", &stack_frame, 0);
}

extern "x86-interrupt" fn breakpoint_handler(_stack_frame: InterruptStackFrame) {
    let writer = writer::get_writer();
    writer.put_str("\n[ROSS] BREAKPOINT HIT! System Halting.", 0x00FF0000, 2);
    loop {}
}

extern "x86-interrupt" fn invalid_opcode_handler(stack_frame: InterruptStackFrame) {
    panic_screen("INVALID OPCODE", &stack_frame, 0);
}

extern "x86-interrupt" fn double_fault_handler(stack_frame: InterruptStackFrame, error_code: u64) -> ! {
    panic_screen("DOUBLE FAULT", &stack_frame, error_code);
    loop {}
}

extern "x86-interrupt" fn general_protection_fault_handler(stack_frame: InterruptStackFrame, error_code: u64) {
    panic_screen("GENERAL PROTECTION FAULT", &stack_frame, error_code);
}

extern "x86-interrupt" fn page_fault_handler(stack_frame: InterruptStackFrame, error_code: u64) {
    panic_screen("PAGE FAULT", &stack_frame, error_code);
}

fn panic_screen(name: &str, stack_frame: &InterruptStackFrame, error_code: u64) {
    let cr2: u64;
    unsafe { core::arch::asm!("mov {}, cr2", out(reg) cr2); }

    let mut serial = crate::serial::SerialPort;
    use core::fmt::Write;
    let _ = writeln!(serial, "\n!!!!!!!! KERNEL PANIC: {} !!!!!!!!", name);
    let _ = writeln!(serial, "  RIP:   0x{:016x}", stack_frame.instruction_pointer);
    let _ = writeln!(serial, "  CS:    0x{:02x}", stack_frame.code_segment);
    let _ = writeln!(serial, "  CR2:   0x{:016x}", cr2);
    let _ = writeln!(serial, "  ERROR: 0x{:x}", error_code);
    let _ = writeln!(serial, "  RSP:   0x{:016x}\n", stack_frame.stack_pointer);

    let writer = writer::get_writer();
    writer.fill_rect(0, 0, 10000, 10000, 0x000000FF); // Red Screen
    writer.set_pos(100, 100);
    writer.put_str("!!!!!!!! KERNEL PANIC !!!!!!!!", 0x00FFFFFF, 2);
    writer.set_pos(100, 150);
    let _ = write!(writer, "EXCEPTION: {} (RIP: 0x{:x})", name, stack_frame.instruction_pointer);
    writer.set_pos(100, 200);
    let _ = write!(writer, "CR2: 0x{:x}  ERROR: 0x{:x}", cr2, error_code);
    
    loop {}
}
