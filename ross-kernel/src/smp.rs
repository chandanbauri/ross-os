// SMP: AP trampoline + bringup via INIT-SIPI-SIPI sequence.
//
// The trampoline is copied to physical address 0x8000 (SIPI vector 0x08).
// A data block at 0x8F00 is written by the BSP before each SIPI and read by
// the AP during the 16→32→64-bit transition.
//
// DATA BLOCK at 0x8F00:
//   +0x00  u64   ap_cr3        (BSP CR3 — shared page tables)
//   +0x08  u64   ap_stack_top  (unique kernel stack top for this AP)
//   +0x10  u64   ap_entry      (virtual address of ap_main)
//   +0x20  u16   mini_gdt_limit   (= 0x1F)
//   +0x22  u32   mini_gdt_base    (= 0x8F40)
//   +0x28  u16   kgdt_limit       (kernel GDT limit)
//   +0x2A  u64   kgdt_base        (kernel GDT virtual address)
//   +0x40  8×u8  null descriptor
//   +0x48  8×u8  code32 descriptor
//   +0x50  8×u8  data32 descriptor
//   +0x58  8×u8  code64 descriptor
//
// Memory note: 0x8000 is within the lower-half 4-GB identity map so the
// BSP can write to it directly (virt == phys for addresses < 4 GB).

use core::sync::atomic::{AtomicUsize, Ordering};

pub static AP_ONLINE: AtomicUsize = AtomicUsize::new(0);

// ── AP kernel stack pool ─────────────────────────────────────────────────────
// 16 KB per AP, statically allocated so we don't depend on the heap being
// initialised during very early AP startup.
const AP_STACK_SIZE: usize = 16 * 1024;
const MAX_APS: usize = 8;

#[repr(align(16))]
struct ApStack([u8; AP_STACK_SIZE]);
static mut AP_STACKS: [ApStack; MAX_APS] = [const { ApStack([0u8; AP_STACK_SIZE]) }; MAX_APS];

// ── Trampoline code ──────────────────────────────────────────────────────────
// Assembled at link-time VMA but copied to phys 0x8000 at runtime.
// All intra-trampoline address refs use (label - ap_trampoline_start + 0x8000).
core::arch::global_asm!(
    ".intel_syntax noprefix",

    ".global ap_trampoline_start",
    ".global ap_trampoline_end",

    // ── 16-bit real-mode entry ────────────────────────────────────────────
    ".code16",
    "ap_trampoline_start:",
    "cli",
    "cld",
    "xor ax, ax",
    "mov ds, ax",
    "mov es, ax",
    "mov ss, ax",

    // Load mini-GDT (6-byte descriptor: u16 limit, u32 base at 0x8F20)
    "lgdt word ptr [0x8F20]",

    // Enable protected mode (CR0.PE)
    "mov eax, cr0",
    "or  eax, 1",
    "mov cr0, eax",

    // Far jump to flush pipeline and switch to 32-bit PM.
    // Selector 0x08 = code32 in mini-GDT; target = runtime address of pm32 section.
    ".byte 0x66",   // operand-size override → 32-bit offset in the far jump
    ".byte 0xEA",   // far jmp opcode
    ".long (ap_trampoline_pm32 - ap_trampoline_start + 0x8000)",
    ".word 0x08",   // selector: code32

    // ── 32-bit protected-mode ──────────────────────────────────────────────
    ".align 4",
    ".code32",
    "ap_trampoline_pm32:",
    "mov ax, 0x10",  // data32 selector
    "mov ds, ax",
    "mov es, ax",
    "mov ss, ax",

    // Set up a minimal temporary stack for the push/lretq trick below.
    // 0x8FF0 is within the trampoline page and well above our data block.
    "mov esp, 0x8FF0",

    // Enable PAE (required for 4-level paging)
    "mov eax, cr4",
    "or  eax, 0x20",
    "mov cr4, eax",

    // Load CR3 from data block (lower 32 bits of BSP CR3)
    "mov eax, dword ptr [0x8F00]",
    "mov cr3, eax",

    // Enable EFER.LME (long mode enable)
    "mov ecx, 0xC0000080",
    "rdmsr",
    "or  eax, 0x100",
    "wrmsr",

    // Enable paging (CR0.PG) — activates long mode compatibility
    "mov eax, cr0",
    "or  eax, 0x80000001",
    "mov cr0, eax",

    // Far jump to 64-bit code via code64 segment (selector 0x18 in mini-GDT)
    ".byte 0xEA",
    ".long (ap_trampoline_lm64 - ap_trampoline_start + 0x8000)",
    ".word 0x18",

    // ── 64-bit long mode ──────────────────────────────────────────────────
    ".align 8",
    ".code64",
    "ap_trampoline_lm64:",
    // Clear data segments
    "xor ax, ax",
    "mov ds, ax",
    "mov es, ax",
    "mov ss, ax",

    // Load the kernel's real GDT (10-byte descriptor: u16+u64 at 0x8F28)
    "lgdt [0x8F28]",

    // Reload CS to kernel code selector (0x08) via far-return trick.
    // Stack layout for lretq: [rsp+0]=RIP, [rsp+8]=CS
    "push 0x08",                                    // new CS
    "lea  rax, [rip + ap_trampoline_after_cs]",
    "push rax",                                     // new RIP
    ".byte 0x48, 0xCB",                             // REX.W LRET (64-bit far return)

    "ap_trampoline_after_cs:",
    // Set up AP kernel stack
    "mov rsp, [0x8F08]",
    "xor rbp, rbp",

    // Call ap_main (address in data block at 0x8F10)
    "mov rax, [0x8F10]",
    "call rax",

    // Should never return; halt just in case.
    "2: hlt",
    "jmp 2b",

    "ap_trampoline_end:",
    ".att_syntax prefix",
);

unsafe extern "C" {
    static ap_trampoline_start: u8;
    static ap_trampoline_end:   u8;
}

// ── Data-block helpers (all writes go to the identity-mapped page at 0x8000) ──

unsafe fn db_write_u32(offset: u64, val: u32) {
    ((0x8F00u64 + offset) as *mut u32).write_volatile(val);
}
unsafe fn db_write_u64(offset: u64, val: u64) {
    ((0x8F00u64 + offset) as *mut u64).write_volatile(val);
}
unsafe fn db_write_u16(offset: u64, val: u16) {
    ((0x8F00u64 + offset) as *mut u16).write_volatile(val);
}

/// Copy trampoline code to 0x8000 and populate the fixed data block.
unsafe fn install_trampoline(stack_top: u64, cr3: u64) {
    // Copy code
    let src_start = core::ptr::addr_of!(ap_trampoline_start);
    let src_end   = core::ptr::addr_of!(ap_trampoline_end);
    let len = src_end as usize - src_start as usize;
    core::ptr::copy_nonoverlapping(src_start, 0x8000 as *mut u8, len);

    // Data block at 0x8F00
    db_write_u64(0x00, cr3);        // ap_cr3
    db_write_u64(0x08, stack_top);  // ap_stack_top
    db_write_u64(0x10, ap_main as u64); // ap_entry

    // Mini-GDT descriptor at 0x8F20: limit=0x1F (4 descriptors × 8 - 1), base=0x8F40
    db_write_u16(0x20, 0x1F);
    db_write_u32(0x22, 0x8F40);

    // Kernel GDT descriptor at 0x8F28
    let kgdt_ptr = core::ptr::addr_of!(crate::GDT) as u64;
    let kgdt_limit = (core::mem::size_of::<crate::gdt::Gdt>() - 1) as u16;
    db_write_u16(0x28, kgdt_limit);
    db_write_u64(0x2A, kgdt_ptr);

    // Mini-GDT entries at 0x8F40:
    //   [0] null
    //   [1] 0x08 code32: G=1, D/B=1, P=1, DPL=0, code, execute+read
    //   [2] 0x10 data32: G=1, D/B=1, P=1, DPL=0, data, read+write
    //   [3] 0x18 code64: L=1, P=1, DPL=0, code, execute+read
    db_write_u64(0x40, 0x0000_0000_0000_0000u64); // null
    db_write_u64(0x48, 0x00CF_9A00_0000_FFFFu64); // code32
    db_write_u64(0x50, 0x00CF_9200_0000_FFFFu64); // data32
    db_write_u64(0x58, 0x0020_9A00_0000_0000u64); // code64
}

/// AP Rust entry point — called from the trampoline.
unsafe extern "C" fn ap_main() -> ! {
    // Reload segment registers expected by kernel
    unsafe {
        core::arch::asm!(
            "mov ax, 0x10",
            "mov ds, ax",
            "mov es, ax",
            "mov ss, ax",
            out("ax") _,
        );
    }

    // Initialise this AP's LAPIC.
    let base = crate::lapic::LAPIC_BASE.load(core::sync::atomic::Ordering::Relaxed);
    crate::lapic::init_ap(base);

    let id = crate::lapic::my_id();
    use core::fmt::Write;
    let _ = write!(crate::serial::SerialPort, "[SMP] AP {} Online\n", id);

    AP_ONLINE.fetch_add(1, Ordering::SeqCst);

    // Enable interrupts and idle
    unsafe { core::arch::asm!("sti"); }
    loop { core::arch::asm!("hlt"); }
}

/// Small spin-delay (counts approximately `us` microseconds at ~1 GHz).
fn spin_us(us: u64) {
    let count = us * 1000; // ~1000 nop loops per µs at ~1 GHz
    for _ in 0..count { unsafe { core::arch::asm!("nop"); } }
}

/// Wake up all APs listed in `ap_ids`, skipping the BSP (`bsp_id`).
/// Must be called after LAPIC is initialised.
pub fn init(ap_ids: &[u8], bsp_id: u8) {
    let cr3: u64;
    unsafe { core::arch::asm!("mov {}, cr3", out(reg) cr3); }

    let mut ap_num: usize = 0;

    for &apic_id in ap_ids {
        if apic_id == bsp_id { continue; }
        if ap_num >= MAX_APS {
            crate::serial::serial_print("[SMP] Too many APs — skipping\n");
            break;
        }

        let stack_top = unsafe {
            let base = core::ptr::addr_of_mut!(AP_STACKS[ap_num].0) as u64;
            base + AP_STACK_SIZE as u64
        };

        unsafe { install_trampoline(stack_top, cr3); }

        use core::fmt::Write;
        let _ = write!(crate::serial::SerialPort,
            "[SMP] Waking AP {} (stack top {:#x})\n", apic_id, stack_top);

        // INIT-SIPI-SIPI sequence per Intel MP spec.
        crate::lapic::send_init(apic_id);
        spin_us(10_000); // wait 10 ms

        crate::lapic::send_sipi(apic_id, 0x08); // SIPI: vector 0x08 → phys 0x8000
        spin_us(200);

        crate::lapic::send_sipi(apic_id, 0x08); // second SIPI
        spin_us(200);

        ap_num += 1;
    }

    // Wait up to ~100 ms for all APs to check in.
    let expected = ap_num;
    let mut waited = 0u32;
    while AP_ONLINE.load(Ordering::SeqCst) < expected && waited < 10_000_000 {
        core::hint::spin_loop();
        waited += 1;
    }

    use core::fmt::Write;
    let _ = write!(crate::serial::SerialPort,
        "[SMP] {}/{} APs online\n", AP_ONLINE.load(Ordering::SeqCst), expected);
}
