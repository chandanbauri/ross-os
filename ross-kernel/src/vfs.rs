use alloc::vec::Vec;
use alloc::string::String;
use alloc::sync::Arc;
use spin::Mutex;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodeType {
    File,
    Directory,
}

pub struct Stat {
    pub size: usize,
    pub node_type: NodeType,
}

pub trait VfsNode: Send + Sync {
    fn attribute(&self) -> Stat;
    fn read(&self, offset: usize, buffer: &mut [u8]) -> Result<usize, ()>;
    fn readdir(&self) -> Result<Vec<String>, ()>;
    fn finddir(&self, name: &str) -> Result<Arc<dyn VfsNode>, ()>;

    // Default: read-only.  FAT32 overrides both.
    fn write(&self, _offset: usize, _data: &[u8]) -> Result<usize, ()> { Err(()) }
    fn create(&self, _name: &str) -> Result<Arc<dyn VfsNode>, ()> { Err(()) }
}

pub struct VfsRoot {
    pub root_node: Option<Arc<dyn VfsNode>>,
    /// Persistent FAT32 disk mounted at `/mnt/disk`. Set by `mount_disk`.
    pub disk_root: Option<Arc<dyn VfsNode>>,
}

pub static VFS: Mutex<VfsRoot> = Mutex::new(VfsRoot {
    root_node: None,
    disk_root: None,
});

pub fn init(root: Arc<dyn VfsNode>) {
    let mut vfs = VFS.lock();
    vfs.root_node = Some(root);
}

/// Mount the persistent disk at `/mnt/disk`.
pub fn mount_disk(root: Arc<dyn VfsNode>) {
    let mut vfs = VFS.lock();
    vfs.disk_root = Some(root);
}

/// Create a new file at `path`; the parent directory must already exist.
pub fn create(path: &str) -> Result<Arc<dyn VfsNode>, ()> {
    let trimmed = path.trim_start_matches('/');
    let (parent_path, name) = match trimmed.rfind('/') {
        Some(pos) => (&trimmed[..pos], &trimmed[pos+1..]),
        None      => ("", trimmed),
    };
    let parent = if parent_path.is_empty() { open("/") } else { open(parent_path) }?;
    parent.create(name)
}

pub fn open(path: &str) -> Result<Arc<dyn VfsNode>, ()> {
    let vfs = VFS.lock();
    let trimmed = path.trim_start_matches('/');

    // Dispatch `/mnt/disk/...` to the FAT32 mount if present.
    if let Some(rest) = trimmed.strip_prefix("mnt/disk") {
        let mut current = vfs.disk_root.as_ref().ok_or(())?.clone();
        let rest = rest.trim_start_matches('/');
        for component in rest.split('/').filter(|s| !s.is_empty()) {
            current = current.finddir(component)?;
        }
        return Ok(current);
    }

    let mut current = vfs.root_node.as_ref().ok_or(())?.clone();
    for component in trimmed.split('/').filter(|s| !s.is_empty()) {
        current = current.finddir(component)?;
    }
    Ok(current)
}
