use core::sync::atomic::{AtomicBool, Ordering};

use crate::idt::InterruptStackFrame;
use crate::kbuf;
use crate::pic;
use crate::writer;

static READY_SHOWN: AtomicBool = AtomicBool::new(false);

/// IRQ1 handler: read scancode, buffer it, and handle Enter immediately.
pub extern "x86-interrupt" fn handler(_frame: InterruptStackFrame) {
    let scancode = unsafe { pic::inb(0x60) };
    kbuf::push(scancode); // buffer for future consumers

    // 0x1C = Enter make-code; handle directly to avoid missing it in the event loop
    if scancode == 0x1C && !READY_SHOWN.load(Ordering::Relaxed) {
        READY_SHOWN.store(true, Ordering::Relaxed);
        show_ready();
    }

    unsafe { pic::send_eoi(1); }
}

/// Replace the "Starting..." overlay with "R.O.S.S. Ready."
pub fn show_ready() {
    let writer = writer::get_writer();

    let clear_y = writer.h / 2 + 20;
    writer.fill_rect(0, clear_y, writer.w, 100, writer::BG);

    let msg   = "R.O.S.S. Ready.";
    let scale = 3;
    let msg_w = (msg.len() * (8 * scale + scale)).saturating_sub(scale);
    let cx    = writer.w / 2;
    writer.set_pos(cx.saturating_sub(msg_w / 2), clear_y + 20);
    writer.put_str(msg, writer::FG, scale);
}
