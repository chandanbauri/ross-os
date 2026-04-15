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
}

pub struct VfsRoot {
    pub root_node: Option<Arc<dyn VfsNode>>,
}

pub static VFS: Mutex<VfsRoot> = Mutex::new(VfsRoot { root_node: None });

pub fn init(root: Arc<dyn VfsNode>) {
    let mut vfs = VFS.lock();
    vfs.root_node = Some(root);
}

pub fn open(path: &str) -> Result<Arc<dyn VfsNode>, ()> {
    let vfs = VFS.lock();
    let mut current = vfs.root_node.as_ref().ok_or(())?.clone();

    // Simple path splitting
    for component in path.split('/').filter(|s| !s.is_empty()) {
        current = current.finddir(component)?;
    }

    Ok(current)
}
