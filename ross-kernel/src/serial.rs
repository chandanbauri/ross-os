use core::arch::asm;
use core::fmt;

pub struct SerialPort;

impl SerialPort {
    pub fn write_byte(&self, byte: u8) {
        unsafe {
            while (Self::inb(0x3F8 + 5) & 0x20) == 0 {}
            Self::outb(0x3F8, byte);
        }
    }
    
    unsafe fn inb(port: u16) -> u8 {
        let mut ret: u8;
        asm!("in al, dx", out("al") ret, in("dx") port, options(nomem, nostack, preserves_flags));
        ret
    }
    unsafe fn outb(port: u16, val: u8) {
        asm!("out dx, al", in("al") val, in("dx") port, options(nomem, nostack, preserves_flags));
    }
}

impl fmt::Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            self.write_byte(byte);
        }
        Ok(())
    }
}

pub fn serial_print(s: &str) {
    use core::fmt::Write;
    let _ = SerialPort.write_str(s);
}

pub fn serial_print_byte(b: u8) {
    SerialPort.write_byte(b);
}
