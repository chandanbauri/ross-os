/// Lock-free single-producer / single-consumer ring buffer for PS/2 scancodes.
/// The IRQ1 handler is the sole producer; the kernel main loop is the consumer.

use core::sync::atomic::{AtomicUsize, Ordering};

const BUF_SIZE: usize = 256; // must be a power of two for cheap wrapping

static mut BUFFER: [u8; BUF_SIZE] = [0; BUF_SIZE];
static HEAD: AtomicUsize = AtomicUsize::new(0); // next write index
static TAIL: AtomicUsize = AtomicUsize::new(0); // next read  index

/// Push one scancode byte into the ring buffer.
/// Called from the IRQ1 interrupt handler — must not block.
pub fn push(byte: u8) {
    let head = HEAD.load(Ordering::Relaxed);
    let next = (head + 1) % BUF_SIZE;
    if next != TAIL.load(Ordering::Acquire) {
        // SAFETY: only one interrupt can fire at a time on a single-core system.
        unsafe { BUFFER[head] = byte; }
        HEAD.store(next, Ordering::Release);
    }
    // If the buffer is full the scancode is silently dropped.
}

/// Pop one scancode byte from the ring buffer.
/// Returns `None` if the buffer is empty.
pub fn pop() -> Option<u8> {
    let tail = TAIL.load(Ordering::Relaxed);
    if tail == HEAD.load(Ordering::Acquire) {
        return None; // empty
    }
    let byte = unsafe { BUFFER[tail] };
    TAIL.store((tail + 1) % BUF_SIZE, Ordering::Release);
    Some(byte)
}

/// Convert a PS/2 Set-1 make-code to its ASCII character, if any.
/// Returns `None` for unmapped or release events (bit 7 set).
pub fn scancode_to_ascii(sc: u8) -> Option<u8> {
    if sc & 0x80 != 0 { return None; } // release event

    const MAP: [u8; 58] = [
        0,    0x1B, b'1', b'2', b'3', b'4', b'5', b'6', // 00-07
        b'7', b'8', b'9', b'0', b'-', b'=', 0x08, b'\t', // 08-0F
        b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i',  // 10-17
        b'o', b'p', b'[', b']', b'\n', 0,   b'a', b's',  // 18-1F
        b'd', b'f', b'g', b'h', b'j', b'k', b'l', b';',  // 20-27
        b'\'',b'`', 0,   b'\\',b'z', b'x', b'c', b'v',  // 28-2F
        b'b', b'n', b'm', b',', b'.', b'/', 0,   b'*',   // 30-37
        0,    b' ',                                        // 38-39
    ];

    let idx = sc as usize;
    if idx < MAP.len() && MAP[idx] != 0 { Some(MAP[idx]) } else { None }
}
