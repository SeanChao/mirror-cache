use crate::cache::CacheData;
use crate::error::{Error, Result};

use bytes::Bytes;
use futures::StreamExt;
use futures::{Stream, TryStreamExt};
use std::collections::HashMap;
use std::fs;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::vec::Vec;
use tokio::{fs::OpenOptions, io::BufReader, sync::RwLock};
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
    FileSystem {
        root_dir: String,
    },
    Memory {
        map: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    },
}

impl Storage {
    pub fn new_mem() -> Self {
        Storage::Memory {
            map: Arc::new(RwLock::new(HashMap::new())),
        }
    }
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
                            Ok(stream) => Ok(CacheData::ByteStream(Box::new(stream), Some(len))),
                            Err(e) => Err(e),
                        }
                    }
                    Err(e) => Err(Error::IoError(e)),
                }
            }
            Storage::Memory { map, .. } => map.read().await.get(name).map_or(
                Err(Error::IoError(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "No such key.",
                ))),
                |x| Ok(x.clone().into()),
            ),
        }
    }

    pub async fn persist(&self, name: &str, mut data: CacheData) {
        match self {
            Storage::FileSystem { root_dir, .. } => fs_persist(root_dir, name, &mut data).await,
            Storage::Memory { ref map, .. } => {
                map.write()
                    .await
                    .insert(name.to_string(), data.into_vec_u8().await);
            }
        }
    }

    pub async fn remove(&self, name: &str) -> Result<()> {
        match self {
            Storage::FileSystem { ref root_dir, .. } => {
                let mut path = PathBuf::from(root_dir);
                path.push(name);
                fs::remove_file(path).map_err(|e| e.into())
            }
            Storage::Memory { map, .. } => {
                map.write().await.remove(name);
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    async fn write_read(storage: &mut Storage) {
        let name = "write_read_test";
        let data = "Metaphysics includes cosmosology and ontology.";
        storage.persist(name, String::from(data).into()).await;
        let data_read: Vec<u8> = storage.read(name).await.unwrap().into_vec_u8().await;
        assert_eq!(data.as_bytes().to_vec(), data_read);
    }

    async fn remove(storage: &mut Storage) {
        let name = "remove_test";
        storage.persist(name, String::from("wow").into()).await;
        storage.remove(name).await.unwrap();
        assert!(storage.read(name).await.is_err());
    }

    #[tokio::test]
    async fn test_fs_write_read() {
        let mut storage = Storage::FileSystem {
            root_dir: "cache/storage_test".to_string(),
        };
        write_read(&mut storage).await;
    }

    #[tokio::test]
    async fn test_fs_remove() {
        let mut storage = Storage::FileSystem {
            root_dir: "cache/test_fs_remove".to_string(),
        };
        remove(&mut storage).await;
    }

    #[tokio::test]
    async fn test_mem_write_read() {
        let mut storage = Storage::new_mem();
        write_read(&mut storage).await;
    }

    #[tokio::test]
    async fn test_mem_remove() {
        let mut storage = Storage::new_mem();
        remove(&mut storage).await;
    }
}
