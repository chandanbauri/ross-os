use core::fmt;
use ross_common::font::FONT_BASIC;
use ross_common::BootInfo;

// Shared colour palette (BGRx little-endian u32)
pub const BG:   u32 = 0x00_80_00_00; // Rich maroon
pub const FG:   u32 = 0x00_FF_FF_FF; // White
pub const DIM:  u32 = 0x00_CC_CC_CC; // Soft grey

static mut WRITER_STORAGE: Option<Writer> = None;

pub struct Writer {
    framebuffer: *mut u32,
    pub w: usize,
    pub h: usize,
    x: usize,
    y: usize,
}

impl Writer {
    fn new(info: &BootInfo) -> Self {
        Self {
            framebuffer: info.framebuffer_ptr as *mut u32,
            w: info.screen_width,
            h: info.screen_height,
            x: 50,
            y: 50,
        }
    }

    pub fn set_pos(&mut self, x: usize, y: usize) {
        self.x = x;
        self.y = y;
    }

    pub fn fill_rect(&self, x: usize, y: usize, w: usize, h: usize, color: u32) {
        for row in y..(y + h).min(self.h) {
            for col in x..(x + w).min(self.w) {
                unsafe { *self.framebuffer.add(row * self.w + col) = color; }
            }
        }
    }

    pub fn put_char(&mut self, ch: u8, color: u32, scale: usize) {
        if ch == b'\n' {
            self.newline(scale);
            return;
        }
        if ch as usize >= 128 { return; }

        let advance = 8 * scale + scale;
        if self.x + advance > self.w { self.newline(scale); }

        let glyph = FONT_BASIC[ch as usize];
        for gy in 0..8 {
            let row = glyph[gy];
            for gx in 0..8 {
                if (row >> (7 - gx)) & 1 == 1 {
                    for sy in 0..scale {
                        for sx in 0..scale {
                            let px = self.x + gx * scale + sx;
                            let py = self.y + gy * scale + sy;
                            if px < self.w && py < self.h {
                                unsafe { *self.framebuffer.add(py * self.w + px) = color; }
                            }
                        }
                    }
                }
            }
        }
        self.x += advance;
    }

    pub fn put_str(&mut self, s: &str, color: u32, scale: usize) {
        for b in s.bytes() {
            self.put_char(b, color, scale);
        }
    }

    fn newline(&mut self, scale: usize) {
        self.x = 50;
        self.y += 8 * scale + 4;
    }
}

/// fmt::Write routes through put_str at scale=2 in FG colour.
impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.put_str(s, FG, 2);
        Ok(())
    }
}

pub fn init(info: &BootInfo) {
    unsafe { WRITER_STORAGE = Some(Writer::new(info)); }
}

pub fn get_writer() -> &'static mut Writer {
    unsafe {
        let ptr = core::ptr::addr_of_mut!(WRITER_STORAGE);
        (*ptr).as_mut().expect("Writer not initialized")
    }
}

// ── Formatting macros ─────────────────────────────────────────────────────────

#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = write!($crate::writer::get_writer(), $($arg)*);
    }};
}

#[macro_export]
macro_rules! kprintln {
    ()             => { $crate::kprint!("\n") };
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = writeln!($crate::writer::get_writer(), $($arg)*);
    }};
}
