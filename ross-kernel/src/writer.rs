use core::fmt;
use ross_common::font::FONT_BASIC;
use ross_common::BootInfo;

// ── Colour palette (BGRx little-endian u32) ──────────────────────────────────
pub const BG:     u32 = 0x00_18_07_02; // Very dark maroon
pub const FG:     u32 = 0x00_FF_FF_FF; // White
pub const DIM:    u32 = 0x00_99_99_99; // Soft grey
pub const ACCENT: u32 = 0x00_00_FF_99; // Teal/Mint — prompt & success
pub const RED:    u32 = 0x00_44_44_FF; // Error (BGR: full red channel)
pub const YELLOW: u32 = 0x00_00_CC_FF; // Warning (BGR: yellow)

// Left margin used by the shell — must match Writer::newline()
pub const LEFT_MARGIN: usize = 20;

static mut WRITER_STORAGE: Option<Writer> = None;

pub struct Writer {
    framebuffer:  *mut u32,
    pub w:        usize,
    pub h:        usize,
    pub x:        usize,
    pub y:        usize,
    /// Horizontal position of the first editable character on this line.
    /// Backspace will not go further left than this.
    pub input_x:  usize,
}

impl Writer {
    fn new(info: &BootInfo) -> Self {
        Self {
            framebuffer: info.framebuffer_ptr as *mut u32,
            w:  info.screen_width,
            h:  info.screen_height,
            x:  LEFT_MARGIN,
            y:  50,
            input_x: LEFT_MARGIN,
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
        if ch == b'\n' { self.newline(scale); return; }
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
        for b in s.bytes() { self.put_char(b, color, scale); }
    }

    /// Erase the last typed character (same-line, does not cross the prompt).
    pub fn backspace(&mut self, scale: usize) {
        let char_w = 8 * scale + scale;
        if self.x >= self.input_x + char_w {
            self.x -= char_w;
            self.fill_rect(self.x, self.y, char_w, 8 * scale, BG);
        }
    }

    pub fn newline(&mut self, scale: usize) {
        self.x = LEFT_MARGIN;
        self.y += 8 * scale + 4;
        if self.y + 8 * scale >= self.h {
            // Simple wrap: reset to a safe zone (no scroll yet)
            self.y = self.h / 2 + 60;
            self.fill_rect(0, self.y, self.w, self.h - self.y, BG);
        }
    }

    /// Record the current x as the start of user-editable input on this line.
    pub fn mark_input_start(&mut self) {
        self.input_x = self.x;
    }
}

/// fmt::Write → kprint!/kprintln! macros (scale 2, FG colour).
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

pub fn try_get_writer() -> Option<&'static mut Writer> {
    unsafe {
        let ptr = core::ptr::addr_of_mut!(WRITER_STORAGE);
        (*ptr).as_mut()
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
    ()            => { $crate::kprint!("\n") };
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = writeln!($crate::writer::get_writer(), $($arg)*);
    }};
}
