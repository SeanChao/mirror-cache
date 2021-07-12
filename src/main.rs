mod cache;
mod error;
mod util;

#[tokio::main]
async fn main() {
    let redis_client =
        redis::Client::open("redis://127.0.0.1/").expect("failed to initialize redis client");
    let cache = cache::Cache::new("cache", 16 * 1024 * 1024);
    let api = filters::root(&redis_client, &cache);
    warp::serve(api).run(([127, 0, 0, 1], 9000)).await;
}

mod filters {
    use super::handlers;
    use crate::cache::Cache;
    use std::convert::Infallible;
    use warp::Filter;

    type Client = redis::Client;

    pub fn root(
        redis_client: &redis::Client,
        cache: &Cache,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        pypi_index().or(pypi_packages(redis_client.clone(), cache.clone()))
    }

    // GET /pypi/web/simple/:string
    fn pypi_index() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::path!("pypi" / "web" / "simple" / String).and_then(handlers::get_pypi_index)
    }

    // GET /pypi/package/:string/:string/:string/:string
    fn pypi_packages(
        client: Client,
        cache: Cache,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::path!("pypi" / "packages" / String / String / String / String)
            .and(with_redis_client(client))
            .and(with_cache(cache))
            .and_then(handlers::get_pypi_pkg)
    }

    #[allow(dead_code)]
    fn with_cache(cache: Cache) -> impl Filter<Extract = (Cache,), Error = Infallible> + Clone {
        warp::any().map(move || cache.clone())
    }

    fn with_redis_client(
        client: redis::Client,
    ) -> impl Filter<Extract = (redis::Client,), Error = Infallible> + Clone {
        warp::any().map(move || client.clone())
    }
}

mod handlers {
    use super::models;
    use crate::cache::{Cache, CacheEntry};
    use crate::util;
    use reqwest::ClientBuilder;
    use std::fs;
    use std::io::prelude::*;
    use warp::{http::Response, Rejection};

    type Client = redis::Client;

    pub async fn get_pypi_index(path: String) -> Result<impl warp::Reply, Rejection> {
        let upstream = format!("https://pypi.org/simple/{}", path);
        let client = ClientBuilder::new().build().unwrap();
        let resp = client.get(&upstream).send().await;
        match resp {
            Ok(response) => Ok(Response::builder()
                .header("content-type", "text/html")
                .body(response.text().await.unwrap().replace(
                    "https://files.pythonhosted.org/packages",
                    format!("http://localhost:9000/pypi/packages").as_str(),
                ))),
            Err(err) => {
                println!("{:?}", err);
                Err(warp::reject::reject())
            }
        }
    }

    pub async fn get_pypi_pkg(
        path: String,
        path2: String,
        path3: String,
        path4: String,
        redis_client: Client,
        cache: Cache,
    ) -> Result<impl warp::Reply, Rejection> {
        let parent_dirs = format!("{}/{}/{}", path, path2, path3);
        let filename = format!("{}", path4);
        let fullpath = format!("{}/{}/{}/{}", path, path2, path3, path4);
        let cached_file_path = format!("cache/{}/{}", parent_dirs, filename);

        let mut con = models::get_con(&redis_client)
            .await
            .map_err(|e| warp::reject::custom(e))
            .unwrap();
        let mut sync_con = models::get_sync_con(&redis_client).unwrap();

        // Check whether the file is cached
        let cache_result = models::get_cache_entry(&mut con, &fullpath).await?;
        println!("[HMGET] {} -> {:?}", &fullpath, &cache_result);
        if let Some(cache_entry) = cache_result {
            // cache hit
            // update cache entry in db
            models::update_cache_entry_atime(&mut con, &fullpath, util::now()).await?;
            let cached_file_path = format!("cache/{}", cache_entry.path);
            let file_content = match fs::read(cached_file_path) {
                Ok(data) => data,
                Err(_) => vec![],
            };
            if file_content.len() > 0 {
                println!("cache hit");
                return Ok(Response::builder().body(file_content));
            }
        }
        // cache miss
        println!("cache miss");
        let upstream = format!("https://files.pythonhosted.org/packages/{}", fullpath);
        let client = ClientBuilder::new().build().unwrap();
        println!("GET {}", &upstream);
        let resp = client.get(&upstream).send().await;
        match resp {
            Ok(response) => {
                let content_size = response.content_length().unwrap();
                println!("fetched {}", content_size);
                // cache to local filesystem
                let resp_bytes = response.bytes().await.unwrap();
                let data_to_write = resp_bytes.to_vec();
                fs::create_dir_all(format!("cache/{}", parent_dirs)).unwrap();
                let mut f = fs::File::create(&cached_file_path).unwrap();
                f.write_all(&data_to_write).unwrap();
                let _redis_resp_str = models::set_cache_entry(
                    &mut sync_con,
                    &fullpath,
                    &CacheEntry::new(&fullpath, content_size),
                    &cache,
                )
                .await?;
                Ok(Response::builder().body(data_to_write))
            }
            Err(err) => {
                println!("{:?}", err);
                Err(warp::reject::reject())
            }
        }
    }
}

mod models {
    extern crate redis;
    use crate::cache::{Cache, CacheEntry};
    use crate::error::Error::*;
    use crate::error::Result;
    use redis::{aio::Connection, AsyncCommands, Commands, Connection as SyncConnection};
    use std::collections::HashMap;
    use std::fs;

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
    pub async fn get_cache_entry(con: &mut Connection, key: &str) -> Result<Option<CacheEntry>> {
        let map: HashMap<String, String> = con.hgetall(key).await?;
        println!("redis return {:?}", map);
        if map.is_empty() {
            // not exist in cache
            return Ok(None);
        }
        let cache_entry = CacheEntry {
            valid: if map.get("valid").unwrap_or(&String::from("0")).eq(&"0") {
                false
            } else {
                true
            },
            path: String::from(map.get("path").unwrap_or(&String::from(""))),
            atime: String::from(map.get("atime").unwrap_or(&String::from("0")))
                .parse::<i64>()
                .unwrap_or(0),
            size: String::from(map.get("size").unwrap_or(&String::from("0")))
                .parse::<u64>()
                .unwrap_or(0),
        };
        Ok(Some(cache_entry))
    }

    pub async fn set_cache_entry(
        con: &mut SyncConnection,
        key: &str,
        entry: &CacheEntry,
        cache: &Cache,
    ) -> Result<()> {
        if entry.size > cache.size_limit {
            println!(
                "skip cache for {:?}, because its size exceeds the limit",
                entry
            );
            return Ok(());
        }
        // evict cache entry if necessary
        let tx_result = redis::transaction(con, &[key, "total_size"], |con, pipe| {
            let mut cur_cache_size = con
                .get::<&str, Option<u64>>("total_size")
                .unwrap()
                .unwrap_or(0);
            while cur_cache_size + entry.size > cache.size_limit {
                // LRU eviction
                println!(
                    "current {} + new {} > limit {}",
                    con.get::<&str, Option<u64>>("total_size")
                        .unwrap()
                        .unwrap_or(0),
                    entry.size,
                    cache.size_limit
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
                    let path = cache.cache_path(&f);
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
            let kv_array = entry.to_redis_multiple_fields();
            pipe.incr("total_size", entry.size)
                .hset_multiple::<&str, &str, String>(key, &kv_array)
                .ignore()
                .zadd("cache_keys", key, entry.atime)
                .query(con)?;
            Ok(Some(()))
        });
        tx_result.map_err(|e| RedisCMDError(e))
    }

    pub async fn update_cache_entry_atime(
        con: &mut Connection,
        key: &str,
        atime: i64,
    ) -> Result<i64> {
        match con.hset::<&str, &str, i64, i64>(key, "atime", atime).await {
            Ok(s) => Ok(s),
            Err(e) => Err(RedisCMDError(e)),
        }
    }
}
