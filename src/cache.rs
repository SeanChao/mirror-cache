use redis::Commands;
use std::fs;
use std::io::prelude::*;
use std::marker::Send;
use std::vec::Vec;

use crate::models;
use crate::util;

pub type BytesArray = Vec<u8>;

pub trait CachePolicy: Sync + Send {
    fn put(&self, key: &str, entry: BytesArray);
    fn get(&self, key: &str) -> Option<BytesArray>;
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
    /**
     * put a cache entry with given `key` as key and `entry` as value
     * An entry larger than the size limit of the current cache (self) is ignored.
     * If the size limit is exceeded after putting the entry, LRU eviction will run.
     * This function handles both local FS data and redis metadata.
     */
    fn put(&self, key: &str, entry: BytesArray) {
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
        let _tx_result = redis::transaction(
            &mut sync_con,
            &[key, "total_size", "cache_keys"],
            |con, _pipe| {
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
                    let pkg_to_remove: Vec<(String, u64)> = con.zpopmin("cache_keys", 1).unwrap();
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
                        cur_cache_size = con
                            .decr::<&str, u64, u64>("total_size", pkg_size.unwrap_or(0))
                            .unwrap();
                        println!("total_size -= {:?} -> {}", pkg_size, cur_cache_size);
                    }
                }
                Ok(Some(()))
            },
        );
        // cache to local filesystem
        let data_to_write = entry;
        let (parent_dirs, _cache_file_name) = util::split_dirs(key);
        fs::create_dir_all(format!("cache/{}", parent_dirs)).unwrap();
        let mut f = fs::File::create(format!("cache/{}", key)).unwrap();
        f.write_all(&data_to_write).unwrap();
        let entry = &CacheEntry::new(&key, data_to_write.len() as u64);
        let _redis_resp_str = models::set_cache_entry(&mut sync_con, &key, entry);
        println!("CACHE SET {} -> {:?}", &key, entry);
    }

    fn get(&self, key: &str) -> Option<BytesArray> {
        let mut sync_con = models::get_sync_con(&self.redis_client).unwrap();
        let cache_result = models::get_cache_entry(&mut sync_con, key).unwrap();
        print!("CACHE GET {} -> {:?} ", key, &cache_result);
        if let Some(_cache_entry) = cache_result {
            // cache hit
            // update cache entry in db
            let new_atime = util::now();
            match models::update_cache_entry_atime(&mut sync_con, key, new_atime) {
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
                println!("[HIT]");
                return Some(file_content);
            }
        };
        println!("[MISS]");
        None
    }
}

/**
 *
 * TtlRedisCache is a simple cache policy that expire an existing cache entry
 * within the given TTL. The expiration is supported by redis.
 */
pub struct TtlRedisCache {
    pub root_dir: String,
    pub ttl: u64, // cache entry ttl in seconds
    redis_client: redis::Client,
}

impl TtlRedisCache {
    pub fn new(root_dir: &str, ttl: u64, redis_client: redis::Client) -> Self {
        let cloned_client = redis_client.clone();
        let cloned_root_dir = String::from(root_dir);
        std::thread::spawn(move || {
            println!("TTL expiration listener is created!");
            loop {
                let mut con = cloned_client.get_connection().unwrap();
                let mut pubsub = con.as_pubsub();
                // TODO: subscribe only current cache key pattern
                match pubsub.psubscribe("__keyevent*__:expired") {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("Failed to psubscribe: {}", e);
                        continue;
                    }
                }
                loop {
                    match pubsub.get_message() {
                        Ok(msg) => {
                            let payload: String = msg.get_payload().unwrap();
                            let pkg_name = Self::from_redis_key(&payload);
                            println!(
                                "channel '{}': {}, pkg: {}",
                                msg.get_channel_name(),
                                payload,
                                pkg_name
                            );
                            match fs::remove_file(Self::to_fs_path(&cloned_root_dir, &pkg_name)) {
                                Ok(_) => {}
                                Err(e) => {
                                    if e.kind() == std::io::ErrorKind::NotFound {
                                        eprintln!("Failed to remove {}: {}", &pkg_name, e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to get_message: {}", e);
                        }
                    }
                }
            }
        });
        Self {
            root_dir: String::from(root_dir),
            ttl,
            redis_client,
        }
    }

    pub fn to_redis_key(root_dir: &str, name: &str) -> String {
        format!("ttl_redis_cache/{}/{}", root_dir, name)
    }

    pub fn from_redis_key(key: &str) -> String {
        String::from(util::split_dirs(key).1)
    }

    pub fn to_fs_path(root_dir: &str, name: &str) -> String {
        format!("{}/{}", root_dir, name)
    }
}

impl CachePolicy for TtlRedisCache {
    fn put(&self, key: &str, entry: BytesArray) {
        let redis_key = Self::to_redis_key(&self.root_dir, key);
        let mut sync_con = models::get_sync_con(&self.redis_client).unwrap();
        let data_to_write = entry;
        let fs_path = Self::to_fs_path(&self.root_dir, &key);
        let (parent_dirs, _cached_filename) = util::split_dirs(&fs_path);
        fs::create_dir_all(parent_dirs).unwrap();
        let mut f = fs::File::create(fs_path).unwrap();
        f.write_all(&data_to_write).unwrap();
        match models::set(&mut sync_con, &redis_key, "") {
            Ok(_) => {}
            Err(e) => {
                eprintln!("set cache entry for {} failed: {}", key, e);
            }
        }
        match models::expire(&mut sync_con, &redis_key, self.ttl as usize) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("set cache entry ttl for {} failed: {}", key, e);
            }
        }
        println!("CACHE SET {} TTL={}", &key, self.ttl);
    }

    fn get(&self, key: &str) -> Option<BytesArray> {
        let redis_key = Self::to_redis_key(&self.root_dir, key);
        let mut sync_con = models::get_sync_con(&self.redis_client).unwrap();
        match models::get(&mut sync_con, &redis_key) {
            Ok(res) => match res {
                Some(_) => {
                    let cached_file_path = Self::to_fs_path(&self.root_dir, &key);
                    let file_content = match fs::read(cached_file_path) {
                        Ok(data) => data,
                        Err(_) => vec![],
                    };
                    if file_content.len() > 0 {
                        println!("GET {} [HIT]", key);
                        return Some(file_content);
                    }
                    None
                }
                None => None,
            },
            Err(e) => {
                println!("get cache entry key={} failed: {}", key, e);
                None
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::io;
    use std::thread;
    use std::time;

    static TEST_CACHE_DIR: &str = "cache";

    fn flush_cache() {
        let redis_client = new_redis_client();
        let mut con = redis_client.get_connection().unwrap();
        redis::cmd("flushall").query::<()>(&mut con).unwrap();
        match fs::remove_dir_all(TEST_CACHE_DIR) {
            Ok(_) => {}
            Err(e) => {
                if e.kind() != io::ErrorKind::NotFound {
                    println!("failed to remove cache dir: {:?}", e);
                }
            }
        };
    }

    fn new_redis_client() -> redis::Client {
        let redis_client = redis::Client::open("redis://localhost:3001/")
            .expect("Failed to connect to redis server (test)");
        return redis_client;
    }

    fn get_cache_size(con: &mut redis::Connection) -> u64 {
        con.get("total_size").unwrap()
    }

    fn get_file_all(path: &str) -> Vec<u8> {
        let mut buf = Vec::new();
        fs::File::open(path).unwrap().read_to_end(&mut buf).unwrap();
        buf
    }

    fn file_not_exist(path: &str) -> bool {
        match fs::File::open(path) {
            Ok(_) => false,
            Err(e) => {
                if e.kind() == io::ErrorKind::NotFound {
                    return true;
                }
                false
            }
        }
    }

    #[test]
    #[serial]
    fn test_cache_entry_success() {
        flush_cache();
        let redis_client = new_redis_client();
        let lru_cache = LruRedisCache::new(TEST_CACHE_DIR, 16 * 1024 * 1024, redis_client);
        let redis_tester_client = new_redis_client();
        let key = "answer";
        let cached_data = vec![42];
        lru_cache.put("answer", cached_data.clone());
        let total_size_expected = cached_data.len() as u64;
        let mut con = redis_tester_client.get_connection().unwrap();
        let total_size_actual: u64 = get_cache_size(&mut con);
        let cached_data_actual = get_file_all(&format!("{}/{}", TEST_CACHE_DIR, key));
        // metadata: size is 1, file content is the same
        assert_eq!(total_size_actual, total_size_expected);
        assert_eq!(&cached_data_actual, &cached_data);
        flush_cache();
    }

    #[test]
    #[serial]
    fn test_lru_cache_size_constraint() {
        flush_cache();
        let redis_client = new_redis_client();
        let redis_client_tester = new_redis_client();
        let lru_cache = LruRedisCache::new(TEST_CACHE_DIR, 16, redis_client);
        lru_cache.put("tsu_ki", vec![0; 5]);
        let mut con = redis_client_tester.get_connection().unwrap();
        let total_size_actual: u64 = get_cache_size(&mut con);
        assert_eq!(total_size_actual, 5);
        thread::sleep(time::Duration::from_secs(1));
        lru_cache.put("kirei", vec![0; 11]);
        let total_size_actual: u64 = get_cache_size(&mut con);
        assert_eq!(total_size_actual, 16);
        assert_eq!(
            get_file_all(&format!("{}/{}", TEST_CACHE_DIR, "tsu_ki")),
            vec![0; 5]
        );
        assert_eq!(
            get_file_all(&format!("{}/{}", TEST_CACHE_DIR, "kirei")),
            vec![0; 11]
        );
        // cache is full, evict tsu_ki
        lru_cache.put("suki", vec![1; 4]);
        assert_eq!(get_cache_size(&mut con), 15);
        assert_eq!(
            file_not_exist(&format!("{}/{}", TEST_CACHE_DIR, "tsu_ki")),
            true
        );
        // evict both
        lru_cache.put("deadbeef", vec![2; 16]);
        assert_eq!(get_cache_size(&mut con), 16);
        assert_eq!(
            file_not_exist(&format!("{}/{}", TEST_CACHE_DIR, "kirei")),
            true
        );
        assert_eq!(
            file_not_exist(&format!("{}/{}", TEST_CACHE_DIR, "suki")),
            true
        );
        assert_eq!(
            get_file_all(&format!("{}/{}", TEST_CACHE_DIR, "deadbeef")),
            vec![2; 16]
        );
        flush_cache();
    }

    #[test]
    #[serial]
    fn test_lru_cache_no_evict_recent() {
        flush_cache();
        let redis_client = new_redis_client();
        let redis_client_tester = new_redis_client();
        let mut con = redis_client_tester.get_connection().unwrap();
        let lru_cache = LruRedisCache::new(TEST_CACHE_DIR, 3, redis_client);
        let key1 = "1二号去听经";
        let key2 = "2晚上住旅店";
        let key3 = "3三号去餐厅";
        let key4 = "4然后看电影";
        lru_cache.put(key1, vec![1]);
        thread::sleep(time::Duration::from_secs(1));
        lru_cache.put(key2, vec![2]);
        thread::sleep(time::Duration::from_secs(1));
        lru_cache.put(key3, vec![3]);
        assert_eq!(get_cache_size(&mut con), 3);
        // set key4, evict key1
        thread::sleep(time::Duration::from_secs(1));
        lru_cache.put(key4, vec![4]);
        assert_eq!(lru_cache.get(key1), None);
        assert_eq!(get_cache_size(&mut con), 3);
        // get key2, update atime
        thread::sleep(time::Duration::from_secs(1));
        assert_eq!(lru_cache.get(key2), Some(vec![2]));
        assert_eq!(get_cache_size(&mut con), 3);
        // set key1, evict key3
        thread::sleep(time::Duration::from_secs(1));
        lru_cache.put(key1, vec![11]);
        assert_eq!(get_cache_size(&mut con), 3);
        assert_eq!(lru_cache.get(key3), None);
        assert_eq!(get_cache_size(&mut con), 3);
        flush_cache();
    }

    #[test]
    #[serial]
    fn test_atime_updated_upon_access() {
        flush_cache();
        let redis_client = new_redis_client();
        let redis_client_tester = new_redis_client();
        let mut con = redis_client_tester.get_connection().unwrap();
        let lru_cache = LruRedisCache::new(TEST_CACHE_DIR, 3, redis_client);
        let key = "Shire";
        lru_cache.put(key, vec![0]);
        let atime_before: i64 = con.hget(key, "atime").unwrap();
        thread::sleep(time::Duration::from_secs(1));
        lru_cache.get(key);
        let atime_after: i64 = con.hget(key, "atime").unwrap();
        assert_eq!(atime_before < atime_after, true);
        flush_cache();
    }
}
