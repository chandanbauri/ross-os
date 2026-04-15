/// Write a byte to an x86 I/O port.
#[inline]
pub unsafe fn outb(port: u16, val: u8) {
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") port,
            in("al") val,
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// Read a byte from an x86 I/O port.
#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    unsafe {
        core::arch::asm!(
            "in al, dx",
            out("al") val,
            in("dx") port,
            options(nomem, nostack, preserves_flags)
        );
    }
    val
}

/// Small I/O delay — write to unused port 0x80 (POST diagnostic port).
#[inline]
unsafe fn io_wait() {
    unsafe { outb(0x80, 0x00); }
}

const PIC1_CMD:  u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_CMD:  u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;
const PIC_EOI:   u8  = 0x20;

/// Remap both 8259A PICs; unmask only IRQ1 (PS/2 keyboard).
pub unsafe fn init() {
    unsafe {
        // ICW1: cascade, ICW4 needed
        outb(PIC1_CMD,  0x11); io_wait();
        outb(PIC2_CMD,  0x11); io_wait();

        // ICW2: vector offsets after remap
        outb(PIC1_DATA, 0x20); io_wait(); // IRQ 0-7  → vectors 32-39
        outb(PIC2_DATA, 0x28); io_wait(); // IRQ 8-15 → vectors 40-47

        // ICW3: cascade wiring
        outb(PIC1_DATA, 0x04); io_wait(); // master: slave on IRQ2
        outb(PIC2_DATA, 0x02); io_wait(); // slave:  cascade identity

        // ICW4: 8086 mode, normal EOI
        outb(PIC1_DATA, 0x01); io_wait();
        outb(PIC2_DATA, 0x01); io_wait();

        // OCW1: mask all IRQs except IRQ1 (keyboard)
        outb(PIC1_DATA, 0xFD); // 1111_1101 → IRQ1 unmasked
        outb(PIC2_DATA, 0xFF); // all slave IRQs masked
    }
}

/// Signal EOI for a given IRQ line.
pub unsafe fn send_eoi(irq: u8) {
    unsafe {
        if irq >= 8 { outb(PIC2_CMD, PIC_EOI); }
        outb(PIC1_CMD, PIC_EOI);
    }
}

/// Send EOI to both PICs (used for spurious IRQ stubs).
pub unsafe fn send_eoi_both() {
    unsafe {
        outb(PIC2_CMD, PIC_EOI);
        outb(PIC1_CMD, PIC_EOI);
    }
}
