mod cache;
mod error;
mod models;
mod util;

#[tokio::main]
async fn main() {
    let api = filters::root();
    warp::serve(api).run(([127, 0, 0, 1], 9000)).await;
}

mod filters {
    use super::handlers;
    use crate::cache::CachePolicy;
    use crate::cache::LruRedisCache;
    use std::convert::Infallible;
    use std::sync::Arc;
    use warp::Filter;

    pub fn root() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        let redis_client =
            redis::Client::open("redis://127.0.0.1/").expect("failed to connect to redis");
        let lru_cache = LruRedisCache::new("cache", 16 * 1024 * 1024, redis_client);
        pypi_index().or(pypi_packages(Arc::new(lru_cache)))
    }

    // GET /pypi/web/simple/:string
    fn pypi_index() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::path!("pypi" / "web" / "simple" / String).and_then(handlers::get_pypi_index)
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
