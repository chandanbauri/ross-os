#![no_std]
#![no_main]

use core::panic::PanicInfo;
use ross_common::BootInfo;
use ross_common::font::FONT_BASIC;

// ── Palette ───────────────────────────────────────────────────────────────────
// BGRx u32 little-endian: byte0=Blue, byte1=Green, byte2=Red, byte3=0
const BG:    u32 = 0x00_80_00_00;
const FG:    u32 = 0x00_FF_FF_FF;
const DIM:   u32 = 0x00_CC_CC_CC;
const LINE:  u32 = 0x00_FF_FF_FF;

// ── Rendering primitives ──────────────────────────────────────────────────────

/// Fill a rectangle with a solid color.
#[inline]
fn fill_rect(fb: *mut u32, stride: usize, x: usize, y: usize, w: usize, h: usize, color: u32) {
    for row in y..y + h {
        for col in x..x + w {
            unsafe { *fb.add(row * stride + col) = color; }
        }
    }
}

/// Draw one character from the 8×8 bitmap font, scaled by `scale`.
fn put_char(fb: *mut u32, stride: usize, x: usize, y: usize, ch: u8, color: u32, scale: usize) {
    if ch as usize >= 128 { return; }
    let glyph = FONT_BASIC[ch as usize];
    for gy in 0..8 {
        let row = glyph[gy];
        for gx in 0..8 {
            if (row >> (7 - gx)) & 1 == 1 {
                fill_rect(fb, stride, x + gx * scale, y + gy * scale, scale, scale, color);
            }
        }
    }
}

/// Draw a string left-to-right and return the x position after the last char.
fn put_str(fb: *mut u32, stride: usize, x: usize, y: usize, s: &str, color: u32, scale: usize) {
    let advance = 8 * scale + scale;
    let mut cx = x;
    for b in s.bytes() {
        put_char(fb, stride, cx, y, b, color, scale);
        cx += advance;
    }
}

/// Return the pixel width of a string at the given scale.
fn text_width(s: &str, scale: usize) -> usize {
    let n = s.len();
    if n == 0 { return 0; }
    let advance = 8 * scale + scale;
    n * advance - scale // no trailing gap
}

// ── Entry point ───────────────────────────────────────────────────────────────
#[unsafe(no_mangle)]
pub extern "sysv64" fn _start(info: &'static BootInfo) -> ! {
    let w  = info.screen_width;
    let h  = info.screen_height;
    let fb = info.framebuffer_ptr as *mut u32;
    let cx = w / 2;

    fill_rect(fb, w, 0, 0, w, h, BG);


    let title       = "R.O.S.S.";
    let title_scale = 5;
    let title_w     = text_width(title, title_scale);
    let title_y     = h / 2 - 80;
    put_str(fb, w, cx - title_w / 2, title_y, title, FG, title_scale);

    let sep_w = (title_w * 6) / 5;
    let sep_y = title_y + 8 * title_scale + 12;
    fill_rect(fb, w, cx - sep_w / 2, sep_y, sep_w, 1, LINE);

    let sub       = "Rapid Operating System Shell";
    let sub_scale = 2;
    let sub_w     = text_width(sub, sub_scale);
    let sub_y     = sep_y + 14;
    put_str(fb, w, cx - sub_w / 2, sub_y, sub, DIM, sub_scale);

    let msg       = "Starting...";
    let msg_scale = 2;
    let msg_w     = text_width(msg, msg_scale);
    let msg_y     = h / 2 + 40;
    put_str(fb, w, cx - msg_w / 2, msg_y, msg, FG, msg_scale);

    let bar_w = 320_usize.min(w - 80);
    let bar_h = 4;
    let bar_x = cx - bar_w / 2;
    let bar_y = msg_y + 8 * msg_scale + 14;
    fill_rect(fb, w, bar_x,     bar_y, bar_w,         bar_h, DIM);
    fill_rect(fb, w, bar_x + 1, bar_y, bar_w * 70 / 100, bar_h, FG);

    // 8. Halt — nothing more to do yet
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {}
}
