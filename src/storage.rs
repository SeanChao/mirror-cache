use crate::cache::CacheData;
use crate::error::Result;

use bytes::Bytes;
use futures::StreamExt;
use std::fs;
use std::io::prelude::*;
use std::path::PathBuf;

async fn fs_persist(root_dir: &str, name: &str, data: &mut CacheData) {
    let mut path = PathBuf::from(root_dir);
    path.push(name);
    let parent_dirs = path.as_path().parent().unwrap();
    fs::create_dir_all(parent_dirs).unwrap();
    let mut f = fs::File::create(path).unwrap();
    match data {
        CacheData::ByteStream(stream) => {
            while let Some(v) = stream.next().await {
                f.write_all(v.unwrap().as_ref()).unwrap()
            }
        }
        _ => f.write_all(data.as_ref()).unwrap(),
    }
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
                match fs::read(path) {
                    Ok(vec) => Ok(Bytes::from(vec).into()),
                    Err(e) => Err(e.into()),
                }
            }
        }
    }

    pub async fn persist(&self, name: &str, data: &mut CacheData) {
        match &self {
            Storage::FileSystem { root_dir, .. } => fs_persist(&root_dir, name, data).await,
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
