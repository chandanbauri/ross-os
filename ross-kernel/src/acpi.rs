// ACPI RSDP scan + MADT parse — discovers CPU count and LAPIC base address.

#[repr(C, packed)]
struct Rsdp {
    signature:    [u8; 8],
    checksum:     u8,
    oem_id:       [u8; 6],
    revision:     u8,
    rsdt_addr:    u32,
    // ACPI 2.0+ fields (revision >= 2)
    _length:      u32,
    xsdt_addr:    u64,
    _ext_checksum: u8,
    _reserved:    [u8; 3],
}

#[repr(C, packed)]
struct SdtHeader {
    signature:        [u8; 4],
    length:           u32,
    _revision:        u8,
    _checksum:        u8,
    _oem_id:          [u8; 6],
    _oem_table_id:    [u8; 8],
    _oem_revision:    u32,
    _creator_id:      u32,
    _creator_revision: u32,
}

#[repr(C, packed)]
struct Madt {
    header:     SdtHeader,
    lapic_addr: u32,
    _flags:     u32,
    // followed by variable-length IC structures
}

pub struct AcpiInfo {
    pub lapic_base: u64,
    pub ap_ids:     alloc::vec::Vec<u8>,
    pub bsp_id:     u8,
}

fn scan_range(start: u64, end: u64) -> Option<u64> {
    let mut addr = start & !0xF; // 16-byte align
    while addr + 8 <= end {
        let sig = unsafe { core::slice::from_raw_parts(addr as *const u8, 8) };
        if sig == b"RSD PTR " {
            return Some(addr);
        }
        addr += 16;
    }
    None
}

/// Find RSDP by scanning:
///  1. Legacy BIOS compatibility area (0xE0000–0xFFFFF) — BIOS/CSM
///  2. All Reserved regions in the boot memory map — EFI ACPI reclaim
fn find_rsdp(map: *const ross_common::MemoryRegion, map_len: usize) -> Option<u64> {
    // Method 1: legacy BIOS area
    if let Some(a) = scan_range(0xE_0000, 0x10_0000) {
        return Some(a);
    }

    // Method 2: scan Reserved memory regions from the bootloader memory map.
    // OVMF places ACPI tables in EfiACPIReclaimMemory which we tag as Reserved.
    // Skip MMIO above 0xD000_0000 (PCI/LAPIC space) to avoid side-effects.
    if !map.is_null() && map_len > 0 {
        let regions = unsafe { core::slice::from_raw_parts(map, map_len) };
        for r in regions {
            if r.kind != ross_common::MemoryKind::Reserved { continue; }
            if r.base >= 0xD000_0000 { continue; } // skip MMIO/firmware area
            if r.size > 8 * 1024 * 1024 { continue; } // skip huge regions
            if let Some(a) = scan_range(r.base, r.base + r.size) {
                return Some(a);
            }
        }
    }
    None
}

fn find_table_xsdt(xsdt_phys: u64, sig: &[u8; 4]) -> Option<u64> {
    let ptr = xsdt_phys as *const u8;
    let header = unsafe { &*(ptr as *const SdtHeader) };
    let total = { header.length } as usize;
    let hdr_size = core::mem::size_of::<SdtHeader>();
    let n_entries = (total.saturating_sub(hdr_size)) / 8;
    let entries = unsafe {
        core::slice::from_raw_parts(ptr.add(hdr_size) as *const u64, n_entries)
    };
    for &entry_phys in entries {
        let ep = entry_phys as *const u8;
        let esig = unsafe { core::slice::from_raw_parts(ep, 4) };
        if esig == sig { return Some(entry_phys); }
    }
    None
}

fn find_table_rsdt(rsdt_phys: u64, sig: &[u8; 4]) -> Option<u64> {
    let ptr = rsdt_phys as *const u8;
    let header = unsafe { &*(ptr as *const SdtHeader) };
    let total = { header.length } as usize;
    let hdr_size = core::mem::size_of::<SdtHeader>();
    let n_entries = (total.saturating_sub(hdr_size)) / 4;
    let entries = unsafe {
        core::slice::from_raw_parts(ptr.add(hdr_size) as *const u32, n_entries)
    };
    for &entry_phys in entries {
        let ep = entry_phys as *const u8;
        let esig = unsafe { core::slice::from_raw_parts(ep, 4) };
        if esig == sig { return Some(entry_phys as u64); }
    }
    None
}

fn parse_madt(madt_phys: u64) -> (u64, alloc::vec::Vec<u8>) {
    let ptr = madt_phys as *const u8;
    let madt = unsafe { &*(ptr as *const Madt) };
    let lapic_base = { madt.lapic_addr } as u64;
    let total = { madt.header.length } as usize;

    let mut apic_ids = alloc::vec::Vec::new();
    let mut off = core::mem::size_of::<Madt>();
    while off + 2 <= total {
        let ic_type = unsafe { ptr.add(off).read() };
        let ic_len  = unsafe { ptr.add(off + 1).read() } as usize;
        if ic_len < 2 { break; }

        if ic_type == 0 && ic_len >= 8 {
            // Processor Local APIC: [type, len, acpi_uid, apic_id, flags:u32]
            let apic_id = unsafe { ptr.add(off + 3).read() };
            let flags   = unsafe { (ptr.add(off + 4) as *const u32).read_unaligned() };
            if flags & 1 != 0 {
                apic_ids.push(apic_id);
            }
        }
        off += ic_len;
    }
    (lapic_base, apic_ids)
}

pub fn init(map: *const ross_common::MemoryRegion, map_len: usize) -> Option<AcpiInfo> {
    let rsdp_phys = find_rsdp(map, map_len)?;
    crate::serial::serial_print("[ACPI] RSDP found\n");

    let rsdp = unsafe { &*(rsdp_phys as *const Rsdp) };

    let madt_phys = if rsdp.revision >= 2 {
        let xsdt = { rsdp.xsdt_addr };
        find_table_xsdt(xsdt, b"APIC")
    } else {
        let rsdt = { rsdp.rsdt_addr } as u64;
        find_table_rsdt(rsdt, b"APIC")
    }?;

    let (lapic_base, apic_ids) = parse_madt(madt_phys);

    // Read BSP's LAPIC ID from IA32_APIC_BASE MSR area (LAPIC ID register at base+0x20)
    let bsp_id = unsafe {
        let id_reg = (lapic_base + 0x20) as *const u32;
        ((*id_reg) >> 24) as u8
    };

    use core::fmt::Write;
    let _ = write!(crate::serial::SerialPort, "[ACPI] {} CPUs, LAPIC @ {:#x}, BSP ID {}\n",
        apic_ids.len(), lapic_base, bsp_id);

    Some(AcpiInfo { lapic_base, ap_ids: apic_ids, bsp_id })
}
