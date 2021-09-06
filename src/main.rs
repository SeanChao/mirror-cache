mod cache;
mod error;
mod metric;
mod models;
mod settings;
mod storage;
mod task;
mod util;

use crate::task::TaskManager;

// use metrics::increment_counter;
use metrics_exporter_prometheus::PrometheusBuilder;
use metrics_util::MetricKindMask;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use regex::{Regex, RegexSet};
use std::path::Path;
use tokio::sync::RwLock;

#[macro_use]
extern crate serde_derive;

extern crate pretty_env_logger;
#[macro_use]
extern crate log;

pub type LockedSharedTaskManager = RwLock<TaskManager>;

lazy_static::lazy_static! {
    /// A regular expression set of all specified rule paths.
    static ref RE_SET: RwLock<RegexSet> = {
        let app_settings = settings::Settings::new().unwrap();
        let rules: Vec<String> = app_settings.rules.iter().map(|rule| rule.path.clone()).collect();
        RwLock::new(RegexSet::new(&rules).unwrap())
    };
    /// Global task manager.
    static ref TASK_MANAGER: LockedSharedTaskManager = {
        let app_settings = settings::Settings::new().unwrap();
        let mut tm = TaskManager::new(app_settings.clone());
        tm.refresh_config(&app_settings);
        RwLock::new(tm)
    };
}

#[tokio::main]
async fn main() {
    let app_settings = settings::Settings::new().unwrap();
    let port = app_settings.port;
    let api = filters::root();

    // initialize the logger
    let mut log_builder = pretty_env_logger::formatted_builder();
    log_builder
        .filter_module("hyper::proto", log::LevelFilter::Error) // hide excessive logs
        .filter_module("tracing::span", log::LevelFilter::Error)
        .filter_module("tokio_util::codec", log::LevelFilter::Error)
        .filter_level(app_settings.get_log_level())
        .init();

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

    // Watcher::
    // We listen to file changes by giving Notify
    // a function that will get called when events happen
    let mut watcher =
        // To make sure that the config lives as long as the function
        // we need to move the ownership of the config inside the function
        // To learn more about move please read [Using move Closures with Threads](https://doc.rust-lang.org/book/ch16-01-threads.html?highlight=move#using-move-closures-with-threads)
        RecommendedWatcher::new(move |result: std::result::Result<Event, notify::Error>| {
            let event = result.unwrap();
            if event.kind.is_modify() {
                util::sleep_ms(2000);
                // update config:
                futures::executor::block_on(async {
                    match settings::Settings::new() {
                        Ok(settings) => {
                            TASK_MANAGER.write().await.refresh_config(&settings);
                            let rules: Vec<String> = settings.rules.iter().map(|rule| rule.path.clone()).collect();
                            let mut new_re_set = RE_SET.write().await;
                            *new_re_set = RegexSet::new(&rules).unwrap();
                            info!("config updated");
                        },
                        Err(e) => {
                            error!("Failed to load config: {}. Use the original config.", e);
                        }
                    }
                })
            }
        }).unwrap();
    watcher
        .watch(Path::new("config.yml"), RecursiveMode::Recursive)
        .unwrap();

    warp::serve(api).run(([127, 0, 0, 1], port)).await;
}

mod filters {
    use super::*;
    use warp::Filter;

    pub fn root() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        let log = warp::log::custom(|info| {
            info!(
                "🌐 {} {} Response: {}",
                info.method(),
                info.path(),
                info.status(),
            );
        });

        fallback().with(log)
    }

    /// fallback handler, matches all paths
    fn fallback() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::path::tail()
            .map(|tail: warp::filters::path::Tail| tail.as_str().to_string())
            .and_then(handlers::fallback_handler)
    }
}

mod handlers {
    use super::*;
    use crate::error::Error;
    use crate::task::Task;
    use std::result::Result;
    use warp::Rejection;
    use warp::Reply;

    pub async fn fallback_handler(path: String) -> Result<impl warp::Reply, Rejection> {
        // Dynamically dispatch tasks defined in config file
        let tm = TASK_MANAGER.read().await.clone();
        let config = &tm.config;
        let rules_regex_set = RE_SET.read().await;
        let matched_indices: Vec<usize> = rules_regex_set.matches(&path).into_iter().collect();
        if matched_indices.len() == 0 {
            // No matching rule
            return Err(warp::reject());
        }
        let idx = *matched_indices.first().unwrap();
        let rule = config.rules.get(idx).unwrap();

        let upstream = rule.upstream.clone();
        let re = Regex::new(&rule.path).unwrap();
        trace!("matched by rule #{}: {}", idx, &rule.path);
        let replaced = re.replace_all(&path, &upstream);
        let task = Task::Others {
            rule_id: idx,
            url: String::from(replaced),
        };
        match tm.resolve_task(&task).await {
            Ok(data) => {
                let mut resp = data.into_response();
                if let Some(options) = &rule.options {
                    if let Some(content_type) = &options.content_type {
                        resp = warp::reply::with_header(resp, "content-type", content_type)
                            .into_response();
                    }
                }
                return Ok(resp);
            }
            Err(e) => match e {
                Error::UpstreamRequestError(res) => {
                    let resp = warp::http::Response::builder()
                        .status(res.status())
                        .body(res.bytes().await.unwrap().into())
                        .unwrap();
                    Ok(resp)
                }
                _ => Err(warp::reject::custom(e)),
            },
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

    async fn setup() {
        TASK_MANAGER.write().await.refresh_config(&get_settings());
    }

    fn get_settings() -> Settings {
        settings::Settings::new_from("config-test", "app_test").unwrap()
    }

    fn get_filter_root() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone
    {
        filters::root()
    }

    #[tokio::test]
    async fn get_pypi_index() {
        setup().await;
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
        assert!(resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("text/html"));
        // webpage fetched successfully
        assert!(resp_text.contains(&format!("Links for {}", pkg_name)));
        // target link is replaced successfully
        assert!(resp_text.contains(&app_url));
    }
}
