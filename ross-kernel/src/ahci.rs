//! Minimal AHCI SATA driver — synchronous READ via DMA.
//!
//! Only enough to read sectors from port 0 of the HBA. Supports up to 8
//! sectors per call (fits in one 4 KB DMA page).

use crate::paging::phys_to_virt;
use crate::pci;
use crate::pmm;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;

// ── Generic HBA memory registers ────────────────────────────────────────────
const HBA_CAP:  usize = 0x00;
const HBA_GHC:  usize = 0x04;
const HBA_PI:   usize = 0x0C;

const GHC_AE:   u32 = 1 << 31; // AHCI Enable

// ── Per-port registers (offset from HBA_PORT_BASE + port*0x80) ──────────────
const HBA_PORT_BASE: usize = 0x100;
const HBA_PORT_SIZE: usize = 0x80;

const PX_CLB:   usize = 0x00;
const PX_CLBU:  usize = 0x04;
const PX_FB:    usize = 0x08;
const PX_FBU:   usize = 0x0C;
const PX_IS:    usize = 0x10;
const PX_CMD:   usize = 0x18;
const PX_TFD:   usize = 0x20;
const PX_SIG:   usize = 0x24;
const PX_SSTS:  usize = 0x28;
const PX_SERR:  usize = 0x30;
const PX_CI:    usize = 0x38;

const CMD_ST:   u32 = 1 << 0;
const CMD_FRE:  u32 = 1 << 4;
const CMD_FR:   u32 = 1 << 14;
const CMD_CR:   u32 = 1 << 15;

const TFD_ERR:  u32 = 1 << 0;
const TFD_DRQ:  u32 = 1 << 3;
const TFD_BSY:  u32 = 1 << 7;

const IS_TFES:  u32 = 1 << 30; // Task File Error Status

const SIG_SATA: u32 = 0x0000_0101;
const ATA_CMD_READ_DMA_EXT:  u8 = 0x25;
const ATA_CMD_WRITE_DMA_EXT: u8 = 0x35;

// ── DMA structures ──────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy)]
struct CommandHeader {
    flags:     u16,
    prdtl:     u16,
    prdbc:     u32,
    ctba:      u32,
    ctbau:     u32,
    _reserved: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct PrdtEntry {
    dba:       u32,
    dbau:      u32,
    _reserved: u32,
    dbc:       u32, // bits 21:0 = byte count - 1
}

#[repr(C)]
struct CommandTable {
    cfis:      [u8; 64],
    acmd:      [u8; 16],
    _reserved: [u8; 48],
    prdt:      [PrdtEntry; 8],
}

// ── Driver state ────────────────────────────────────────────────────────────

struct PortState {
    port:     u8,
    clb_phys: u64,
    ct_phys:  u64,
    dma_phys: u64,
}

static PORT: Mutex<Option<PortState>> = Mutex::new(None);

/// ABAR mapped virtual address. We use the low-half identity map (phys == virt
/// for 0..4 GB), because `phys_to_virt` wraps around for addresses ≥ 2 GB and
/// MMIO regions (e.g. AHCI at ~4 GB) fall outside the higher-half kernel alias.
static ABAR_VIRT: AtomicUsize = AtomicUsize::new(0);

unsafe fn mmio_read(off: usize) -> u32 {
    let base = ABAR_VIRT.load(Ordering::Relaxed);
    unsafe { read_volatile((base + off) as *const u32) }
}

unsafe fn mmio_write(off: usize, val: u32) {
    let base = ABAR_VIRT.load(Ordering::Relaxed);
    unsafe { write_volatile((base + off) as *mut u32, val) }
}

unsafe fn port_read(port: u8, off: usize) -> u32 {
    unsafe { mmio_read(HBA_PORT_BASE + (port as usize) * HBA_PORT_SIZE + off) }
}

unsafe fn port_write(port: u8, off: usize, val: u32) {
    unsafe { mmio_write(HBA_PORT_BASE + (port as usize) * HBA_PORT_SIZE + off, val) }
}

// ── Public API ──────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    let controller = pci::find_by_class(0x01, 0x06).ok_or("No AHCI controller")?;
    let bar5 = controller.read_bar(5);
    let abar_phys = (bar5 & 0xFFFF_FFF0) as u64;
    if abar_phys == 0 { return Err("ABAR is zero"); }

    // ABAR is MMIO at ~4 GB — reach it via the low-half identity map.
    ABAR_VIRT.store(abar_phys as usize, Ordering::Relaxed);

    use core::fmt::Write;
    let mut serial = crate::serial::SerialPort;
    let _ = writeln!(
        serial,
        "[AHCI] controller {:02x}:{:02x}.{} ABAR=0x{:08x}",
        controller.bus, controller.device, controller.function, abar_phys
    );

    unsafe {
        let ghc = mmio_read(HBA_GHC);
        mmio_write(HBA_GHC, ghc | GHC_AE);
        let cap = mmio_read(HBA_CAP);
        let pi  = mmio_read(HBA_PI);
        let _ = writeln!(serial, "[AHCI] CAP=0x{:08x} PI=0x{:08x}", cap, pi);

        for p in 0..32u8 {
            if pi & (1 << p) == 0 { continue; }
            let sig  = port_read(p, PX_SIG);
            let ssts = port_read(p, PX_SSTS);
            let det  = ssts & 0x0F;
            let ipm  = (ssts >> 8) & 0x0F;
            if det == 3 && ipm == 1 && sig == SIG_SATA {
                let _ = writeln!(serial, "[AHCI] SATA drive on port {}", p);
                init_port(p)?;
                return Ok(());
            }
        }
    }
    Err("No SATA drive attached")
}

fn init_port(port: u8) -> Result<(), &'static str> {
    unsafe {
        // Stop command engine before reprogramming CLB/FB
        let cmd = port_read(port, PX_CMD);
        port_write(port, PX_CMD, cmd & !(CMD_ST | CMD_FRE));
        for _ in 0..100_000 {
            if port_read(port, PX_CMD) & (CMD_FR | CMD_CR) == 0 { break; }
        }
    }

    let clb_phys = pmm::alloc_page().ok_or("OOM: CLB")? as u64;
    let fb_phys  = pmm::alloc_page().ok_or("OOM: FB")?  as u64;
    let ct_phys  = pmm::alloc_page().ok_or("OOM: CT")?  as u64;
    let dma_phys = pmm::alloc_page().ok_or("OOM: DMA")? as u64;

    unsafe {
        let clb_virt = phys_to_virt(clb_phys as usize) as *mut u8;
        let fb_virt  = phys_to_virt(fb_phys  as usize) as *mut u8;
        let ct_virt  = phys_to_virt(ct_phys  as usize) as *mut u8;
        let dma_virt = phys_to_virt(dma_phys as usize) as *mut u8;
        core::ptr::write_bytes(clb_virt, 0, 4096);
        core::ptr::write_bytes(fb_virt,  0, 4096);
        core::ptr::write_bytes(ct_virt,  0, 4096);
        core::ptr::write_bytes(dma_virt, 0, 4096);

        // Command header 0 → our one command table
        let hdr = clb_virt as *mut CommandHeader;
        (*hdr).ctba  = (ct_phys & 0xFFFF_FFFF) as u32;
        (*hdr).ctbau = (ct_phys >> 32) as u32;

        port_write(port, PX_CLB,  (clb_phys & 0xFFFF_FFFF) as u32);
        port_write(port, PX_CLBU, (clb_phys >> 32) as u32);
        port_write(port, PX_FB,   (fb_phys  & 0xFFFF_FFFF) as u32);
        port_write(port, PX_FBU,  (fb_phys  >> 32) as u32);
        port_write(port, PX_SERR, 0xFFFF_FFFF);
        port_write(port, PX_IS,   0xFFFF_FFFF);

        // Start command engine
        for _ in 0..100_000 {
            if port_read(port, PX_CMD) & CMD_CR == 0 { break; }
        }
        let cmd = port_read(port, PX_CMD);
        port_write(port, PX_CMD, cmd | CMD_FRE | CMD_ST);
    }

    *PORT.lock() = Some(PortState { port, clb_phys, ct_phys, dma_phys });

    use core::fmt::Write;
    let mut serial = crate::serial::SerialPort;
    let _ = writeln!(serial, "[AHCI] Port {} initialized", port);
    Ok(())
}

/// Read `count` consecutive sectors starting at `lba` into `buf`.
/// Up to 8 sectors (4 KB) per call.
pub fn read_sectors(lba: u64, count: u16, buf: &mut [u8]) -> Result<(), &'static str> {
    if count == 0 { return Ok(()); }
    if count > 8 { return Err("AHCI: max 8 sectors per call"); }
    let bytes = (count as usize) * 512;
    if buf.len() < bytes { return Err("AHCI: buf too small"); }

    let state_guard = PORT.lock();
    let state = state_guard.as_ref().ok_or("AHCI not initialized")?;
    let port = state.port;

    unsafe {
        let ct = &mut *(phys_to_virt(state.ct_phys as usize) as *mut CommandTable);

        // Zero the CFIS region; keep ACMD/reserved untouched.
        for b in ct.cfis.iter_mut() { *b = 0; }

        // H2D Register FIS, 5 dwords
        ct.cfis[0]  = 0x27;
        ct.cfis[1]  = 1 << 7;                     // C = 1 (command)
        ct.cfis[2]  = ATA_CMD_READ_DMA_EXT;
        ct.cfis[4]  =  (lba        & 0xFF) as u8;
        ct.cfis[5]  = ((lba >>  8) & 0xFF) as u8;
        ct.cfis[6]  = ((lba >> 16) & 0xFF) as u8;
        ct.cfis[7]  = 1 << 6;                     // LBA mode
        ct.cfis[8]  = ((lba >> 24) & 0xFF) as u8;
        ct.cfis[9]  = ((lba >> 32) & 0xFF) as u8;
        ct.cfis[10] = ((lba >> 40) & 0xFF) as u8;
        ct.cfis[12] =  (count       & 0xFF) as u8;
        ct.cfis[13] = ((count >> 8) & 0xFF) as u8;

        // One PRDT pointing at our DMA buffer
        ct.prdt[0].dba       = (state.dma_phys & 0xFFFF_FFFF) as u32;
        ct.prdt[0].dbau      = (state.dma_phys >> 32) as u32;
        ct.prdt[0]._reserved = 0;
        ct.prdt[0].dbc       = (bytes as u32) - 1;

        let hdr = &mut *(phys_to_virt(state.clb_phys as usize) as *mut CommandHeader);
        hdr.flags = 5;   // CFL = 5 dwords, W = 0 (read)
        hdr.prdtl = 1;
        hdr.prdbc = 0;

        // Wait for port to be idle
        for _ in 0..1_000_000 {
            if port_read(port, PX_TFD) & (TFD_BSY | TFD_DRQ) == 0 { break; }
        }

        port_write(port, PX_IS, 0xFFFF_FFFF);
        port_write(port, PX_CI, 1);

        let mut ok = false;
        for _ in 0..10_000_000 {
            if port_read(port, PX_IS) & IS_TFES != 0 { return Err("AHCI task file error"); }
            if port_read(port, PX_CI) & 1 == 0 { ok = true; break; }
        }
        if !ok { return Err("AHCI read timed out"); }
        if port_read(port, PX_TFD) & TFD_ERR != 0 { return Err("AHCI TFD error bit"); }

        let dma_virt = phys_to_virt(state.dma_phys as usize) as *const u8;
        core::ptr::copy_nonoverlapping(dma_virt, buf.as_mut_ptr(), bytes);
    }

    Ok(())
}

/// Write `count` consecutive sectors from `buf` to `lba`.
/// Up to 8 sectors (4 KB) per call.
pub fn write_sectors(lba: u64, count: u16, buf: &[u8]) -> Result<(), &'static str> {
    if count == 0 { return Ok(()); }
    if count > 8 { return Err("AHCI: max 8 sectors per call"); }
    let bytes = (count as usize) * 512;
    if buf.len() < bytes { return Err("AHCI: buf too small"); }

    let state_guard = PORT.lock();
    let state = state_guard.as_ref().ok_or("AHCI not initialized")?;
    let port = state.port;

    unsafe {
        // Copy caller's data into the DMA buffer before issuing the command.
        let dma_virt = phys_to_virt(state.dma_phys as usize) as *mut u8;
        core::ptr::copy_nonoverlapping(buf.as_ptr(), dma_virt, bytes);

        let ct = &mut *(phys_to_virt(state.ct_phys as usize) as *mut CommandTable);
        for b in ct.cfis.iter_mut() { *b = 0; }
        ct.cfis[0]  = 0x27;
        ct.cfis[1]  = 1 << 7;
        ct.cfis[2]  = ATA_CMD_WRITE_DMA_EXT;
        ct.cfis[4]  =  (lba        & 0xFF) as u8;
        ct.cfis[5]  = ((lba >>  8) & 0xFF) as u8;
        ct.cfis[6]  = ((lba >> 16) & 0xFF) as u8;
        ct.cfis[7]  = 1 << 6;
        ct.cfis[8]  = ((lba >> 24) & 0xFF) as u8;
        ct.cfis[9]  = ((lba >> 32) & 0xFF) as u8;
        ct.cfis[10] = ((lba >> 40) & 0xFF) as u8;
        ct.cfis[12] =  (count       & 0xFF) as u8;
        ct.cfis[13] = ((count >> 8) & 0xFF) as u8;

        ct.prdt[0].dba       = (state.dma_phys & 0xFFFF_FFFF) as u32;
        ct.prdt[0].dbau      = (state.dma_phys >> 32) as u32;
        ct.prdt[0]._reserved = 0;
        ct.prdt[0].dbc       = (bytes as u32) - 1;

        let hdr = &mut *(phys_to_virt(state.clb_phys as usize) as *mut CommandHeader);
        hdr.flags = 5 | (1 << 6);  // CFL=5, W=1 (write to device)
        hdr.prdtl = 1;
        hdr.prdbc = 0;

        for _ in 0..1_000_000 {
            if port_read(port, PX_TFD) & (TFD_BSY | TFD_DRQ) == 0 { break; }
        }
        port_write(port, PX_IS, 0xFFFF_FFFF);
        port_write(port, PX_CI, 1);

        let mut ok = false;
        for _ in 0..10_000_000 {
            if port_read(port, PX_IS) & IS_TFES != 0 { return Err("AHCI task file error"); }
            if port_read(port, PX_CI) & 1 == 0 { ok = true; break; }
        }
        if !ok { return Err("AHCI write timed out"); }
        if port_read(port, PX_TFD) & TFD_ERR != 0 { return Err("AHCI TFD error bit"); }
    }
    Ok(())
}

pub fn is_ready() -> bool {
    PORT.lock().is_some()
}
