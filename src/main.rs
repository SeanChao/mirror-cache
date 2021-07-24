mod cache;
mod config;
mod error;
mod models;
mod util;

#[tokio::main]
async fn main() {
    let config = config::Config {
        redis_url: String::from("redis://127.0.0.1/"),
    };
    let api = filters::root(config);
    warp::serve(api).run(([127, 0, 0, 1], 9000)).await;
}

static TEMP_TTL_DEFAULT: u64 = 5;
static TEMP_TTL_CACHE_PATH: &str = "cache/ttl";

mod filters {
    use super::handlers;
    use crate::cache;
    use crate::cache::CachePolicy;
    use crate::config::Config;
    use std::convert::Infallible;
    use std::sync::Arc;
    use warp::Filter;

    pub fn root(
        config: Config,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        let redis_client =
            redis::Client::open(config.redis_url).expect("failed to connect to redis");
        let lru_cache = cache::LruRedisCache::new("cache", 16 * 1024 * 1024, redis_client.clone());
        let ttl_cache = cache::TtlRedisCache::new(
            super::TEMP_TTL_CACHE_PATH,
            super::TEMP_TTL_DEFAULT,
            redis_client.clone(),
        );
        pypi_index(Arc::new(ttl_cache)).or(pypi_packages(Arc::new(lru_cache)))
    }

    // GET /pypi/web/simple/:string
    fn pypi_index(
        cache: Arc<impl CachePolicy>,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::path!("pypi" / "web" / "simple" / String)
            .and(with_cache(cache))
            .and_then(handlers::get_pypi_index)
    }

    // GET /pypi/package/:string/:string/:string/:string
    fn pypi_packages(
        cache: Arc<impl CachePolicy>,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::path!("pypi" / "packages" / String / String / String / String)
            .and(with_cache(cache))
            .and_then(handlers::get_pypi_pkg)
    }

    fn with_cache(
        cache: Arc<impl CachePolicy>,
    ) -> impl Filter<Extract = (Arc<impl CachePolicy>,), Error = Infallible> + Clone {
        warp::any().map(move || cache.clone())
    }
}

mod handlers {
    use crate::cache::CachePolicy;
    use reqwest::ClientBuilder;
    use std::sync::Arc;
    use warp::{http::Response, Rejection};

    pub async fn get_pypi_index(
        path: String,
        cache: Arc<impl CachePolicy>,
    ) -> Result<impl warp::Reply, Rejection> {
        if let Some(cached_data) = cache.get(&path) {
            // cache hit
            match std::str::from_utf8(&cached_data) {
                Ok(v) => {
                    return Ok(Response::builder()
                        .header("content-type", "text/html")
                        .body(String::from(v)));
                }
                Err(e) => {
                    println!("error converting cached data into utf8 text: {}", e);
                }
            }
        }
        // cache miss
        let upstream = format!("https://pypi.org/simple/{}", path);
        let client = ClientBuilder::new().build().unwrap();
        let resp = client.get(&upstream).send().await;
        match resp {
            Ok(response) => {
                let resp_body = response.text().await.unwrap().replace(
                    "https://files.pythonhosted.org/packages",
                    "http://localhost:9000/pypi/packages",
                );
                cache.put(&path, resp_body.as_bytes().to_vec());
                Ok(Response::builder()
                    .header("content-type", "text/html")
                    .body(resp_body))
            }
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
        cache: Arc<impl CachePolicy>,
    ) -> Result<impl warp::Reply, Rejection> {
        let fullpath = format!("{}/{}/{}/{}", path, path2, path3, path4);
        println!("GET pypi-pkg: {}", &fullpath);

        if let Some(cached_entry) = cache.get(&fullpath) {
            let bytes = cached_entry;
            return Ok(Response::builder().body(bytes));
        }
        // cache miss
        println!("cache miss");
        let upstream = format!("https://files.pythonhosted.org/packages/{}", &fullpath);
        let client = ClientBuilder::new().build().unwrap();
        println!("GET {}", &upstream);
        let resp = client.get(&upstream).send().await;
        match resp {
            Ok(response) => {
                let content_size = response.content_length().unwrap();
                println!("fetched {}", content_size);
                let resp_bytes = response.bytes().await.unwrap();
                let data_to_write = resp_bytes.to_vec();
                cache.put(&fullpath, data_to_write.clone());
                Ok(Response::builder().body(data_to_write))
            }
            Err(err) => {
                println!("{:?}", err);
                Err(warp::reject::reject())
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::config::Config;
    use super::*;
    use std::fs;
    use warp::http::StatusCode;
    use warp::test::request;
    use warp::Filter;

    fn get_filter_root() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone
    {
        let config = Config {
            redis_url: format!("redis://localhost:3001/"),
        };
        filters::root(config)
    }

    #[tokio::test]
    async fn get_pypi_index() {
        let pkg_name = "hello-world";
        let api = get_filter_root();
        let resp = request()
            .method("GET")
            .path(&format!("/pypi/web/simple/{}", pkg_name))
            .reply(&api)
            .await;
        let resp_bytes = resp.body().to_vec();
        let resp_text = std::str::from_utf8(&resp_bytes).unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        // webpage fetched successfully
        assert!(resp_text.contains(&format!("Links for {}", pkg_name)));
        // target link is replaced successfully
        assert!(resp_text.contains("http://localhost:9000"));
    }

    #[tokio::test]
    async fn get_pypi_index_cached() {
        let pkg_name = "Luna";
        let api = get_filter_root();
        let _resp = request()
            .method("GET")
            .path(&format!("/pypi/web/simple/{}", pkg_name))
            .reply(&api)
            .await;
        // hack local cache
        let data = "My mum always said things we lose have a way of coming back to us in the end. If not always in the way we expect.";
        fs::write(
            cache::TtlRedisCache::to_fs_path(TEMP_TTL_CACHE_PATH, pkg_name),
            data,
        )
        .unwrap();
        let resp = request()
            .method("GET")
            .path(&format!("/pypi/web/simple/{}", pkg_name))
            .reply(&api)
            .await;
        let resp_bytes = resp.body().to_vec();
        let resp_text = std::str::from_utf8(&resp_bytes).unwrap();
        assert_eq!(resp_text, data);
    }

    #[tokio::test]
    async fn get_pypi_index_cache_expired() {
        let pkg_name = "moon";
        let api = get_filter_root();
        let _resp = request()
            .method("GET")
            .path(&format!("/pypi/web/simple/{}", pkg_name))
            .reply(&api)
            .await;
        // wait for cache expiration
        std::thread::sleep(std::time::Duration::from_millis(
            TEMP_TTL_DEFAULT * 1000 + 1000,
        ));
        // hack local cache
        let data = "月がきれい";
        let fs_path = cache::TtlRedisCache::to_fs_path(TEMP_TTL_CACHE_PATH, pkg_name);
        let (parents, _file) = util::split_dirs(&fs_path);
        fs::create_dir_all(parents).unwrap();
        fs::write(&fs_path, data).unwrap();
        let resp = request()
            .method("GET")
            .path(&format!("/pypi/web/simple/{}", pkg_name))
            .reply(&api)
            .await;
        let resp_bytes = resp.body().to_vec();
        let resp_text = std::str::from_utf8(&resp_bytes).unwrap();
        assert!(resp_text.contains("Links for"));
        assert_ne!(resp_text, data);
        std::thread::sleep(std::time::Duration::from_millis(
            TEMP_TTL_DEFAULT * 1000 + 1000,
        ));
        assert!(!std::path::Path::new(&fs_path).exists()); // cache file should be removed
    }
}
