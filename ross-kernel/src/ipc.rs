//! Kernel-managed IPC pipes (anonymous, byte-stream, non-blocking).
//!
//! A pipe is a single kernel ring buffer addressed by an integer ID. The
//! syscalls `sys_pipe`/`sys_read`/`sys_write` let user tasks exchange bytes
//! through it. Both ends share the same ID — the distinction between "read
//! end" and "write end" is left to the userland convention.

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use spin::Mutex;

const MAX_PIPE_BYTES: usize = 4096;
const MAX_PIPES:      usize = 64;

struct Pipe {
    buffer: VecDeque<u8>,
    closed: bool,
}

struct PipeTable {
    pipes: Vec<Option<Pipe>>,
}

impl PipeTable {
    const fn new() -> Self {
        Self { pipes: Vec::new() }
    }
}

static PIPES: Mutex<PipeTable> = Mutex::new(PipeTable::new());

/// Create a new pipe. Returns its ID on success.
pub fn create() -> Option<usize> {
    let mut tbl = PIPES.lock();

    // Reuse a freed slot if available
    for (i, slot) in tbl.pipes.iter_mut().enumerate() {
        if slot.is_none() {
            *slot = Some(Pipe { buffer: VecDeque::new(), closed: false });
            return Some(i);
        }
    }

    if tbl.pipes.len() >= MAX_PIPES { return None; }
    tbl.pipes.push(Some(Pipe { buffer: VecDeque::new(), closed: false }));
    Some(tbl.pipes.len() - 1)
}

/// Append bytes to the pipe. Returns bytes written (may be less than `data.len()`
/// if the pipe is full). `Err(())` if the pipe ID is invalid or closed.
pub fn write(id: usize, data: &[u8]) -> Result<usize, ()> {
    let mut tbl = PIPES.lock();
    let pipe = tbl.pipes.get_mut(id).and_then(|s| s.as_mut()).ok_or(())?;
    if pipe.closed { return Err(()); }
    let free = MAX_PIPE_BYTES.saturating_sub(pipe.buffer.len());
    let take = core::cmp::min(free, data.len());
    for b in &data[..take] {
        pipe.buffer.push_back(*b);
    }
    Ok(take)
}

/// Drain up to `buf.len()` bytes from the pipe. Returns bytes read (0 if empty).
pub fn read(id: usize, buf: &mut [u8]) -> Result<usize, ()> {
    let mut tbl = PIPES.lock();
    let pipe = tbl.pipes.get_mut(id).and_then(|s| s.as_mut()).ok_or(())?;
    let n = core::cmp::min(buf.len(), pipe.buffer.len());
    for slot in &mut buf[..n] {
        *slot = pipe.buffer.pop_front().unwrap();
    }
    Ok(n)
}

/// Close a pipe, releasing its slot once drained.
#[allow(dead_code)]
pub fn close(id: usize) {
    let mut tbl = PIPES.lock();
    if let Some(slot) = tbl.pipes.get_mut(id) {
        *slot = None;
    }
}
