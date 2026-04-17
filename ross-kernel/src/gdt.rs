use core::mem::size_of;

#[repr(C, packed)]
pub struct GdtDescriptor {
    pub limit: u16,
    pub base: u64,
}

#[repr(C, packed)]
pub struct Tss {
    reserved0: u32,
    pub rsp0: u64,
    pub rsp1: u64,
    pub rsp2: u64,
    reserved1: u64,
    pub ist1: u64,
    pub ist2: u64,
    pub ist3: u64,
    pub ist4: u64,
    pub ist5: u64,
    pub ist6: u64,
    pub ist7: u64,
    reserved2: u64,
    reserved3: u16,
    pub iopb_offset: u16,
}

impl Tss {
    pub const fn new() -> Self {
        Self {
            reserved0: 0,
            rsp0: 0,
            rsp1: 0,
            rsp2: 0,
            reserved1: 0,
            ist1: 0, ist2: 0, ist3: 0, ist4: 0, ist5: 0, ist6: 0, ist7: 0,
            reserved2: 0,
            reserved3: 0,
            iopb_offset: size_of::<Self>() as u16,
        }
    }
}

pub static mut TSS: Tss = Tss::new();
static mut DOUBLE_FAULT_STACK: [u8; 0x4000] = [0; 0x4000];

#[repr(C, align(16))]
pub struct Gdt {
    pub entries: [u64; 8],
}

#[allow(dead_code)] pub const KERNEL_CODE_SELECTOR: u16 = 1 << 3;
#[allow(dead_code)] pub const KERNEL_DATA_SELECTOR: u16 = 2 << 3;
#[allow(dead_code)] pub const USER_DATA_SELECTOR:   u16 = 3 << 3;
#[allow(dead_code)] pub const USER_CODE_SELECTOR:   u16 = 4 << 3;
pub const TSS_SELECTOR:             u16 = 5 << 3;

impl Gdt {
    pub const fn new() -> Self {
        let mut entries = [0u64; 8];
        entries[0] = 0;
        entries[1] = create_gdt_entry(0, 0, 0x9A, 0x2); // Kernel Code
        entries[2] = create_gdt_entry(0, 0, 0x92, 0x0); // Kernel Data
        entries[3] = create_gdt_entry(0, 0, 0xF2, 0x0); // User Data
        entries[4] = create_gdt_entry(0, 0, 0xFA, 0x2); // User Code
        
        // entries[5] and entries[6] will be TSS (set in load)
        
        Self { entries }
    }

    pub fn load(&'static mut self) {
        // Setup TSS entry (16 bytes)
        let tss_addr = unsafe { core::ptr::addr_of!(TSS) as u64 };
        let tss_limit = (size_of::<Tss>() - 1) as u64;
        
        // Setup IST1 for Double Faults
        unsafe {
            TSS.ist1 = core::ptr::addr_of!(DOUBLE_FAULT_STACK) as u64 + 0x4000;
        }

        self.entries[5] = (tss_limit & 0xFFFF)
                        | ((tss_addr & 0xFFFFFF) << 16)
                        | (0x89u64 << 40) // Access: Present, TSS type
                        | (((tss_limit >> 16) & 0xF) << 48)
                        | (((tss_addr >> 24) & 0xFF) << 56);
        self.entries[6] = tss_addr >> 32;

        let descriptor = GdtDescriptor {
            limit: (size_of::<Self>() - 1) as u16,
            base: self as *const _ as u64,
        };

        unsafe {
            core::arch::asm!(
                "lgdt [{desc}]",
                "mov ax, 0x10",
                "mov ds, ax", "mov es, ax", "mov fs, ax", "mov gs, ax", "mov ss, ax",
                "push 0x08",
                "lea rax, [2f + rip]",
                "push rax",
                "retfq",
                "2:",
                "mov ax, {tss_sel}",
                "ltr ax",
                desc = in(reg) &descriptor,
                tss_sel = const TSS_SELECTOR,
                out("rax") _,
            );
        }
    }
}

pub fn set_tss_stack(stack: u64) {
    unsafe {
        TSS.rsp0 = stack;
    }
}

const fn create_gdt_entry(base: u32, limit: u32, access: u8, flags: u8) -> u64 {
    let mut entry: u64 = 0;
    entry |= (limit & 0xFFFF) as u64;
    entry |= ((base & 0xFFFF) as u64) << 16;
    entry |= (((base >> 16) & 0xFF) as u64) << 32;
    entry |= (access as u64) << 40;
    entry |= (((limit >> 16) & 0x0F) as u64) << 48;
    entry |= ((flags & 0x0F) as u64) << 52;
    entry |= (((base >> 24) & 0xFF) as u64) << 56;
    entry
}
