// Local APIC (xAPIC MMIO mode) — init, periodic timer, EOI, IPI helpers.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub static LAPIC_BASE: AtomicU64  = AtomicU64::new(0xFEE0_0000);
static LAPIC_ENABLED: AtomicBool = AtomicBool::new(false);

// LAPIC register offsets (32-bit registers, 16-byte stride).
const REG_ID:         u32 = 0x020;
const REG_EOI:        u32 = 0x0B0;
const REG_SPURIOUS:   u32 = 0x0F0;
const REG_ICR_LOW:    u32 = 0x300;
const REG_ICR_HIGH:   u32 = 0x310;
const REG_LVT_TIMER:  u32 = 0x320;
const REG_TIMER_INIT: u32 = 0x380;
const REG_TIMER_CURR: u32 = 0x390;
const REG_TIMER_DIV:  u32 = 0x3E0;

#[inline]
fn read(reg: u32) -> u32 {
    let base = LAPIC_BASE.load(Ordering::Relaxed);
    unsafe { ((base + reg as u64) as *const u32).read_volatile() }
}

#[inline]
fn write(reg: u32, val: u32) {
    let base = LAPIC_BASE.load(Ordering::Relaxed);
    unsafe { ((base + reg as u64) as *mut u32).write_volatile(val) }
}

/// Initialise the BSP's LAPIC and calibrate its timer against the PIT.
/// Must be called while the PIC + PIT are still active (needed for calibration).
pub fn init(base: u64) {
    LAPIC_BASE.store(base, Ordering::Relaxed);

    // Enable LAPIC: set spurious vector (0xFF) with enable bit.
    write(REG_SPURIOUS, 0x1FF);

    // ── Calibrate APIC timer against PIT ─────────────────────────────────
    // Divide by 16, one-shot, initial count = max.
    write(REG_TIMER_DIV, 0x3);           // divide by 16
    write(REG_LVT_TIMER, 0x1_0000);      // masked, one-shot
    write(REG_TIMER_INIT, 0xFFFF_FFFF);  // start counting down

    let t0 = crate::pit::ticks();
    // Wait ~10 ms (100 Hz → 1 tick ≈ 10 ms)
    while crate::pit::ticks() == t0 {}
    let t1 = crate::pit::ticks();
    let elapsed_ticks = (t1 - t0) as u32;
    let apic_count = 0xFFFF_FFFFu32.wrapping_sub(read(REG_TIMER_CURR));

    // counts_per_tick = apic_count / elapsed_ticks
    // We want to fire at 100 Hz (one tick = 10 ms = 1 PIT tick at 100 Hz).
    let counts_per_tick = if elapsed_ticks > 0 { apic_count / elapsed_ticks } else { apic_count };

    use core::fmt::Write;
    let _ = write!(crate::serial::SerialPort,
        "[LAPIC] calibrated: {} counts/tick @ divide-by-16\n", counts_per_tick);

    // ── Start periodic timer on vector 32 ─────────────────────────────────
    write(REG_LVT_TIMER, 0x2_0020); // periodic | vector 32
    write(REG_TIMER_DIV, 0x3);      // divide by 16
    write(REG_TIMER_INIT, counts_per_tick);

    LAPIC_ENABLED.store(true, Ordering::Release);
}

/// Per-AP LAPIC init (no calibration — reuse timer, just enable the local unit).
pub fn init_ap(base: u64) {
    LAPIC_BASE.store(base, Ordering::Relaxed);
    write(REG_SPURIOUS, 0x1FF);
    // Start periodic timer with the same vector; the initial count stored in
    // REG_TIMER_INIT is shared MMIO so APs inherit it automatically.
    write(REG_LVT_TIMER, 0x2_0020);
    write(REG_TIMER_DIV, 0x3);
}

/// Acknowledge the current interrupt. Call from the timer ISR.
#[inline(always)]
pub fn eoi() {
    write(REG_EOI, 0);
}

/// Returns true once the BSP has finished LAPIC init.
#[inline]
pub fn is_enabled() -> bool {
    LAPIC_ENABLED.load(Ordering::Acquire)
}

/// Read this CPU's LAPIC ID (bits 31:24 of the ID register).
#[inline]
pub fn my_id() -> u8 {
    (read(REG_ID) >> 24) as u8
}

/// Spin until the ICR delivery status bit clears (IPI accepted by bus).
fn wait_icr_idle() {
    while read(REG_ICR_LOW) & (1 << 12) != 0 {
        core::hint::spin_loop();
    }
}

/// Send INIT IPI to the given APIC ID (level assert).
pub fn send_init(apic_id: u8) {
    write(REG_ICR_HIGH, (apic_id as u32) << 24);
    write(REG_ICR_LOW, 0x0000_4500); // INIT | level assert
    wait_icr_idle();
}

/// Send INIT de-assert IPI (required by MP spec after INIT).
pub fn send_init_deassert() {
    write(REG_ICR_HIGH, 0);
    write(REG_ICR_LOW, 0x0000_8500); // INIT | level deassert | all incl. self
    wait_icr_idle();
}

/// Send SIPI. `vector` is the page (physical = vector << 12; e.g. 0x08 → 0x8000).
pub fn send_sipi(apic_id: u8, vector: u8) {
    write(REG_ICR_HIGH, (apic_id as u32) << 24);
    write(REG_ICR_LOW, 0x0000_4600 | vector as u32);
    wait_icr_idle();
}
