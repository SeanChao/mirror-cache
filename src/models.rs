use crate::cache::CacheEntry;
use crate::cache::LruCacheMetadata;
use crate::error::Error::*;
use crate::error::Result;
use redis::{aio::Connection, Commands, Connection as SyncConnection};
use sled::transaction::TransactionalTree;
use std::collections::HashMap;
use std::convert::{From, TryInto};

#[allow(dead_code)]
pub async fn get_con(client: &redis::Client) -> Result<Connection> {
    client
        .get_async_connection()
        .await
        .map_err(RedisClientError)
}

pub fn get_sync_con(client: &redis::Client) -> Result<SyncConnection> {
    client.get_connection().map_err(RedisClientError)
}

/// get cache entry
pub fn get_cache_entry(
    con: &mut SyncConnection,
    key: &str,
) -> Result<Option<CacheEntry<LruCacheMetadata, String, ()>>> {
    let map: HashMap<String, String> = con.hgetall(key)?;
    if map.is_empty() {
        // not exist in cache
        return Ok(None);
    }
    let cache_entry = CacheEntry {
        metadata: LruCacheMetadata {
            atime: String::from(map.get("atime").unwrap_or(&String::from("0")))
                .parse::<i64>()
                .unwrap_or(0),
            size: String::from(map.get("size").unwrap_or(&String::from("0")))
                .parse::<u64>()
                .unwrap_or(0),
        },
        key: String::from(map.get("path").unwrap_or(&String::from(""))),
        value: (),
    };
    Ok(Some(cache_entry))
}

/// set an lru cache entry
pub fn set_lru_cache_entry(
    con: &mut SyncConnection,
    key: &str,
    entry: &CacheEntry<LruCacheMetadata, String, ()>,
    total_size_key: &str,
    zlist_key: &str,
) -> Result<()> {
    let kv_array = entry.to_redis_multiple_fields();
    let tx_result = redis::transaction(con, &[key, total_size_key, zlist_key], |con, pipe| {
        let pkg_size: Option<u64> = con.hget(&key, "size").unwrap();
        pipe.decr(
            total_size_key,
            if let Some(old_size) = pkg_size {
                old_size
            } else {
                0
            },
        )
        .incr(total_size_key, entry.metadata.size)
        .hset_multiple::<&str, &str, String>(key, &kv_array)
        .ignore()
        .zadd(zlist_key, key, entry.metadata.atime)
        .query(con)?;
        Ok(Some(()))
    });
    tx_result.map_err(RedisCMDError)
}

pub fn update_cache_entry_atime(
    con: &mut SyncConnection,
    key: &str,
    atime: i64,
    zlist_key: &str,
) -> Result<i64> {
    match redis::pipe()
        .atomic()
        .hset(key, "atime", atime)
        .zadd(zlist_key, key, atime)
        .query::<(i64, i64)>(con)
    {
        Ok(_) => Ok(atime),
        Err(e) => Err(RedisCMDError(e)),
    }
}

pub fn set(con: &mut SyncConnection, key: &str, value: &str) -> Result<String> {
    match con.set(key, value) {
        Ok(res) => Ok(res),
        Err(e) => Err(RedisCMDError(e)),
    }
}

pub fn get(con: &mut SyncConnection, key: &str) -> Result<Option<String>> {
    match con.get(key) {
        Ok(val) => Ok(val),
        Err(e) => Err(RedisCMDError(e)),
    }
}

/**
 * Set the TTL of given key
 */
pub fn expire(con: &mut SyncConnection, key: &str, ttl: usize) -> Result<i32> {
    match con.expire(key, ttl) {
        Ok(res) => Ok(res),
        Err(e) => Err(RedisCMDError(e)),
    }
}

pub struct SledMetadata {
    pub atime: i64,
    pub size: u64,
}

impl From<sled::IVec> for SledMetadata {
    fn from(vec: sled::IVec) -> Self {
        Self {
            atime: i64::from_be_bytes(vec.subslice(0, 8).as_ref().try_into().unwrap()),
            size: u64::from_be_bytes(vec.subslice(8, 8).as_ref().try_into().unwrap()),
        }
    }
}

impl From<SledMetadata> for sled::IVec {
    fn from(metadata: SledMetadata) -> Self {
        [metadata.atime.to_be_bytes(), metadata.size.to_be_bytes()]
            .concat()
            .into()
    }
}

/// Update the atime for the given cache key.
/// This should be called within a transaction context to ensure atomicity.
pub fn sled_update_cache_entry_atime(
    metadata_tree: &TransactionalTree,
    atime_tree: &TransactionalTree,
    key: &str,
    atime: i64,
) {
    let old_entry: SledMetadata = metadata_tree.get(key).unwrap().unwrap().into();
    let old_atime = old_entry.atime;
    atime_tree.remove(&old_atime.to_be_bytes()).unwrap();
    let new_metadata = SledMetadata {
        atime,
        size: old_entry.size,
    };
    metadata_tree.insert(key, new_metadata).unwrap();
    atime_tree.insert(&atime.to_be_bytes(), key).unwrap();
}

pub fn sled_insert_cache_entry(
    db: &TransactionalTree,
    prefix: &str,
    metadata_tree: &TransactionalTree,
    atime_tree: &TransactionalTree,
    key: &str,
    size: u64,
    atime: i64,
) {
    match metadata_tree.insert(key, SledMetadata { atime, size }) {
        Ok(Some(old_entry)) => {
            // remove old entry in atime_tree
            let old_entry: SledMetadata = old_entry.into();
            atime_tree.remove(&old_entry.atime.to_be_bytes()).unwrap();
            sled_lru_set_current_size(
                db,
                prefix,
                sled_lru_get_current_size(db, prefix) - old_entry.size,
            );
            trace!(
                "sled_insert_cache_entry updated entry {} -> ({} , {})",
                key,
                atime,
                size
            );
        }
        Ok(None) => {
            trace!(
                "sled_insert_cache_entry inserted entry {} -> ({} , {})",
                key,
                atime,
                size
            );
        }
        Err(e) => {
            error!("Error inserting cache entry: {}", e);
        }
    };
    atime_tree.insert(&atime.to_be_bytes(), key).unwrap();
}

pub fn sled_lru_get_current_size(db: &sled::transaction::TransactionalTree, prefix: &str) -> u64 {
    let total_size_raw: [u8; 8] = db
        .get(format!("{}_total_size", prefix))
        .unwrap()
        .unwrap()
        .as_ref()
        .try_into()
        .unwrap();
    u64::from_be_bytes(total_size_raw)
}

pub fn sled_lru_get_current_size_notx(db: &sled::Db, prefix: &str) -> u64 {
    let total_size_raw: [u8; 8] = db
        .get(format!("{}_total_size", prefix))
        .unwrap()
        .unwrap()
        .as_ref()
        .try_into()
        .unwrap();
    u64::from_be_bytes(total_size_raw)
}

pub fn sled_lru_set_current_size(
    db: &sled::transaction::TransactionalTree,
    prefix: &str,
    size: u64,
) {
    db.insert(
        format!("{}_total_size", prefix).as_str(),
        &size.to_be_bytes(),
    )
    .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use sled::IVec;
    use std::thread;

    fn new_redis_client() -> redis::Client {
        redis::Client::open("redis://localhost:3001/")
            .expect("Failed to connect to redis server (test)")
    }

    #[test]
    fn test_key_value_set_success() {
        let client = new_redis_client();
        let mut con = client.get_connection().unwrap();
        let key = "IkariShinji";
        let val = "Kaworu";
        set(&mut con, key, val).unwrap();
        thread::sleep(std::time::Duration::from_millis(500));
        let val_actual = get(&mut con, key).unwrap().unwrap();
        assert_eq!(val_actual, val);
    }

    #[test]
    fn test_get_nonexist_key() {
        let client = new_redis_client();
        let mut con = client.get_connection().unwrap();
        let val_actual = get(&mut con, "wubba lubba dub dub").unwrap();
        assert_eq!(val_actual, None);
    }

    #[test]
    fn test_key_expired_in_ttl() {
        let client = new_redis_client();
        let mut con = client.get_connection().unwrap();
        let key = "麻中蓬";
        set(&mut con, key, "$_$").unwrap();
        expire(&mut con, key, 1).unwrap();
        thread::sleep(std::time::Duration::from_millis(1500));
        let val_actual = get(&mut con, key).unwrap();
        assert_eq!(val_actual, None);
    }

    #[test]
    fn sled_metadata_to_ivec() {
        let metadata = SledMetadata {
            atime: 233,
            size: 0xaabbccdddeadbeef,
        };
        let ivec: IVec = metadata.into();
        assert_eq!(
            ivec,
            vec![0, 0, 0, 0, 0, 0, 0, 233, 0xaa, 0xbb, 0xcc, 0xdd, 0xde, 0xad, 0xbe, 0xef]
        );
    }

    #[test]
    fn sled_ivec_to_metadata() {
        let ivec: IVec = vec![
            0, 0, 0, 0, 0, 0, 0, 233, 0xaa, 0xbb, 0xcc, 0xdd, 0xde, 0xad, 0xbe, 0xef,
        ]
        .into();
        let metadata: SledMetadata = ivec.into();
        assert_eq!(metadata.atime, 233);
        assert_eq!(metadata.size, 0xaabbccdddeadbeef);
    }
}
