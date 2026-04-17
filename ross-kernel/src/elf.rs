#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ElfHeader {
    pub magic: [u8; 4],
    pub class: u8,
    pub data: u8,
    pub version: u8,
    pub os_abi: u8,
    pub abi_version: u8,
    pub pad: [u8; 7],
    pub e_type: u16,
    pub e_machine: u16,
    pub e_version: u32,
    pub e_entry: u64,
    pub e_phoff: u64,
    pub e_shoff: u64,
    pub e_flags: u32,
    pub e_ehsize: u16,
    pub e_phentsize: u16,
    pub e_phnum: u16,
    pub e_shentsize: u16,
    pub e_shnum: u16,
    pub e_shstrndx: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ProgramHeader {
    pub p_type: u32,
    pub p_flags: u32,
    pub p_offset: u64,
    pub p_vaddr: u64,
    pub p_paddr: u64,
    pub p_filesz: u64,
    pub p_memsz: u64,
    pub p_align: u64,
}

pub const PT_LOAD: u32 = 1;

impl ElfHeader {
    pub fn is_valid(&self) -> bool {
        self.magic == [0x7f, b'E', b'L', b'F'] && self.class == 2 // 64-bit
    }

    pub fn program_headers<'a>(&self, data: &'a [u8]) -> &'a [ProgramHeader] {
        let start = self.e_phoff as usize;
        let count = self.e_phnum as usize;
        let size = self.e_phentsize as usize;
        
        unsafe {
            core::slice::from_raw_parts(
                data.as_ptr().add(start) as *const ProgramHeader,
                count
            )
        }
    }
}
