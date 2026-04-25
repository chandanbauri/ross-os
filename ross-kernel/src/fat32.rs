//! FAT32 driver — read + write, mounted at /mnt/disk via the VFS.
//!
//! Supports 8.3 filenames only (LFN entries are silently skipped).
//! Sectors-per-cluster > 8 are handled by looping AHCI calls in 8-sector
//! chunks.  Write path: cluster allocation, FAT chain update (both copies),
//! cluster-level read-modify-write, and directory-entry size update.

use crate::ahci;
use crate::vfs::{NodeType, Stat, VfsNode};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

// ── BPB layout ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct BiosParameterBlock {
    bytes_per_sector:    u32,
    sectors_per_cluster: u32,
    reserved_sectors:    u32,
    num_fats:            u32,
    fat_size_sectors:    u32,
    root_cluster:        u32,
}

fn parse_bpb(sector: &[u8; 512]) -> Result<BiosParameterBlock, &'static str> {
    if sector[510] != 0x55 || sector[511] != 0xAA {
        return Err("FAT32: missing 0x55AA signature");
    }
    let bps  = u16::from_le_bytes([sector[0x0B], sector[0x0C]]) as u32;
    let spc  = sector[0x0D] as u32;
    let rsvd = u16::from_le_bytes([sector[0x0E], sector[0x0F]]) as u32;
    let nfat = sector[0x10] as u32;
    let fat16 = u16::from_le_bytes([sector[0x16], sector[0x17]]) as u32;
    let fat32 = u32::from_le_bytes([sector[0x24], sector[0x25], sector[0x26], sector[0x27]]);
    let root  = u32::from_le_bytes([sector[0x2C], sector[0x2D], sector[0x2E], sector[0x2F]]);

    if fat16 != 0 { return Err("FAT32: FAT12/16 not supported"); }
    if bps != 512 { return Err("FAT32: only 512-byte sectors supported"); }
    if spc == 0 || !spc.is_power_of_two() { return Err("FAT32: bad sectors_per_cluster"); }

    Ok(BiosParameterBlock {
        bytes_per_sector:    bps,
        sectors_per_cluster: spc,
        reserved_sectors:    rsvd,
        num_fats:            nfat,
        fat_size_sectors:    fat32,
        root_cluster:        root,
    })
}

// ── Filesystem object ────────────────────────────────────────────────────────

pub struct Fat32Fs {
    bpb:               BiosParameterBlock,
    fat_start_sector:  u64,
    data_start_sector: u64,
}

impl Fat32Fs {
    pub fn mount() -> Result<Arc<Self>, &'static str> {
        let mut sec = [0u8; 512];
        ahci::read_sectors(0, 1, &mut sec)?;
        let bpb = parse_bpb(&sec)?;

        let fat_start  = bpb.reserved_sectors as u64;
        let data_start = fat_start + (bpb.num_fats as u64) * (bpb.fat_size_sectors as u64);
        Ok(Arc::new(Self { bpb, fat_start_sector: fat_start, data_start_sector: data_start }))
    }

    pub fn root(self: &Arc<Self>) -> Arc<dyn VfsNode> {
        Arc::new(Fat32Node::dir(self.clone(), self.bpb.root_cluster, 0, 0))
    }

    // ── Low-level helpers ────────────────────────────────────────────────────

    fn cluster_to_sector(&self, cluster: u32) -> u64 {
        self.data_start_sector + ((cluster as u64) - 2) * (self.bpb.sectors_per_cluster as u64)
    }

    fn cluster_size_bytes(&self) -> usize {
        (self.bpb.sectors_per_cluster as usize) * 512
    }

    /// Read one cluster into `buf` (buf must be ≥ cluster_size_bytes).
    fn read_cluster(&self, cluster: u32, buf: &mut [u8]) -> Result<(), &'static str> {
        let first = self.cluster_to_sector(cluster);
        let mut rem = self.bpb.sectors_per_cluster;
        let mut sec = first;
        let mut off = 0usize;
        while rem > 0 {
            let cnt = rem.min(8) as u16;
            ahci::read_sectors(sec, cnt, &mut buf[off..])?;
            sec += cnt as u64;
            off += cnt as usize * 512;
            rem -= cnt as u32;
        }
        Ok(())
    }

    /// Write one cluster from `buf`.
    fn write_cluster(&self, cluster: u32, buf: &[u8]) -> Result<(), &'static str> {
        let first = self.cluster_to_sector(cluster);
        let mut rem = self.bpb.sectors_per_cluster;
        let mut sec = first;
        let mut off = 0usize;
        while rem > 0 {
            let cnt = rem.min(8) as u16;
            ahci::write_sectors(sec, cnt, &buf[off..])?;
            sec += cnt as u64;
            off += cnt as usize * 512;
            rem -= cnt as u32;
        }
        Ok(())
    }

    fn next_cluster(&self, cluster: u32) -> Result<u32, &'static str> {
        let ob  = (cluster as u64) * 4;
        let sec = self.fat_start_sector + ob / 512;
        let pos = (ob % 512) as usize;
        let mut buf = [0u8; 512];
        ahci::read_sectors(sec, 1, &mut buf)?;
        let raw = u32::from_le_bytes([buf[pos], buf[pos+1], buf[pos+2], buf[pos+3]]);
        Ok(raw & 0x0FFF_FFFF)
    }

    fn set_fat_entry(&self, cluster: u32, value: u32) -> Result<(), &'static str> {
        let ob  = (cluster as u64) * 4;
        let sec_off = ob / 512;
        let pos     = (ob % 512) as usize;
        for f in 0..self.bpb.num_fats {
            let sec = self.fat_start_sector + f as u64 * self.bpb.fat_size_sectors as u64 + sec_off;
            let mut buf = [0u8; 512];
            ahci::read_sectors(sec, 1, &mut buf)?;
            let existing = u32::from_le_bytes([buf[pos], buf[pos+1], buf[pos+2], buf[pos+3]]);
            let new = (existing & 0xF000_0000) | (value & 0x0FFF_FFFF);
            buf[pos..pos+4].copy_from_slice(&new.to_le_bytes());
            ahci::write_sectors(sec, 1, &buf)?;
        }
        Ok(())
    }

    /// Allocate a free cluster, zero it, and optionally link it after `prev`.
    fn alloc_cluster(&self, prev: Option<u32>) -> Result<u32, &'static str> {
        let entries_per_sector = 512u64 / 4;
        let mut found = None;
        'scan: for si in 0..self.bpb.fat_size_sectors as u64 {
            let mut buf = [0u8; 512];
            ahci::read_sectors(self.fat_start_sector + si, 1, &mut buf)?;
            for i in 0..entries_per_sector as usize {
                let cluster = (si * entries_per_sector) as u32 + i as u32;
                if cluster < 2 { continue; }
                let e = u32::from_le_bytes([buf[i*4], buf[i*4+1], buf[i*4+2], buf[i*4+3]]) & 0x0FFF_FFFF;
                if e == 0 { found = Some(cluster); break 'scan; }
            }
        }
        let c = found.ok_or("FAT32: disk full")?;
        self.set_fat_entry(c, 0x0FFF_FFFF)?;    // mark EOC
        if let Some(p) = prev {
            self.set_fat_entry(p, c)?;           // link prev → new
        }
        // Zero the new cluster
        let cb = self.cluster_size_bytes();
        let zero = vec![0u8; cb];
        self.write_cluster(c, &zero)?;
        Ok(c)
    }

    fn read_chain_into(&self, first: u32, out: &mut Vec<u8>, max: usize) -> Result<(), &'static str> {
        out.clear();
        if first < 2 { return Ok(()); }
        let cb = self.cluster_size_bytes();
        let mut cluster = first;
        while cluster >= 2 && cluster < 0x0FFF_FFF8 && out.len() < max {
            let take = cb.min(max - out.len());
            let old_len = out.len();
            out.resize(old_len + cb, 0);
            self.read_cluster(cluster, &mut out[old_len..])?;
            out.truncate(old_len + take);
            cluster = self.next_cluster(cluster)?;
        }
        Ok(())
    }
}

// ── Node ─────────────────────────────────────────────────────────────────────

pub struct Fat32Node {
    fs:               Arc<Fat32Fs>,
    first_cluster:    AtomicU32,
    size:             AtomicU32,
    is_dir:           bool,
    /// Cluster containing this entry's 32-byte record (0 = root / no parent).
    parent_cluster:   u32,
    /// Byte offset of the 32-byte record inside `parent_cluster`.
    dir_entry_offset: usize,
}

impl Fat32Node {
    fn dir(fs: Arc<Fat32Fs>, first: u32, parent: u32, off: usize) -> Self {
        Self { fs, first_cluster: AtomicU32::new(first), size: AtomicU32::new(0),
               is_dir: true, parent_cluster: parent, dir_entry_offset: off }
    }
    fn file(fs: Arc<Fat32Fs>, first: u32, size: u32, parent: u32, off: usize) -> Self {
        Self { fs, first_cluster: AtomicU32::new(first), size: AtomicU32::new(size),
               is_dir: false, parent_cluster: parent, dir_entry_offset: off }
    }

    /// Iterate every live 8.3 dir entry with its (cluster, offset_in_cluster).
    /// Returns `Some(T)` when `f` returns Some, or `None` after all entries.
    fn walk_dir<T>(&self, mut f: impl FnMut(&[u8; 32], u32, usize) -> Option<T>)
        -> Result<Option<T>, &'static str>
    {
        let cb = self.fs.cluster_size_bytes();
        let mut buf = vec![0u8; cb];
        let mut cluster = self.first_cluster.load(Ordering::Relaxed);
        while cluster >= 2 && cluster < 0x0FFF_FFF8 {
            self.fs.read_cluster(cluster, &mut buf)?;
            let mut i = 0;
            while i + 32 <= cb {
                let entry: &[u8; 32] = buf[i..i+32].try_into().unwrap();
                let b0 = entry[0];
                if b0 == 0x00 { return Ok(None); }
                let off = i;
                i += 32;
                if b0 == 0xE5 { continue; }
                let attr = entry[0x0B];
                if attr == 0x0F || attr & 0x08 != 0 { continue; }
                if let Some(v) = f(entry, cluster, off) { return Ok(Some(v)); }
            }
            cluster = self.fs.next_cluster(cluster)?;
        }
        Ok(None)
    }

    /// Find the first free (0x00 or 0xE5) dir-entry slot, extending the cluster
    /// chain if necessary. Returns (cluster, byte_offset_in_cluster).
    fn find_free_slot(&self) -> Result<(u32, usize), &'static str> {
        let cb = self.fs.cluster_size_bytes();
        let mut buf = vec![0u8; cb];
        let mut cluster = self.first_cluster.load(Ordering::Relaxed);
        let mut last_cluster = cluster;
        while cluster >= 2 && cluster < 0x0FFF_FFF8 {
            last_cluster = cluster;
            self.fs.read_cluster(cluster, &mut buf)?;
            for i in (0..cb).step_by(32) {
                if buf[i] == 0x00 || buf[i] == 0xE5 {
                    return Ok((cluster, i));
                }
            }
            cluster = self.fs.next_cluster(cluster)?;
        }
        // No free slot found — extend directory by one cluster.
        let new_c = self.fs.alloc_cluster(Some(last_cluster))?;
        Ok((new_c, 0))
    }

    fn update_size_on_disk(&self, new_size: u32) -> Result<(), &'static str> {
        if self.parent_cluster == 0 { return Ok(()); }
        let cb = self.fs.cluster_size_bytes();
        let mut buf = vec![0u8; cb];
        self.fs.read_cluster(self.parent_cluster, &mut buf)?;
        let o = self.dir_entry_offset;
        buf[o+0x1C..o+0x20].copy_from_slice(&new_size.to_le_bytes());
        self.fs.write_cluster(self.parent_cluster, &buf)?;
        Ok(())
    }
}

// ── 8.3 helpers ──────────────────────────────────────────────────────────────

fn decode_8_3(e: &[u8; 32]) -> String {
    let name_end = e[0..8].iter().rposition(|&b| b != b' ').map(|i| i+1).unwrap_or(0);
    let ext_end  = e[8..11].iter().rposition(|&b| b != b' ').map(|i| i+1).unwrap_or(0);
    let mut s = String::new();
    for b in &e[0..name_end] { s.push(b.to_ascii_lowercase() as char); }
    if ext_end > 0 {
        s.push('.');
        for b in &e[8..8+ext_end] { s.push(b.to_ascii_lowercase() as char); }
    }
    s
}

fn name_matches(e: &[u8; 32], target: &str) -> bool {
    decode_8_3(e).eq_ignore_ascii_case(target)
}

fn first_cluster_of(e: &[u8; 32]) -> u32 {
    (u16::from_le_bytes([e[0x14], e[0x15]]) as u32) << 16
        | u16::from_le_bytes([e[0x1A], e[0x1B]]) as u32
}

fn entry_size(e: &[u8; 32]) -> u32 {
    u32::from_le_bytes([e[0x1C], e[0x1D], e[0x1E], e[0x1F]])
}

/// Convert a name like "foo.txt" to the 11-byte FAT 8.3 representation.
/// Returns `None` if the name can't be represented (too long, etc.).
pub fn to_8_3(name: &str) -> Option<[u8; 11]> {
    let bytes = name.as_bytes();
    let (base, ext) = if let Some(dot) = name.rfind('.') {
        (&bytes[..dot], &bytes[dot+1..])
    } else {
        (bytes, &[][..])
    };
    if base.is_empty() || base.len() > 8 || ext.len() > 3 { return None; }
    let mut out = [b' '; 11];
    for (i, b) in base.iter().enumerate() { out[i] = b.to_ascii_uppercase(); }
    for (i, b) in ext.iter().enumerate()  { out[8+i] = b.to_ascii_uppercase(); }
    Some(out)
}

// ── VfsNode ──────────────────────────────────────────────────────────────────

impl VfsNode for Fat32Node {
    fn attribute(&self) -> Stat {
        Stat {
            size: self.size.load(Ordering::Relaxed) as usize,
            node_type: if self.is_dir { NodeType::Directory } else { NodeType::File },
        }
    }

    fn read(&self, offset: usize, buffer: &mut [u8]) -> Result<usize, ()> {
        if self.is_dir { return Err(()); }
        let total = self.size.load(Ordering::Relaxed) as usize;
        if offset >= total { return Ok(0); }
        let want = buffer.len().min(total - offset);
        let mut chain = Vec::new();
        self.fs.read_chain_into(self.first_cluster.load(Ordering::Relaxed),
                                &mut chain, offset + want).map_err(|_| ())?;
        let end = chain.len().min(offset + want);
        if offset >= end { return Ok(0); }
        let n = end - offset;
        buffer[..n].copy_from_slice(&chain[offset..end]);
        Ok(n)
    }

    fn write(&self, offset: usize, data: &[u8]) -> Result<usize, ()> {
        if self.is_dir || data.is_empty() { return Err(()); }
        let cb = self.fs.cluster_size_bytes();
        let old_size = self.size.load(Ordering::Relaxed) as usize;
        let new_size = (offset + data.len()).max(old_size);

        // Ensure there is a first cluster.
        if self.first_cluster.load(Ordering::Relaxed) < 2 {
            let c = self.fs.alloc_cluster(None).map_err(|_| ())?;
            self.first_cluster.store(c, Ordering::Relaxed);
            // Update the dir entry's first-cluster fields.
            if self.parent_cluster != 0 {
                let mut buf = vec![0u8; cb];
                self.fs.read_cluster(self.parent_cluster, &mut buf).map_err(|_| ())?;
                let o = self.dir_entry_offset;
                buf[o+0x14] = ((c >> 16) & 0xFF) as u8;
                buf[o+0x15] = ((c >> 24) & 0xFF) as u8;
                buf[o+0x1A] = (c & 0xFF) as u8;
                buf[o+0x1B] = ((c >> 8) & 0xFF) as u8;
                self.fs.write_cluster(self.parent_cluster, &buf).map_err(|_| ())?;
            }
        }

        // Walk/extend chain to cover [offset .. offset+data.len()).
        let start_cluster_idx = offset / cb;
        let mut cluster = self.first_cluster.load(Ordering::Relaxed);
        for _ in 0..start_cluster_idx {
            let next = self.fs.next_cluster(cluster).map_err(|_| ())?;
            cluster = if next >= 0x0FFF_FFF8 {
                self.fs.alloc_cluster(Some(cluster)).map_err(|_| ())?
            } else { next };
        }

        let mut written = 0;
        let mut buf_off = offset % cb;
        while written < data.len() {
            let mut buf = vec![0u8; cb];
            self.fs.read_cluster(cluster, &mut buf).map_err(|_| ())?;
            let take = (data.len() - written).min(cb - buf_off);
            buf[buf_off..buf_off+take].copy_from_slice(&data[written..written+take]);
            self.fs.write_cluster(cluster, &buf).map_err(|_| ())?;
            written += take;
            buf_off  = 0;
            if written < data.len() {
                let next = self.fs.next_cluster(cluster).map_err(|_| ())?;
                cluster = if next >= 0x0FFF_FFF8 {
                    self.fs.alloc_cluster(Some(cluster)).map_err(|_| ())?
                } else { next };
            }
        }

        if new_size > old_size {
            self.size.store(new_size as u32, Ordering::Relaxed);
            self.update_size_on_disk(new_size as u32).map_err(|_| ())?;
        }
        Ok(written)
    }

    fn create(&self, name: &str) -> Result<Arc<dyn VfsNode>, ()> {
        if !self.is_dir { return Err(()); }
        let name83 = to_8_3(name).ok_or(())?;

        // Check for duplicate.
        let dup = self.walk_dir(|e, _, _| if name_matches(e, name) { Some(()) } else { None })
            .map_err(|_| ())?;
        if dup.is_some() { return Err(()); }

        let (slot_cluster, slot_off) = self.find_free_slot().map_err(|_| ())?;
        let cb = self.fs.cluster_size_bytes();
        let mut buf = vec![0u8; cb];
        self.fs.read_cluster(slot_cluster, &mut buf).map_err(|_| ())?;

        let e = &mut buf[slot_off..slot_off+32];
        e.fill(0);
        e[0..11].copy_from_slice(&name83);
        e[0x0B] = 0x20; // archive
        // first_cluster = 0 (empty); size = 0.

        // If next entry is in range, mark it as end-of-directory.
        if slot_off + 64 <= cb { buf[slot_off+32] = 0x00; }

        self.fs.write_cluster(slot_cluster, &buf).map_err(|_| ())?;

        Ok(Arc::new(Fat32Node::file(
            self.fs.clone(), 0, 0, slot_cluster, slot_off,
        )))
    }

    fn readdir(&self) -> Result<Vec<String>, ()> {
        if !self.is_dir { return Err(()); }
        let mut names = Vec::new();
        self.walk_dir(|e, _, _| -> Option<()> { names.push(decode_8_3(e)); None })
            .map_err(|_| ())?;
        Ok(names)
    }

    fn finddir(&self, name: &str) -> Result<Arc<dyn VfsNode>, ()> {
        if !self.is_dir { return Err(()); }
        let found = self.walk_dir(|e, cluster, off| {
            if !name_matches(e, name) { return None; }
            let is_dir = e[0x0B] & 0x10 != 0;
            Some((first_cluster_of(e), entry_size(e), is_dir, cluster, off))
        }).map_err(|_| ())?;

        if let Some((first, size, is_dir, pcluster, poff)) = found {
            let node = if is_dir {
                Fat32Node::dir( self.fs.clone(), first, pcluster, poff)
            } else {
                Fat32Node::file(self.fs.clone(), first, size, pcluster, poff)
            };
            Ok(Arc::new(node))
        } else { Err(()) }
    }
}
