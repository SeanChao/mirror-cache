use crate::cache::CacheEntry;
use crate::cache::LruCacheMetadata;
use crate::error::Error::*;
use crate::error::Result;
use redis::{aio::Connection, Commands, Connection as SyncConnection};
use std::collections::HashMap;

#[allow(dead_code)]
pub async fn get_con(client: &redis::Client) -> Result<Connection> {
    client
        .get_async_connection()
        .await
        .map_err(|e| RedisClientError(e).into())
}

pub fn get_sync_con(client: &redis::Client) -> Result<SyncConnection> {
    client
        .get_connection()
        .map_err(|e| RedisClientError(e).into())
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
    tx_result.map_err(|e| RedisCMDError(e))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    fn new_redis_client() -> redis::Client {
        let redis_client = redis::Client::open("redis://localhost:3001/")
            .expect("Failed to connect to redis server (test)");
        return redis_client;
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
}
