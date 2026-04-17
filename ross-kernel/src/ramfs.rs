use alloc::vec::Vec;
use alloc::string::String;
use alloc::sync::Arc;
use core::str;
use crate::vfs::{VfsNode, Stat, NodeType};

#[repr(C)]
struct TarHeader {
    name: [u8; 100],
    mode: [u8; 8],
    uid: [u8; 8],
    gid: [u8; 8],
    size: [u8; 12],
    mtime: [u8; 12],
    checksum: [u8; 8],
    _typeflag: u8,
    linkname: [u8; 100],
    magic: [u8; 6],
    version: [u8; 2],
    uname: [u8; 32],
    gname: [u8; 32],
    devmajor: [u8; 8],
    devminor: [u8; 8],
    _prefix: [u8; 155],
}

pub struct TarFile {
    pub name: String,
    pub data: &'static [u8],
}

impl VfsNode for TarFile {
    fn attribute(&self) -> Stat {
        Stat {
            size: self.data.len(),
            node_type: NodeType::File,
        }
    }

    fn read(&self, offset: usize, buffer: &mut [u8]) -> Result<usize, ()> {
        if offset >= self.data.len() {
            return Ok(0);
        }
        let available = self.data.len() - offset;
        let to_copy = core::cmp::min(available, buffer.len());
        buffer[..to_copy].copy_from_slice(&self.data[offset..offset + to_copy]);
        Ok(to_copy)
    }

    fn readdir(&self) -> Result<Vec<String>, ()> {
        Err(()) // Not a directory
    }

    fn finddir(&self, _name: &str) -> Result<Arc<dyn VfsNode>, ()> {
        Err(()) // Not a directory
    }
}



pub struct TarFileSystem {
    pub files: Vec<Arc<TarFile>>,
}

impl TarFileSystem {
    pub fn new(data: &'static [u8]) -> Self {
        let mut files = Vec::new();
        let mut offset = 0;

        while offset + 512 <= data.len() {
            let header = unsafe { &*(data.as_ptr().add(offset) as *const TarHeader) };
            
            // Check if magic is "ustar"
            if &header.magic[0..5] != b"ustar" {
                break;
            }

            let mut name = str::from_utf8(&header.name).unwrap_or("").trim_matches('\0');
            if name.starts_with("./") {
                name = &name[2..];
            }

            // Parse size (octal)
            let size_str = str::from_utf8(&header.size).unwrap_or("0");
            let size = usize::from_str_radix(size_str.trim_matches('\0').trim(), 8).unwrap_or(0);

            if name.is_empty() || name == "." {
                // Skip empty or root directory entries (padded to 512 byte blocks)
                offset += 512 + ((size + 511) & !511);
                continue;
            }

            let file_data = &data[offset + 512 .. offset + 512 + size];
            
            files.push(Arc::new(TarFile {
                name: String::from(name),
                data: file_data,
            }));

            // TAR files are padded to 512 byte blocks
            offset += 512 + ((size + 511) & !511);
        }

        Self { files }
    }
}

impl VfsNode for TarFileSystem {
    fn attribute(&self) -> Stat {
        Stat { size: 0, node_type: NodeType::Directory }
    }

    fn read(&self, _offset: usize, _buffer: &mut [u8]) -> Result<usize, ()> { Err(()) }
    fn readdir(&self) -> Result<Vec<String>, ()> {
        Ok(self.files.iter().map(|f| f.name.clone()).collect())
    }

    fn finddir(&self, name: &str) -> Result<Arc<dyn VfsNode>, ()> {
        for file in &self.files {
            if file.name == name {
                return Ok(file.clone() as Arc<dyn VfsNode>);
            }
        }
        Err(())
    }
}
