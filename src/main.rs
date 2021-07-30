mod cache;
mod config;
mod error;
mod models;
mod task;
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
static TEMP_TTL_CACHE_PATH: &str = "cache/pypi/web/simple";

mod filters {
    use super::handlers;
    use crate::cache;
    use crate::cache::CachePolicy;
    use crate::cache::DownloadQueue;
    use crate::config::Config;
    use crate::task;
    use std::convert::Infallible;
    use std::sync::Arc;
    use warp::Filter;

    pub fn root(
        config: Config,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        tokio::spawn(async move { task::task_recv(rx).await });
        let redis_client =
            redis::Client::open(config.redis_url).expect("failed to connect to redis");
        let lru_cache =
            cache::LruRedisCache::new("cache", 16 * 1024 * 1024, redis_client.clone(), Some(tx));
        let ttl_cache = cache::TtlRedisCache::new(
            super::TEMP_TTL_CACHE_PATH,
            super::TEMP_TTL_DEFAULT,
            redis_client.clone(),
        );
        let ttl_cache_conda = cache::TtlRedisCache::new("cache/conda", 5, redis_client.clone());
        pypi_index(Arc::new(ttl_cache))
            .or(pypi_packages(Arc::new(lru_cache)))
            .or(anaconda_all(Arc::new(ttl_cache_conda)))
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
        cache: Arc<impl CachePolicy + DownloadQueue>,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::path!("pypi" / "packages" / String / String / String / String)
            .and(with_cache_download_queue(cache))
            .and_then(handlers::get_pypi_pkg)
    }

    // GET /anaconda/:repo/:arch/:filename
    fn anaconda_all(
        cache: Arc<impl CachePolicy>,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::path!("anaconda" / String / String / String)
            .and(with_cache(cache))
            .and_then(handlers::get_anaconda)
    }

    // fn anaconda_

    fn with_cache(
        cache: Arc<impl CachePolicy>,
    ) -> impl Filter<Extract = (Arc<impl CachePolicy>,), Error = Infallible> + Clone {
        warp::any().map(move || cache.clone())
    }

    fn with_cache_download_queue(
        cache: Arc<impl CachePolicy + DownloadQueue>,
    ) -> impl Filter<Extract = (Arc<impl CachePolicy + DownloadQueue>,), Error = Infallible> + Clone
    {
        warp::any().map(move || cache.clone())
    }
}

mod handlers {
    use crate::cache::CachePolicy;
    use crate::cache::DownloadQueue;
    use crate::task::DownloadTask;

    use reqwest::ClientBuilder;
    use std::sync::Arc;
    use tokio::sync::oneshot;
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
        cache: Arc<impl CachePolicy + DownloadQueue>,
    ) -> Result<impl warp::Reply, Rejection> {
        let fullpath = format!("{}/{}/{}/{}", path, path2, path3, path4);

        if let Some(cached_entry) = cache.get(&fullpath) {
            let bytes = cached_entry;
            println!("🎯 GET pypi-pkg: {}", &fullpath);
            return Ok(Response::builder().body(bytes));
        }
        // cache miss
        println!("↗️ GET pypi-pkg: {}", &fullpath);
        let upstream = format!("https://files.pythonhosted.org/packages/{}", &fullpath);
        let tx = cache.get_sender();
        let (resp_tx, resp_rx) = oneshot::channel();
        let task = DownloadTask {
            url: upstream.clone(),
            resp: resp_tx,
        };
        match tx.send(task).await {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Failed to send download task: {}", e);
            }
        };
        let chan_resp = resp_rx.await;
        match chan_resp {
            Ok(response) => match response {
                Ok(response) => {
                    let resp_bytes = response;
                    let data_to_write = resp_bytes.to_vec();
                    cache.put(&fullpath, data_to_write.clone());
                    Ok(Response::builder().body(data_to_write))
                }
                Err(e) => {
                    eprintln!("\tError downloading task : {}", &e);
                    Err(warp::reject::reject())
                }
            },
            Err(err) => {
                eprintln!("{:?}", err);
                Err(warp::reject::reject())
            }
        }
    }

    pub async fn get_anaconda(
        channel: String,
        arch: String,
        filename: String,
        cache: Arc<impl CachePolicy>,
    ) -> Result<impl warp::Reply, Rejection> {
        let cache_key = format!("anaconda/{}/{}/{}", channel, arch, filename);
        if let Some(cached_entry) = cache.get(&cache_key) {
            let bytes = warp::hyper::body::Bytes::from(cached_entry);
            println!("🎯 [GET] /anaconda/{}/{}/{}", channel, arch, filename);
            return Ok(Response::builder().body(bytes));
        }
        println!("↗️ [GET] /anaconda/{}/{}/{}", channel, arch, filename);
        let upstream = format!(
            "https://conda-static.anaconda.org/{}/{}/{}",
            channel, arch, filename
        );
        let client = ClientBuilder::new().build().unwrap();
        let resp = client.get(&upstream).send().await;
        match resp {
            Ok(response) => {
                let resp_bytes = response.bytes().await.unwrap();
                let data_to_write = resp_bytes.to_vec();
                cache.put(&cache_key, data_to_write.clone());
                Ok(Response::builder().body(resp_bytes))
            }
            Err(err) => {
                eprintln!("{:?}", err);
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
        let fs_path = cache::TtlRedisCache::to_fs_path(TEMP_TTL_CACHE_PATH, pkg_name);
        assert!(!std::path::Path::new(&fs_path).exists()); // cache file should be removed
                                                           // hack local cache
        let data = "月がきれい";
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
    }
}
