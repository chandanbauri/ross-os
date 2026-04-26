/// ROSS Shell — Phase 4
///
/// Implements:
///   • Two-state machine: Splash → Active
///   • Line editing: printable chars, Backspace, Enter
///   • Command dispatch: help, clear, memory, uptime, version
///   • Coloured output via writer colour constants

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;
use crate::{pit, pmm, writer};

const MAX_LINE: usize = 128;
const MAX_CWD:  usize = 64;
const SCALE:    usize = 1;

/// Y-coordinate where the terminal area begins.
pub const TERM_Y: usize = 60;

static mut LINE_BUF: [u8; MAX_LINE] = [0u8; MAX_LINE];
static mut LINE_LEN: usize = 0;

// Current working directory — default to the persistent FAT32 disk.
static mut CWD_BUF: [u8; MAX_CWD] = {
    let mut b = [0u8; MAX_CWD];
    b[0] = b'/'; b[1] = b'm'; b[2] = b'n'; b[3] = b't';
    b[4] = b'/'; b[5] = b'd'; b[6] = b'i'; b[7] = b's'; b[8] = b'k';
    b
};
static mut CWD_LEN: usize = 9; // "/mnt/disk"

/// Resolve a path: if it starts with '/' use as-is; otherwise prepend cwd.
fn resolve(path: &str) -> String {
    if path.starts_with('/') {
        String::from(path)
    } else {
        let cwd = unsafe { core::str::from_utf8_unchecked(&CWD_BUF[..CWD_LEN]) };
        let mut s = String::from(cwd);
        if !s.ends_with('/') { s.push('/'); }
        s.push_str(path);
        s
    }
}

// ── State machine ─────────────────────────────────────────────────────────────

pub enum State {
    /// Waiting for the first Enter to leave the splash screen.
    Splash,
    /// Shell is active; process commands.
    Active,
}

/// Process one decoded byte from the keyboard ring buffer.
pub fn handle_byte(state: &mut State, ch: u8) {
    let wr = writer::get_writer();
    match state {
        State::Splash => {
            if ch == b'\n' {
                *state = State::Active;
                activate(wr);
            }
        }
        State::Active => match ch {
            b'\n' => {
                // Execute current line
                wr.put_char(b'\n', writer::FG, SCALE);
                let len = unsafe { LINE_LEN };
                let raw = unsafe { &LINE_BUF[..len] };
                let cmd = core::str::from_utf8(raw).unwrap_or("").trim();
                execute(cmd, wr);
                // Reset line buffer
                unsafe { LINE_LEN = 0; }
                prompt(wr);
            }
            0x08 => {
                // Backspace
                if unsafe { LINE_LEN > 0 } {
                    unsafe { LINE_LEN -= 1; }
                    wr.backspace(SCALE);
                }
            }
            b' '..=b'~' => {
                // Printable ASCII
                let len = unsafe { LINE_LEN };
                if len < MAX_LINE {
                    unsafe {
                        LINE_BUF[len] = ch;
                        LINE_LEN += 1;
                    }
                    wr.put_char(ch, writer::FG, SCALE);
                }
            }
            _ => {}
        },
    }
}

// ── Splash → Shell transition ─────────────────────────────────────────────────

fn activate(wr: &mut writer::Writer) {
    let w = wr.w;
    let h = wr.h;

    // Replace "Starting..." with "R.O.S.S. Ready."
    let ready_y = 44;
    wr.fill_rect(20, ready_y, 200, 10, writer::BG);
    let msg   = "R.O.S.S. Ready.";
    wr.set_pos(20, ready_y);
    wr.put_str(msg, writer::ACCENT, SCALE);

    // Draw terminal separator
    wr.fill_rect(0, TERM_Y - 2, w, 2, 0x00_33_33_33);

    // Clear terminal area
    wr.fill_rect(0, TERM_Y, w, h - TERM_Y, writer::BG);

    // Welcome banner
    wr.set_pos(writer::LEFT_MARGIN, TERM_Y + 6);
    wr.put_str("ROSS Shell v0.4  |  Type 'help' for commands", writer::DIM, 1);
    wr.set_pos(writer::LEFT_MARGIN, TERM_Y + 20);

    prompt(wr);
}

fn prompt(wr: &mut writer::Writer) {
    wr.put_str("ross", writer::ACCENT, SCALE);
    wr.put_str("> ", writer::DIM, SCALE);
    wr.mark_input_start();
}

// ── Command dispatch ──────────────────────────────────────────────────────────

fn execute(input: &str, wr: &mut writer::Writer) {
    // Definitive trace for debugging reboots
    crate::serial::serial_print("[SHELL] Executing: ");
    crate::serial::serial_print(input);
    crate::serial::serial_print("\n");

    let mut parts = input.split_whitespace();
    match parts.next().unwrap_or("") {
        "help"    => cmd_help(wr),
        "clear"   => cmd_clear(wr),
        "memory"  => cmd_memory(wr),
        "uptime"  => cmd_uptime(wr),
        "version" => cmd_version(wr),
        "pwd"     => cmd_pwd(wr),
        "cd"      => cmd_cd(parts.next().unwrap_or("/mnt/disk"), wr),
        "ls"      => {
            let raw = parts.next().unwrap_or("");
            let path = if raw.is_empty() {
                unsafe { core::str::from_utf8_unchecked(&CWD_BUF[..CWD_LEN]) }.into()
            } else {
                resolve(raw)
            };
            cmd_ls(&path, wr);
        }
        "lspci"   => cmd_lspci(wr),
        "disk"    => cmd_disk(wr),
        "cat"     => {
            if let Some(raw) = parts.next() { cmd_cat(&resolve(raw), wr); }
            else { wr.put_str("  Usage: cat <path>\n", writer::DIM, SCALE); }
        }
        "write"   => {
            let path = parts.next();
            let data: String = parts.collect::<Vec<_>>().join(" ");
            match (path, data.is_empty()) {
                (Some(p), false) => cmd_write(&resolve(p), data.as_bytes(), wr),
                _ => wr.put_str("  Usage: write <path> <content>\n", writer::DIM, SCALE),
            }
        }
        "reboot"  => cmd_reboot(),
        "exec"    => {
            if let Some(raw) = parts.next() {
                if let Err(_) = crate::task::spawn_process(&resolve(raw)) {
                    wr.put_str("  Error: failed to spawn process\n", writer::RED, SCALE);
                }
            } else {
                wr.put_str("  Usage: exec <path>\n", writer::DIM, SCALE);
            }
        }
        ""        => {}
        other     => {
            wr.put_str("  Error: unknown command '", writer::RED, SCALE);
            wr.put_str(other, writer::RED, SCALE);
            wr.put_str("'\n", writer::RED, SCALE);
        }
    }
}

fn cmd_pwd(wr: &mut writer::Writer) {
    let cwd = unsafe { core::str::from_utf8_unchecked(&CWD_BUF[..CWD_LEN]) };
    wr.put_str("  ", writer::FG, SCALE);
    wr.put_str(cwd, writer::ACCENT, SCALE);
    wr.put_char(b'\n', writer::FG, SCALE);
}

fn cmd_cd(raw: &str, wr: &mut writer::Writer) {
    let path = resolve(raw);
    match crate::vfs::open(&path) {
        Ok(node) if node.attribute().node_type == crate::vfs::NodeType::Directory => {
            let bytes = path.as_bytes();
            let len = bytes.len().min(MAX_CWD);
            unsafe {
                CWD_BUF[..len].copy_from_slice(&bytes[..len]);
                CWD_LEN = len;
            }
        }
        Ok(_) => wr.put_str("  Error: not a directory\n", writer::RED, SCALE),
        Err(_) => wr.put_str("  Error: no such path\n", writer::RED, SCALE),
    }
}

fn cmd_ls(path: &str, wr: &mut writer::Writer) {
    let target = if path.is_empty() { "/" } else { path };
    let node = match crate::vfs::open(target) {
        Ok(n) => n,
        Err(_) => {
            wr.put_str("  Error: no such path\n", writer::RED, SCALE);
            return;
        }
    };
    match node.readdir() {
        Ok(files) => {
            wr.put_str("  Files in ", writer::ACCENT, SCALE);
            wr.put_str(target, writer::ACCENT, SCALE);
            wr.put_str(":\n", writer::ACCENT, SCALE);
            for file in files {
                wr.put_str("    ", writer::FG, SCALE);
                wr.put_str(&file, writer::FG, SCALE);
                wr.put_char(b'\n', writer::FG, SCALE);
            }
        }
        Err(_) => {
            wr.put_str("  Error: not a directory\n", writer::RED, SCALE);
        }
    }
}

fn cmd_cat(path: &str, wr: &mut writer::Writer) {
    let node = match crate::vfs::open(path) {
        Ok(n) => n,
        Err(_) => { wr.put_str("  Error: file not found\n", writer::RED, SCALE); return; }
    };
    let size = node.attribute().size;
    if size == 0 { wr.put_str("  (empty)\n", writer::DIM, SCALE); return; }
    let mut buf = alloc::vec![0u8; size.min(4096)];
    match node.read(0, &mut buf) {
        Ok(n) => {
            for b in &buf[..n] {
                if *b == b'\n' { wr.put_char(b'\n', writer::FG, SCALE); }
                else if b.is_ascii_graphic() || *b == b' ' {
                    wr.put_char(*b, writer::FG, SCALE);
                }
            }
            wr.put_char(b'\n', writer::FG, SCALE);
        }
        Err(_) => wr.put_str("  Error: read failed\n", writer::RED, SCALE),
    }
}

fn cmd_write(path: &str, data: &[u8], wr: &mut writer::Writer) {
    crate::serial::serial_print("[WRITE] path=");
    crate::serial::serial_print(path);
    crate::serial::serial_print("\n");

    let node = match crate::vfs::open(path) {
        Ok(n) => {
            crate::serial::serial_print("[WRITE] file exists, overwriting\n");
            n
        }
        Err(_) => match crate::vfs::create(path) {
            Ok(n) => {
                crate::serial::serial_print("[WRITE] created new file\n");
                n
            }
            Err(_) => {
                crate::serial::serial_print("[WRITE] create failed\n");
                wr.put_str("  Error: cannot create file\n", writer::RED, SCALE);
                return;
            }
        }
    };
    match node.write(0, data) {
        Ok(n) => {
            use core::fmt::Write;
            let _ = writeln!(wr, "  Wrote {} bytes to {}", n, path);
        }
        Err(_) => {
            crate::serial::serial_print("[WRITE] write() failed\n");
            wr.put_str("  Error: write failed (read-only?)\n", writer::RED, SCALE);
        }
    }
}

fn cmd_lspci(wr: &mut writer::Writer) {
    use core::fmt::Write;
    let devices = crate::pci::DEVICES.lock();
    if devices.is_empty() {
        wr.put_str("  No PCI devices enumerated\n", writer::DIM, SCALE);
        return;
    }
    wr.put_str("  PCI Devices:\n", writer::ACCENT, SCALE);
    for d in devices.iter() {
        let _ = writeln!(
            wr,
            "    {:02x}:{:02x}.{}  {:04x}:{:04x}  {}",
            d.bus, d.device, d.function,
            d.vendor_id, d.device_id,
            crate::pci::class_name(d.class, d.subclass),
        );
    }
}

fn cmd_disk(wr: &mut writer::Writer) {
    use core::fmt::Write;
    if !crate::ahci::is_ready() {
        wr.put_str("  AHCI: no SATA drive attached\n", writer::RED, SCALE);
        return;
    }
    let mut buf = [0u8; 512];
    match crate::ahci::read_sectors(0, 1, &mut buf) {
        Ok(()) => {
            let sig = u16::from_le_bytes([buf[510], buf[511]]);
            let _ = writeln!(
                wr,
                "  Sector 0 read OK  sig=0x{:04x} {}",
                sig,
                if sig == 0xAA55 { "(valid MBR)" } else { "(empty/not MBR)" }
            );
            wr.put_str("  First 16 bytes: ", writer::DIM, SCALE);
            for b in &buf[..16] {
                let _ = write!(wr, "{:02x} ", b);
            }
            wr.put_char(b'\n', writer::FG, SCALE);
        }
        Err(e) => {
            wr.put_str("  AHCI error: ", writer::RED, SCALE);
            wr.put_str(e, writer::RED, SCALE);
            wr.put_char(b'\n', writer::FG, SCALE);
        }
    }
}

fn cmd_reboot() -> ! {
    // PS/2 Controller Reset (command 0xFE to port 0x64)
    unsafe {
        crate::pic::outb(0x64, 0xFE);
    }
    loop {
        unsafe { core::arch::asm!("hlt"); }
    }
}

// ── Commands ──────────────────────────────────────────────────────────────────

fn cmd_help(wr: &mut writer::Writer) {
    let cwd = unsafe { core::str::from_utf8_unchecked(&CWD_BUF[..CWD_LEN]) };
    wr.put_str("  cwd: ", writer::DIM, SCALE);
    wr.put_str(cwd, writer::ACCENT, SCALE);
    wr.put_char(b'\n', writer::FG, SCALE);
    let cmds: &[(&str, &str)] = &[
        ("help",    "Show this help message"),
        ("clear",   "Clear the terminal area"),
        ("pwd",     "Show current directory"),
        ("cd",      "Change directory: cd <path>"),
        ("ls",      "List directory (default: cwd)"),
        ("lspci",   "List enumerated PCI devices"),
        ("disk",    "Read sector 0 and show MBR signature"),
        ("cat",     "Print a file (relative paths use cwd)"),
        ("write",   "Write to a file: write <path> <content>"),
        ("exec",    "Execute an ELF binary"),
        ("memory",  "Show physical memory statistics"),
        ("uptime",  "Show system uptime"),
        ("version", "Show ROSS version info"),
        ("reboot",  "Restart the system"),
    ];
    wr.put_str("  Available commands:\n", writer::ACCENT, SCALE);
    for (name, desc) in cmds {
        wr.put_str("    ", writer::FG, SCALE);
        wr.put_str(name, writer::FG, SCALE);
        // pad to 10 chars
        for _ in name.len()..10 { wr.put_char(b' ', writer::FG, SCALE); }
        wr.put_str("- ", writer::DIM, SCALE);
        wr.put_str(desc, writer::DIM, SCALE);
        wr.put_char(b'\n', writer::FG, SCALE);
    }
}

fn cmd_clear(wr: &mut writer::Writer) {
    let w = wr.w;
    let h = wr.h;
    wr.fill_rect(0, TERM_Y, w, h - TERM_Y, writer::BG);
    wr.set_pos(writer::LEFT_MARGIN, TERM_Y + 6);
    wr.put_str("ROSS Shell v0.4  |  Type 'help' for commands", writer::DIM, 1);
    wr.set_pos(writer::LEFT_MARGIN, TERM_Y + 20);
}

fn cmd_memory(wr: &mut writer::Writer) {
    let free_mib  = pmm::free_mib();
    let total_mib = pmm::total_mib();
    let used_mib  = total_mib.saturating_sub(free_mib);

    wr.put_str("  Physical Memory\n", writer::ACCENT, SCALE);

    wr.put_str("    Total  : ", writer::FG,  SCALE);
    print_num(wr, total_mib);
    wr.put_str(" MiB\n", writer::FG, SCALE);

    wr.put_str("    Used   : ", writer::FG,  SCALE);
    print_num(wr, used_mib);
    wr.put_str(" MiB\n", writer::FG, SCALE);

    wr.put_str("    Free   : ", writer::ACCENT, SCALE);
    print_num(wr, free_mib);
    wr.put_str(" MiB\n", writer::ACCENT, SCALE);
}

fn cmd_uptime(wr: &mut writer::Writer) {
    let ticks   = pit::ticks();
    let seconds = ticks / 100;
    let minutes = seconds / 60;
    let secs    = seconds % 60;

    wr.put_str("  Uptime: ", writer::FG, SCALE);
    print_num(wr, minutes as usize);
    wr.put_str("m ", writer::FG, SCALE);
    print_num(wr, secs as usize);
    wr.put_str("s  (", writer::DIM, SCALE);
    print_num(wr, ticks as usize);
    wr.put_str(" ticks @ 100 Hz)\n", writer::DIM, SCALE);

}

fn cmd_version(wr: &mut writer::Writer) {
    wr.put_str("  R.O.S.S.  Rapid Operating System Shell\n", writer::ACCENT, SCALE);
    wr.put_str("  Phase 6  |  x86_64  |  Bare Metal\n", writer::DIM, SCALE);
}

// ── Utility ───────────────────────────────────────────────────────────────────

/// Print a usize in decimal without using alloc.
fn print_num(wr: &mut writer::Writer, mut n: usize) {
    if n == 0 { wr.put_char(b'0', writer::FG, SCALE); return; }
    let mut buf = [0u8; 20];
    let mut i = 0;
    while n > 0 { buf[i] = b'0' + (n % 10) as u8; n /= 10; i += 1; }
    while i > 0 { i -= 1; wr.put_char(buf[i], writer::FG, SCALE); }
}
