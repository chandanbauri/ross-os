use crate::idt::InterruptStackFrame;
use crate::kbuf;
use crate::pic;
use crate::writer;

/// IRQ1 handler: read the scancode, push it to the ring buffer, send EOI.
pub extern "x86-interrupt" fn handler(_frame: InterruptStackFrame) {
    let scancode = unsafe { pic::inb(0x60) };
    kbuf::push(scancode);
    unsafe { pic::send_eoi(1); }
}

/// Render the "R.O.S.S. Ready." overlay over the "Starting..." area.
/// Called from the main kernel loop when Enter is detected in the kbuf.
pub fn show_ready() {
    let writer = writer::get_writer();

    // Clear the "Starting..." + progress-bar area
    let clear_y = writer.h / 2 + 20;
    writer.fill_rect(0, clear_y, writer.w, 100, writer::BG);

    let msg   = "R.O.S.S. Ready.";
    let scale = 3;
    let msg_w = (msg.len() * (8 * scale + scale)).saturating_sub(scale);
    let cx    = writer.w / 2;
    writer.set_pos(cx.saturating_sub(msg_w / 2), clear_y + 20);
    writer.put_str(msg, writer::FG, scale);
}
