use alloc::sync::Arc;
use alloc::vec::Vec;
use crate::vfs::VfsNode;

const FD_MAX: usize = 64;

pub enum FdEntry {
    Stdin,
    Stdout,
    Stderr,
    Pipe(usize),                                   // IPC pipe ID
    VfsFile(Arc<dyn VfsNode>, usize),              // (node, byte offset)
}

pub struct FdTable {
    slots: Vec<Option<FdEntry>>,
}

impl FdTable {
    pub fn new() -> Self {
        let mut slots = Vec::with_capacity(FD_MAX);
        for _ in 0..FD_MAX { slots.push(None); }
        Self { slots }
    }

    pub fn with_stdio() -> Self {
        let mut t = Self::new();
        t.slots[0] = Some(FdEntry::Stdin);
        t.slots[1] = Some(FdEntry::Stdout);
        t.slots[2] = Some(FdEntry::Stderr);
        t
    }

    /// Insert entry starting from fd 3. Returns the assigned fd or None if full.
    pub fn alloc(&mut self, entry: FdEntry) -> Option<usize> {
        for i in 3..FD_MAX {
            if self.slots[i].is_none() {
                self.slots[i] = Some(entry);
                return Some(i);
            }
        }
        None
    }

    pub fn get(&self, fd: usize) -> Option<&FdEntry> {
        self.slots.get(fd)?.as_ref()
    }

    pub fn get_mut(&mut self, fd: usize) -> Option<&mut FdEntry> {
        self.slots.get_mut(fd)?.as_mut()
    }

    pub fn close(&mut self, fd: usize) -> bool {
        if fd < FD_MAX && self.slots[fd].is_some() {
            self.slots[fd] = None;
            true
        } else {
            false
        }
    }
}
