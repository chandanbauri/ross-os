use crate::idt::InterruptStackFrame;
use crate::pic;
use crate::writer;

// PS/2 Set-1 US QWERTY scancode → ASCII byte (0 = no mapping)
const SCANCODE_MAP: [u8; 58] = [
    0,    0x1B, b'1', b'2', b'3', b'4', b'5', b'6', // 00-07
    b'7', b'8', b'9', b'0', b'-', b'=', 0x08, b'\t', // 08-0F
    b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i',  // 10-17
    b'o', b'p', b'[', b']', b'\n', 0,   b'a', b's',  // 18-1F
    b'd', b'f', b'g', b'h', b'j', b'k', b'l', b';',  // 20-27
    b'\'',b'`', 0,   b'\\',b'z', b'x', b'c', b'v',  // 28-2F
    b'b', b'n', b'm', b',', b'.', b'/', 0,   b'*',   // 30-37
    0,    b' ',                                        // 38-39
];

pub extern "x86-interrupt" fn handler(_frame: InterruptStackFrame) {
    let scancode = unsafe { pic::inb(0x60) };

    // Bit 7 set = key-release event; skip it
    if scancode & 0x80 == 0 {
        let sc = scancode as usize;
        if sc < SCANCODE_MAP.len() && SCANCODE_MAP[sc] == b'\n' {
            show_ready();
        }
    }

    unsafe { pic::send_eoi(1); }
}

fn show_ready() {
    let writer = writer::get_writer();

    // Clear the "Starting..." + progress-bar area
    let clear_y = writer.h / 2 + 20;
    writer.fill_rect(0, clear_y, writer.w, 100, writer::BG);

    // Print "R.O.S.S. Ready."
    let msg   = "R.O.S.S. Ready.";
    let scale = 3;
    let msg_w = (msg.len() * (8 * scale + scale)).saturating_sub(scale);
    let cx    = writer.w / 2;
    writer.set_pos(cx.saturating_sub(msg_w / 2), clear_y + 20);
    writer.put_str(msg, writer::FG, scale);
}
