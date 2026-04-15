use core::sync::atomic::{AtomicU64, Ordering};

/// Global tick counter incremented at 100 Hz by the PIT IRQ0 handler.
static TICKS: AtomicU64 = AtomicU64::new(0);

const PIT_BASE_FREQ: u64 = 1_193_182; // Hz (PIT input oscillator)
const TARGET_HZ:     u64 = 100;
/// Divisor written to PIT channel 0.  1 193 182 / 100 ≈ 11 931 → exactly 100.04 Hz.
const DIVISOR:       u64 = PIT_BASE_FREQ / TARGET_HZ;

/// Programme PIT channel 0 to fire IRQ0 at TARGET_HZ.
/// Must be called before interrupts are enabled.
pub unsafe fn init() {
    unsafe {
        // Mode byte for channel 0:
        //   bits 7-6 = 00  (channel 0)
        //   bits 5-4 = 11  (lo/hi byte access)
        //   bits 3-1 = 010 (mode 2: rate generator)
        //   bit  0   = 0   (binary counter)
        crate::pic::outb(0x43, 0x34);
        crate::pic::outb(0x40, (DIVISOR & 0xFF) as u8);       // low byte
        crate::pic::outb(0x40, ((DIVISOR >> 8) & 0xFF) as u8); // high byte
    }
}

/// Called from the IRQ0 (timer) interrupt handler.
#[inline]
pub fn tick() {
    TICKS.fetch_add(1, Ordering::Relaxed);
}

/// Current tick count since boot.
#[inline]
#[allow(dead_code)]
pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

/// Spin-wait for approximately `ms` milliseconds.
///
/// Resolution: one tick = 10 ms (at 100 Hz).
/// Values below 10 ms round up to one tick.
#[allow(dead_code)]
pub fn sleep_ms(ms: u64) {
    let ticks_needed = ms.div_ceil(1000 / TARGET_HZ); // ceiling divide
    let target = ticks() + ticks_needed;
    while ticks() < target {
        core::hint::spin_loop();
    }
}
