use crate::cache::CacheData;
use crate::error::{Error, Result};

use bytes::Bytes;
use futures::StreamExt;
use futures::{Stream, TryStreamExt};
use std::fs;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use tokio::fs::OpenOptions;
use tokio::io::BufReader;
use tokio_util::codec;

async fn fs_persist(root_dir: &str, name: &str, data: &mut CacheData) {
    let mut path = PathBuf::from(root_dir);
    path.push(name);
    let parent_dirs = path.as_path().parent().unwrap();
    fs::create_dir_all(parent_dirs).unwrap();
    let mut f = fs::File::create(path).unwrap();
    match data {
        CacheData::ByteStream(stream, ..) => {
            while let Some(v) = stream.next().await {
                f.write_all(v.unwrap().as_ref()).unwrap()
            }
        }
        _ => f.write_all(data.as_ref()).unwrap(),
    }
}

/// Storage is an abstraction over a persistent storage.
/// - FileSystem: local filesystem
#[derive(Clone)]
pub enum Storage {
    FileSystem { root_dir: String },
}

pub async fn get_file_stream(path: &Path) -> Result<impl Stream<Item = Result<Bytes>>> {
    let f = OpenOptions::default().read(true).open(path).await?;
    let f = BufReader::new(f);
    let stream = codec::FramedRead::new(f, codec::BytesCodec::new())
        .map_ok(|bytes| bytes.freeze())
        .map_err(|e| e.into());
    Ok(stream)
}

impl Storage {
    pub async fn read(&self, name: &str) -> Result<CacheData> {
        match &self {
            Storage::FileSystem { root_dir, .. } => {
                let mut path = PathBuf::from(root_dir);
                path.push(name);
                match fs::metadata(&path) {
                    Ok(metadata) => {
                        let len = metadata.len(); // actually we don't need to know the size here
                        match get_file_stream(&path).await {
                            Ok(stream) => {
                                Ok(CacheData::ByteStream(Box::new(stream), Some(len as usize)))
                            }
                            Err(e) => Err(e),
                        }
                    }
                    Err(e) => Err(Error::IoError(e)),
                }
            }
        }
    }

    pub async fn persist(&self, name: &str, data: &mut CacheData) {
        match &self {
            Storage::FileSystem { root_dir, .. } => fs_persist(root_dir, name, data).await,
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
