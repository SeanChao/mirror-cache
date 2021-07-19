use redis::Commands;
use std::fs;
use std::io::prelude::*;
use std::marker::Send;
use std::vec::Vec;

use crate::models;
use crate::util;

pub type BytesCacheEntry = Vec<u8>;
pub trait CachePolicy: Sync + Send {
    fn put(&self, key: &str, entry: BytesCacheEntry);
    fn get(&self, key: String) -> Option<BytesCacheEntry>;
}

#[derive(Clone)]
pub struct LruRedisCache {
    root_dir: String,
    pub size_limit: u64, // cache size in bytes(B)
    redis_client: redis::Client,
}

impl LruRedisCache {
    pub fn new(root_dir: &str, size_limit: u64, redis_client: redis::Client) -> Self {
        println!("LRU Redis Cache init: size_limit={}", size_limit);
        Self {
            root_dir: String::from(root_dir),
            size_limit: size_limit,
            redis_client: redis_client,
        }
    }
}

impl CachePolicy for LruRedisCache {
    fn put(&self, key: &str, entry: BytesCacheEntry) {
        // eviction policy
        let file_size = entry.len() as u64;
        let mut sync_con = models::get_sync_con(&self.redis_client).unwrap();

        if file_size > self.size_limit {
            println!(
                "skip cache for {:?}, because its size exceeds the limit",
                entry
            );
        }
        // evict cache entry if necessary
        let _tx_result = redis::transaction(&mut sync_con, &[key, "total_size"], |con, pipe| {
            let mut cur_cache_size = con
                .get::<&str, Option<u64>>("total_size")
                .unwrap()
                .unwrap_or(0);
            while cur_cache_size + file_size > self.size_limit {
                // LRU eviction
                println!(
                    "current {} + new {} > limit {}",
                    con.get::<&str, Option<u64>>("total_size")
                        .unwrap()
                        .unwrap_or(0),
                    file_size,
                    self.size_limit
                );
                let pkg_to_remove: Vec<(String, u64)> = con.zpopmax("cache_keys", 1).unwrap();
                println!("pkg_to_remove: {:?}", pkg_to_remove);
                if pkg_to_remove.is_empty() {
                    println!("some files need to be evicted but they are missing from redis filelist. The cache metadata is inconsistent.");
                    return Err(redis::RedisError::from(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "cache metadata inconsistent",
                    )));
                }
                // remove from local fs and metadata in redis
                for (f, _) in pkg_to_remove {
                    let path = format!("cache/{}", f);
                    println!("remove: {}", path);
                    match fs::remove_file(path) {
                        Ok(_) => {}
                        Err(e) => {
                            println!("failed to remove file: {:?}", e);
                        }
                    };
                    let pkg_size: Option<u64> = con.hget(&f, "size").unwrap();
                    let _del_cnt = con.del::<&str, isize>(&f);
                    println!("total_size -= {:?}", pkg_size);
                    let qres = pipe
                        .decr("total_size", pkg_size.unwrap_or(0))
                        .get("total_size")
                        .query(con)?;
                    println!("qres {:?}", qres);
                    cur_cache_size = 0;
                }
            }
            Ok(Some(()))
        });
        // cache to local filesystem
        let data_to_write = entry;
        let (parent_dirs, _cache_file_name) = util::split_dirs(key);
        fs::create_dir_all(format!("cache/{}", parent_dirs)).unwrap();
        let mut f = fs::File::create(format!("cache/{}", key)).unwrap();
        f.write_all(&data_to_write).unwrap();
        let _redis_resp_str = models::set_cache_entry(
            &mut sync_con,
            &key,
            &CacheEntry::new(&key, data_to_write.len() as u64),
        );
    }

    fn get(&self, key: String) -> Option<BytesCacheEntry> {
        let mut sync_con = models::get_sync_con(&self.redis_client).unwrap();
        let cache_result = models::get_cache_entry(&mut sync_con, &key).unwrap();
        println!("[HMGET] {} -> {:?}", &key, &cache_result);
        if let Some(_cache_entry) = cache_result {
            // cache hit
            // update cache entry in db
            match models::update_cache_entry_atime(&mut sync_con, &key, util::now()) {
                Ok(_) => {}
                Err(e) => {
                    println!("Failed to update cache entry atime: {}", e);
                }
            }
            let cached_file_path = format!("cache/{}", key);
            let file_content = match fs::read(cached_file_path) {
                Ok(data) => data,
                Err(_) => vec![],
            };
            if file_content.len() > 0 {
                println!("cache hit");
                return Some(file_content);
            }
        };
        None
    }
}

#[derive(Hash, Eq, PartialEq, Debug)]
pub struct CacheEntry<Metadata, Key, Value> {
    pub metadata: Metadata,
    pub key: Key,
    pub value: Value,
}

#[derive(Debug)]
pub struct LruCacheMetadata {
    pub size: u64,
    pub atime: i64, // last access timestamp
}

impl CacheEntry<LruCacheMetadata, String, ()> {
    pub fn new(path: &str, size: u64) -> CacheEntry<LruCacheMetadata, String, ()> {
        CacheEntry {
            metadata: LruCacheMetadata {
                size: size,
                atime: util::now(),
            },
            key: String::from(path),
            value: (),
        }
    }

    /**
     * Convert a cache entry to an array keys and values to be stored as redis hash
     */
    pub fn to_redis_multiple_fields(&self) -> Vec<(&str, String)> {
        vec![
            ("path", self.key.clone()),
            ("size", self.metadata.size.to_string()),
            ("atime", self.metadata.atime.to_string()),
        ]
    }
}
