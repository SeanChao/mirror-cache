use crate::cache::CacheData;
use crate::error::Result;

use bytes::Bytes;
use std::fs;
use std::io::prelude::*;
use std::path::PathBuf;

fn fs_persist(root_dir: &str, name: &str, data: &CacheData) {
    let mut path = PathBuf::from(root_dir);
    path.push(name);
    let parent_dirs = path.as_path().parent().unwrap();
    fs::create_dir_all(parent_dirs).unwrap();
    let mut f = fs::File::create(path).unwrap();
    f.write_all(data.as_ref()).unwrap();
}

/// Storage is an abstraction over a persistent storage.
/// - FileSystem: local filesystem
pub enum Storage {
    FileSystem { root_dir: String },
}

impl Storage {
    pub fn read(&self, name: &str) -> Result<CacheData> {
        match &self {
            Storage::FileSystem { root_dir, .. } => {
                let mut path = PathBuf::from(root_dir);
                path.push(name);
                Ok(Bytes::from(fs::read(path).unwrap()).into())
            }
        }
    }

    pub fn persist(&self, name: &str, data: &CacheData) {
        match &self {
            Storage::FileSystem { root_dir, .. } => fs_persist(&root_dir, name, data),
        }
    }

    pub fn remove(&self, name: &str) -> Result<()> {
        match &self {
            Storage::FileSystem { root_dir, .. } => {
                let mut path = PathBuf::from(root_dir);
                path.push(name);
                fs::remove_file(path).map_err(|e| e.into())
            }
        }
    }
}
