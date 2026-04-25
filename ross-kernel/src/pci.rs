//! PCI bus enumeration via legacy port I/O (0xCF8 / 0xCFC).
//!
//! Scans all buses/devices/functions once at boot and caches the registry
//! in `DEVICES`. Later drivers (AHCI, NIC, ...) look up hardware by class.

use alloc::vec::Vec;
use spin::Mutex;

const CONFIG_ADDRESS: u16 = 0xCF8;
const CONFIG_DATA:    u16 = 0xCFC;

#[derive(Debug, Clone, Copy)]
pub struct PciDevice {
    pub bus:         u8,
    pub device:      u8,
    pub function:    u8,
    pub vendor_id:   u16,
    pub device_id:   u16,
    pub class:       u8,
    pub subclass:    u8,
    pub prog_if:     u8,
    pub header_type: u8,
}

impl PciDevice {
    /// Read one of the six 32-bit BARs (index 0..=5).
    pub fn read_bar(&self, index: u8) -> u32 {
        assert!(index < 6);
        read_config_dword(self.bus, self.device, self.function, 0x10 + index * 4)
    }
}

pub static DEVICES: Mutex<Vec<PciDevice>> = Mutex::new(Vec::new());

fn address(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    0x8000_0000
        | ((bus as u32)      << 16)
        | ((device as u32)   << 11)
        | ((function as u32) << 8)
        | ((offset as u32) & 0xFC)
}

fn read_config_dword(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let addr = address(bus, device, function, offset);
    let val: u32;
    unsafe {
        core::arch::asm!(
            "out dx, eax",
            in("dx")  CONFIG_ADDRESS,
            in("eax") addr,
            options(nomem, nostack, preserves_flags)
        );
        core::arch::asm!(
            "in eax, dx",
            out("eax") val,
            in("dx")   CONFIG_DATA,
            options(nomem, nostack, preserves_flags)
        );
    }
    val
}

fn read_config_word(bus: u8, device: u8, function: u8, offset: u8) -> u16 {
    let dw = read_config_dword(bus, device, function, offset & 0xFC);
    let shift = (offset & 2) * 8;
    ((dw >> shift) & 0xFFFF) as u16
}

fn read_config_byte(bus: u8, device: u8, function: u8, offset: u8) -> u8 {
    let dw = read_config_dword(bus, device, function, offset & 0xFC);
    let shift = (offset & 3) * 8;
    ((dw >> shift) & 0xFF) as u8
}

fn probe(bus: u8, device: u8, function: u8) -> Option<PciDevice> {
    let vendor_id = read_config_word(bus, device, function, 0x00);
    if vendor_id == 0xFFFF { return None; }

    Some(PciDevice {
        bus, device, function,
        vendor_id,
        device_id:   read_config_word(bus, device, function, 0x02),
        prog_if:     read_config_byte(bus, device, function, 0x09),
        subclass:    read_config_byte(bus, device, function, 0x0A),
        class:       read_config_byte(bus, device, function, 0x0B),
        header_type: read_config_byte(bus, device, function, 0x0E),
    })
}

pub fn enumerate() {
    let mut devices = DEVICES.lock();
    devices.clear();

    for bus in 0..=255u16 {
        for dev in 0..32u8 {
            let Some(d0) = probe(bus as u8, dev, 0) else { continue };
            let multi_function = d0.header_type & 0x80 != 0;
            devices.push(d0);
            if multi_function {
                for func in 1..8u8 {
                    if let Some(d) = probe(bus as u8, dev, func) {
                        devices.push(d);
                    }
                }
            }
        }
    }

    use core::fmt::Write;
    let mut serial = crate::serial::SerialPort;
    let _ = writeln!(serial, "[PCI] Enumerated {} devices", devices.len());
}

/// Find the first device matching `class`/`subclass`. Returned by value so
/// callers don't need to hold the DEVICES lock.
pub fn find_by_class(class: u8, subclass: u8) -> Option<PciDevice> {
    DEVICES.lock().iter()
        .find(|d| d.class == class && d.subclass == subclass)
        .copied()
}

pub fn class_name(class: u8, subclass: u8) -> &'static str {
    match (class, subclass) {
        (0x00, _)    => "Unclassified",
        (0x01, 0x00) => "SCSI Controller",
        (0x01, 0x01) => "IDE Controller",
        (0x01, 0x06) => "SATA Controller (AHCI)",
        (0x01, 0x08) => "NVMe Controller",
        (0x02, 0x00) => "Ethernet Controller",
        (0x03, 0x00) => "VGA Controller",
        (0x04, _)    => "Multimedia Controller",
        (0x06, 0x00) => "Host Bridge",
        (0x06, 0x01) => "ISA Bridge",
        (0x06, 0x04) => "PCI-to-PCI Bridge",
        (0x0C, 0x03) => "USB Controller",
        _            => "Unknown",
    }
}
