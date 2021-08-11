mod cache;
mod error;
mod metric;
mod models;
mod settings;
mod task;
mod util;

use crate::cache::CachePolicy;
use crate::error::Error;
use crate::error::Result;
use crate::settings::Policy;
use crate::settings::PolicyType;
use crate::settings::Rule;
use crate::task::SharedTaskManager;
use crate::task::TaskManager;
use metrics::increment_counter;
use metrics_exporter_prometheus::PrometheusBuilder;
use metrics_util::MetricKindMask;
use regex::Regex;
use std::sync::Arc;

#[macro_use]
extern crate serde_derive;

extern crate pretty_env_logger;
#[macro_use]
extern crate log;

#[tokio::main]
async fn main() {
    let app_settings = settings::Settings::new().unwrap();

    // initialize the logger
    let mut log_builder = pretty_env_logger::formatted_builder();
    log_builder
        .filter_module("hyper::proto", log::LevelFilter::Error) // hide excessive logs
        .filter_level(app_settings.get_log_level())
        .init();

    let port = app_settings.port;

    let redis_url = app_settings.get_redis_url();
    let policies = app_settings.policies.clone();
    let pypi_index_rule = app_settings.builtin.clone().pypi_index;
    let pypi_pkg_rule = app_settings.builtin.clone().pypi_packages;
    let anaconda_rule = app_settings.builtin.clone().anaconda;

    let mut tm = TaskManager::new(app_settings.clone());

    let redis_client = redis::Client::open(redis_url).expect("failed to connect to redis");
    let pypi_index_cache =
        create_cache_from_rule(&pypi_index_rule, &policies, Some(redis_client.clone())).unwrap();
    let pypi_pkg_cache =
        create_cache_from_rule(&pypi_pkg_rule, &policies, Some(redis_client.clone())).unwrap();
    let anaconda_cache =
        create_cache_from_rule(&anaconda_rule, &policies, Some(redis_client.clone())).unwrap();

    tm.pypi_index_cache = pypi_index_cache;
    tm.pypi_pkg_cache = pypi_pkg_cache;
    tm.anaconda_cache = anaconda_cache;

    for (idx, rule) in app_settings.rules.iter().enumerate() {
        debug!("creating rule #{}: {:?}", idx, rule);
        let cache = create_cache_from_rule(rule, &policies, Some(redis_client.clone())).unwrap();
        tm.add_cache(idx, cache);
    }

    let shared_tm = Arc::new(tm);
    let api = filters::root(shared_tm);

    // init metrics
    let builder = PrometheusBuilder::new();
    builder
        .idle_timeout(
            MetricKindMask::COUNTER | MetricKindMask::HISTOGRAM,
            Some(std::time::Duration::from_secs(10)),
        )
        .listen_address(([127, 0, 0, 1], port + 1))
        .install()
        .expect("failed to install Prometheus recorder");
    metric::register_counters();
    // metric::R

    warp::serve(api).run(([127, 0, 0, 1], port)).await;
}

fn create_cache_from_rule(
    rule: &Rule,
    policies: &Vec<Policy>,
    redis_client: Option<redis::Client>,
) -> Result<Arc<dyn CachePolicy>> {
    let policy_ident = rule.policy.clone();
    for (idx, p) in policies.iter().enumerate() {
        if p.name == policy_ident {
            let policy_type = p.typ;
            match policy_type {
                PolicyType::Lru => {
                    return Ok(Arc::new(cache::LruRedisCache::new(
                        p.path.as_ref().unwrap(),
                        p.size.unwrap_or(0),
                        redis_client.unwrap(),
                        &format!("lru_rule{}", idx),
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

mod filters {
    use super::handlers;
    use super::*;
    use std::convert::Infallible;
    use warp::Filter;

    pub fn root(
        shared_tm: SharedTaskManager,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        let log = warp::log::custom(|info| {
            info!(
                "🌐 {} {} Response: {}",
                info.method(),
                info.path(),
                info.status(),
            );
        });

        pypi_index(shared_tm.clone())
            .or(pypi_packages(shared_tm.clone()))
            .or(anaconda_all(shared_tm.clone()))
            .or(fallback(shared_tm.clone()))
            .with(log)
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

    fn fallback(
        tm: SharedTaskManager,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::path::tail()
            .map(|tail: warp::filters::path::Tail| tail.as_str().to_string())
            .and(with_tm(tm))
            .and_then(handlers::fallback_handler)
    }
}

mod handlers {
    use super::*;
    use crate::task::Task;
    use std::result::Result;

    use warp::{http::Response, Rejection};

    pub async fn get_pypi_index(
        path: String,
        tm: SharedTaskManager,
    ) -> Result<impl warp::Reply, Rejection> {
        increment_counter!(metric::COUNTER_PYPI_INDEX_REQUESTS);
        let tw = Task::PypiIndexTask { pkg_name: path };
        match tw.resolve(tm).await {
            Ok(data) => {
                increment_counter!(metric::COUNTER_PYPI_INDEX_REQ_SUCCESS);
                Ok(Response::builder()
                    .header("content-type", "text/html")
                    .body(data))
            }
            Err(_) => {
                increment_counter!(metric::COUNTER_PYPI_INDEX_REQ_FAILURE);
                Err(warp::reject())
            }
        }
    }

    pub async fn get_pypi_pkg(
        seg0: String,
        seg1: String,
        seg2: String,
        seg3: String,
        tm: SharedTaskManager,
    ) -> Result<impl warp::Reply, Rejection> {
        increment_counter!(metric::COUNTER_PYPI_PKGS_REQ);
        let fullpath = format!("{}/{}/{}/{}", seg0, seg1, seg2, seg3);
        let t = Task::PypiPackagesTask { pkg_path: fullpath };
        match t.resolve(tm).await {
            Ok(data) => Ok(Response::builder().body(data)),
            Err(e) => {
                error!("{}", e);
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
        increment_counter!(metric::COUNTER_ANACONDA_REQ);
        let cache_key = format!("{}/{}/{}", channel, arch, filename);
        let t = Task::AnacondaTask { path: cache_key };
        match t.resolve(tm).await {
            Ok(data) => Ok(Response::builder().body(data)),
            Err(e) => {
                error!("{}", e);
                Err(warp::reject())
            }
        }
    }

    pub async fn fallback_handler(
        path: String,
        tm: SharedTaskManager,
    ) -> Result<impl warp::Reply, Rejection> {
        // Dynamically dispatch tasks defined in config file
        let config = &tm.config;
        for (idx, rule) in config.rules.iter().enumerate() {
            let upstream = rule.upstream.clone();
            if let Some(rule_regex) = &rule.path {
                let re = Regex::new(rule_regex).unwrap();
                if re.is_match(&path) {
                    trace!("captured by rule #{}: {}", idx, rule_regex);
                    let replaced = re.replace_all(&path, &upstream);
                    let t = Task::Others {
                        rule_id: idx,
                        url: String::from(replaced),
                    };
                    if let Ok(data) = t.resolve(tm.clone()).await {
                        return Ok(Response::builder().body(data));
                    }
                }
            }
        }
        Err(warp::reject())
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
        info!(
            "{:?}",
            settings::Settings::new_from("config-test", "app_test").unwrap()
        );
        settings::Settings::new_from("config-test", "app_test").unwrap()
    }

    fn get_filter_root() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone
    {
        let app_settings = get_settings();
        let tm = TaskManager::new(app_settings.clone());
        filters::root(Arc::new(tm))
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
