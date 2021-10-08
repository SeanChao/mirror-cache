use crate::cache::{CacheData, CacheSizeType};
use crate::error::{Error, Result};

use bytes::Bytes;
use futures::{Stream, StreamExt, TryStreamExt};
use rusoto_core::{Region, RusotoError};
use rusoto_s3::{S3Client, S3};
use std::collections::HashMap;
use std::fs;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::vec::Vec;
use tokio::{fs::OpenOptions, io::BufReader, sync::RwLock};
use tokio_util::codec;

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
    S3 {
        endpoint: String,
        bucket: String,
    },
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
            Storage::S3 { endpoint, bucket } => {
                let client = new_s3_client(endpoint);
                let output = client
                    .get_object(rusoto_s3::GetObjectRequest {
                        bucket: bucket.clone(),
                        key: name.to_string(),
                        ..Default::default()
                    })
                    .await?;
                let rusoto_stream = output.body.unwrap();
                Ok(CacheData::ByteStream(
                    Box::new(rusoto_stream.map_err(Error::IoError)),
                    output.content_length.map(|x| x as CacheSizeType),
                ))
            }
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
            Storage::S3 {
                endpoint, bucket, ..
            } => {
                let client = new_s3_client(endpoint);
                let len = data.len();
                match client
                    .head_bucket(rusoto_s3::HeadBucketRequest {
                        bucket: bucket.clone(),
                        ..Default::default()
                    })
                    .await
                {
                    Ok(_) => {}
                    Err(e) => match e {
                        RusotoError::Unknown(resp) => {
                            if resp.status.as_u16() == 404 {
                                client
                                    .create_bucket(rusoto_s3::CreateBucketRequest {
                                        bucket: bucket.clone(),
                                        ..Default::default()
                                    })
                                    .await
                                    .unwrap();
                                debug!("created bucket {}", bucket)
                            }
                        }
                        _ => {
                            error!("{:?}", e);
                        }
                    },
                };

                match client
                    .put_object(rusoto_s3::PutObjectRequest {
                        bucket: bucket.clone(),
                        key: name.to_string(),
                        content_length: Some(len as i64),
                        body: Some(rusoto_s3::StreamingBody::new(
                            data.into_byte_stream()
                                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)),
                        )),
                        ..Default::default()
                    })
                    .await
                {
                    Ok(_) => {}
                    Err(e) => {
                        error!("Failed to persist object in S3: {:?}", e)
                    }
                }
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
            Storage::S3 {
                endpoint, bucket, ..
            } => {
                let client = new_s3_client(endpoint);
                client
                    .delete_object(rusoto_s3::DeleteObjectRequest {
                        bucket: bucket.clone(),
                        key: name.to_string(),
                        ..Default::default()
                    })
                    .await?;
                Ok(())
            }
        }
    }
    pub fn new_mem() -> Self {
        Storage::Memory {
            map: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn new_s3(endpoint: &str, bucket: &str) -> Self {
        Storage::S3 {
            endpoint: endpoint.to_string(),
            bucket: bucket.to_string(),
        }
    }
}

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

pub async fn get_file_stream(path: &Path) -> Result<impl Stream<Item = Result<Bytes>>> {
    let f = OpenOptions::default().read(true).open(path).await?;
    let f = BufReader::new(f);
    let stream = codec::FramedRead::new(f, codec::BytesCodec::new())
        .map_ok(|bytes| bytes.freeze())
        .map_err(|e| e.into());
    Ok(stream)
}

fn new_s3_client(endpoint: &str) -> S3Client {
    S3Client::new(Region::Custom {
        name: "s3 name".to_string(),
        endpoint: endpoint.to_string(),
    })
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
