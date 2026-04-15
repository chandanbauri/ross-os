use core::mem::size_of;

#[repr(C, packed)]
pub struct GdtDescriptor {
    pub limit: u16,
    pub base: u64,
}

#[repr(C, align(8))]
pub struct Gdt {
    pub entries: [u64; 6],
}

#[allow(dead_code)] pub const KERNEL_CODE_SELECTOR: u16 = 1 << 3;
#[allow(dead_code)] pub const KERNEL_DATA_SELECTOR: u16 = 2 << 3;
#[allow(dead_code)] pub const USER_DATA_SELECTOR:   u16 = 3 << 3;
#[allow(dead_code)] pub const USER_CODE_SELECTOR:   u16 = 4 << 3;
#[allow(dead_code)] pub const COMPAT_CODE_SELECTOR: u16 = 5 << 3;

impl Gdt {
    pub const fn new() -> Self {
        let mut entries = [0u64; 6];
        entries[0] = 0;
        entries[1] = create_gdt_entry(0, 0, 0x9A, 0x2); // Kernel Code 64-bit
        entries[2] = create_gdt_entry(0, 0, 0x92, 0x0); // Kernel Data
        entries[3] = create_gdt_entry(0, 0, 0xF2, 0x0); // User Data
        entries[4] = create_gdt_entry(0, 0, 0xFA, 0x2); // User Code 64-bit
        entries[5] = create_gdt_entry(0, 0xFFFFF, 0x9A, 0x4); // Compat Code 32-bit
        Self { entries }
    }

    pub fn load(&'static self) {
        let descriptor = GdtDescriptor {
            limit: (size_of::<Self>() - 1) as u16,
            base: self as *const _ as u64,
        };

        unsafe {
            core::arch::asm!(
                // Load the new GDT
                "lgdt [{desc}]",
                // Reload data segment registers using explicit 16-bit ax register.
                // Using mov ds, r64 is not portable; use ax (16-bit) explicitly.
                "mov ax, 0x10",
                "mov ds, ax",
                "mov es, ax",
                "mov fs, ax",
                "mov gs, ax",
                "mov ss, ax",
                // Reload CS via a far return.
                // Stack after two pushes: [rsp] = RIP, [rsp+8] = CS
                "push 0x08",
                "lea rax, [2f + rip]",
                "push rax",
                "retfq",
                "2:",
                desc = in(reg) &descriptor,
                out("rax") _,   // rax is clobbered by lea+push
                // ax is low 16 of rax, already declared above
            );
        }
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
