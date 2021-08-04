mod cache;
mod error;
mod models;
mod settings;
mod task;
mod util;

use crate::task::TaskManager;
use std::sync::Arc;

type SharedTaskManager = Arc<TaskManager>;

#[macro_use]
extern crate serde_derive;

#[tokio::main]
async fn main() {
    let app_settings = settings::Settings::new().unwrap();
    let port = app_settings.port;

    let api = filters::root(app_settings);
    println!("🎉 Server is running on 127.0.0.1:{}", port);
    warp::serve(api).run(([127, 0, 0, 1], port)).await;
}

mod filters {
    use super::handlers;
    use super::*;
    use crate::cache;
    use crate::cache::CachePolicy;
    use crate::error::Error;
    use crate::error::Result;
    use crate::settings::Policy;
    use crate::settings::PolicyType;
    use crate::settings::Rule;
    use crate::settings::Settings;
    use crate::task::TaskManager;
    use std::convert::Infallible;
    use std::sync::Arc;
    use warp::Filter;

    fn create_cache_from_rule(
        rule: &Rule,
        policies: &Vec<Policy>,
        redis_client: Option<redis::Client>,
    ) -> Result<Arc<dyn CachePolicy>> {
        let policy_ident = rule.policy.clone();
        for p in policies {
            if p.name == policy_ident {
                let policy_type = p.typ;
                match policy_type {
                    PolicyType::Lru => {
                        return Ok(Arc::new(cache::LruRedisCache::new(
                            p.path.as_ref().unwrap(),
                            p.size.unwrap_or(0),
                            redis_client.unwrap(),
                        )));
                    }
                    PolicyType::Ttl => {
                        return Ok(Arc::new(cache::TtlRedisCache::new(
                            p.path.as_ref().unwrap(),
                            p.timeout.unwrap_or(0),
                            redis_client.unwrap(),
                        )));
                    }
                };
            }
        }
        Err(Error::ConfigInvalid(format!(
            "failed to find a matched policy for rule {:?}",
            rule
        )))
    }

    pub fn root(
        settings: Settings,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        let redis_url = settings.get_redis_url();
        let policies = settings.policies.clone();
        let pypi_index_rule = settings.builtin.clone().pypi_index;
        let pypi_pkg_rule = settings.builtin.clone().pypi_packages;
        let anaconda_rule = settings.builtin.clone().anaconda;

        let mut tm = TaskManager::new(settings.clone());

        let redis_client = redis::Client::open(redis_url).expect("failed to connect to redis");
        let pypi_index_cache =
            create_cache_from_rule(&pypi_index_rule, &policies, Some(redis_client.clone()))
                .unwrap();
        let pypi_pkg_cache =
            create_cache_from_rule(&pypi_pkg_rule, &policies, Some(redis_client.clone())).unwrap();
        let anaconda_cache =
            create_cache_from_rule(&anaconda_rule, &policies, Some(redis_client.clone())).unwrap();

        tm.pypi_index_cache = pypi_index_cache;
        tm.pypi_pkg_cache = pypi_pkg_cache;
        tm.anaconda_cache = anaconda_cache;

        let arc_tm = Arc::new(tm);
        pypi_index(arc_tm.clone())
            .or(pypi_packages(arc_tm.clone()))
            .or(anaconda_all(arc_tm.clone()))
    }

    fn with_tm(
        tm: SharedTaskManager,
    ) -> impl Filter<Extract = (SharedTaskManager,), Error = Infallible> + Clone {
        warp::any().map(move || tm.clone())
    }

    // GET /pypi/web/simple/:string
    fn pypi_index(
        tm: SharedTaskManager,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::path!("pypi" / "simple" / String)
            .and(with_tm(tm))
            .and_then(handlers::get_pypi_index)
    }

    // GET /pypi/package/:string/:string/:string/:string
    fn pypi_packages(
        tm: SharedTaskManager,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::path!("pypi" / "packages" / String / String / String / String)
            .and(with_tm(tm))
            .and_then(handlers::get_pypi_pkg)
    }

    // GET /anaconda/:repo/:arch/:filename
    fn anaconda_all(
        tm: SharedTaskManager,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::path!("anaconda" / String / String / String)
            .and(with_tm(tm))
            .and_then(handlers::get_anaconda)
    }
}

mod handlers {
    use super::*;
    use crate::task::Task;

    use warp::{http::Response, Rejection};

    pub async fn get_pypi_index(
        path: String,
        tm: SharedTaskManager,
    ) -> Result<impl warp::Reply, Rejection> {
        let tw = Task::PypiIndexTask {
            pkg_name: path,
            upstream: "https://pypi.org/simple".to_string(),
        };
        let rtm = tm.as_ref();
        match tw.resolve(rtm).await {
            Ok(data) => Ok(Response::builder()
                .header("content-type", "text/html")
                .body(data)),
            Err(_) => Err(warp::reject()),
        }
    }

    pub async fn get_pypi_pkg(
        seg0: String,
        seg1: String,
        seg2: String,
        seg3: String,
        tm: SharedTaskManager,
    ) -> Result<impl warp::Reply, Rejection> {
        let fullpath = format!("{}/{}/{}/{}", seg0, seg1, seg2, seg3);
        let t = Task::PypiPackagesTask { pkg_path: fullpath };
        match t.resolve(tm.as_ref()).await {
            Ok(data) => Ok(Response::builder().body(data)),
            Err(e) => {
                eprintln!("{}", e);
                Err(warp::reject())
            }
        }
    }

    pub async fn get_anaconda(
        channel: String,
        arch: String,
        filename: String,
        tm: SharedTaskManager,
    ) -> Result<impl warp::Reply, Rejection> {
        let cache_key = format!("{}/{}/{}", channel, arch, filename);
        let t = Task::AnacondaTask { path: cache_key };
        match t.resolve(tm.as_ref()).await {
            Ok(data) => Ok(Response::builder().body(data)),
            Err(e) => {
                eprintln!("{}", e);
                Err(warp::reject())
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::settings::Settings;
    use warp::http::StatusCode;
    use warp::test::request;
    use warp::Filter;

    fn get_settings() -> Settings {
        println!(
            "{:?}",
            settings::Settings::new_from("config-test", "app_test").unwrap()
        );
        settings::Settings::new_from("config-test", "app_test").unwrap()
    }

    fn get_filter_root() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone
    {
        let app_settings = get_settings();
        filters::root(app_settings)
    }

    #[tokio::test]
    async fn get_pypi_index() {
        let app_url = get_settings().url.unwrap();
        let pkg_name = "hello-world";
        let api = get_filter_root();
        let resp = request()
            .method("GET")
            .path(&format!("/pypi/simple/{}", pkg_name))
            .reply(&api)
            .await;
        let resp_bytes = resp.body().to_vec();
        let resp_text = std::str::from_utf8(&resp_bytes).unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        // webpage fetched successfully
        assert!(resp_text.contains(&format!("Links for {}", pkg_name)));
        // target link is replaced successfully
        assert!(resp_text.contains(&app_url));
    }
}
