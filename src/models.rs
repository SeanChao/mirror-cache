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

// get cache entry
pub fn get_cache_entry(
    con: &mut SyncConnection,
    key: &str,
) -> Result<Option<CacheEntry<LruCacheMetadata, String, ()>>> {
    let map: HashMap<String, String> = con.hgetall(key)?;
    println!("redis return {:?}", map);
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

pub fn set_cache_entry(
    con: &mut SyncConnection,
    key: &str,
    entry: &CacheEntry<LruCacheMetadata, String, ()>,
) -> Result<()> {
    let kv_array = entry.to_redis_multiple_fields();
    let tx_result = redis::transaction(con, &[key, "total_size", "cache_keys"], |con, pipe| {
        pipe.incr("total_size", entry.metadata.size)
            .hset_multiple::<&str, &str, String>(key, &kv_array)
            .ignore()
            .zadd("cache_keys", key, entry.metadata.atime)
            .query(con)?;
        Ok(Some(()))
    });
    tx_result.map_err(|e| RedisCMDError(e))
}

pub fn update_cache_entry_atime(con: &mut SyncConnection, key: &str, atime: i64) -> Result<i64> {
    match redis::pipe()
        .atomic()
        .hset(key, "atime", atime)
        .zadd("cache_keys", key, atime)
        .query::<(i64, i64)>(con)
    {
        Ok(_) => Ok(atime),
        Err(e) => Err(RedisCMDError(e)),
    }
}
