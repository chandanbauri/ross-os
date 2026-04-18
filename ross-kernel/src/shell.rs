/// ROSS Shell — Phase 4
///
/// Implements:
///   • Two-state machine: Splash → Active
///   • Line editing: printable chars, Backspace, Enter
///   • Command dispatch: help, clear, memory, uptime, version
///   • Coloured output via writer colour constants

use crate::{pit, pmm, writer};

const MAX_LINE: usize = 128;
const SCALE:    usize = 1;

/// Y-coordinate where the terminal area begins.
pub const TERM_Y: usize = 60;

static mut LINE_BUF: [u8; MAX_LINE] = [0u8; MAX_LINE];
static mut LINE_LEN: usize = 0;

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
        "ls"      => cmd_ls(wr),
        "reboot"  => cmd_reboot(),
        "exec"    => {
            if let Some(path) = parts.next() {
                if let Err(_) = crate::task::spawn_process(path) {
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

fn cmd_ls(wr: &mut writer::Writer) {
    if let Ok(files) = crate::vfs::VFS.lock().root_node.as_ref().map(|n| n.readdir()).unwrap_or(Err(())) {
        wr.put_str("  Files in /:\n", writer::ACCENT, SCALE);
        for file in files {
            wr.put_str("    ", writer::FG, SCALE);
            wr.put_str(&file, writer::FG, SCALE);
            wr.put_char(b'\n', writer::FG, SCALE);
        }
    } else {
        wr.put_str("  Error: failed to list directory\n", writer::RED, SCALE);
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
    let cmds: &[(&str, &str)] = &[
        ("help",    "Show this help message"),
        ("clear",   "Clear the terminal area"),
        ("ls",      "List files in the RAMDisk"),
        ("exec",    "Execute an ELF binary (exec <path>)"),
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
