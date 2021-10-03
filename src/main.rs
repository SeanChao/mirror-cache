mod cache;
mod error;
mod metric;
mod models;
mod settings;
mod storage;
mod task;
mod util;

use cache::CacheHitMiss;
use clap::{crate_version, App, Arg};
use metrics::{increment_counter, register_counter};
use metrics_exporter_prometheus::PrometheusBuilder;
use metrics_util::MetricKindMask;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use regex::{Regex, RegexSet};
use settings::{rule_label, Rule};
use std::path::Path;
use task::TaskManager;
use tokio::sync::RwLock;

#[macro_use]
extern crate serde_derive;

extern crate pretty_env_logger;
#[macro_use]
extern crate log;

pub type LockedSharedTaskManager = RwLock<TaskManager>;

lazy_static::lazy_static! {
    /// A regular expression set of all specified rule paths and a list of Regex
    /// As suggest in regex documentation of `RegexSet`:
    /// Other features like finding the location of successive matches or their
    /// sub-captures aren’t supported. If you need this functionality, the
    /// recommended approach is to compile each regex in the set independently
    /// and selectively match them based on which regexes in the set matched.
    static ref RE_SET_LIST: RwLock<(RegexSet, Vec<Regex>)> = {
        RwLock::new((RegexSet::empty(), vec![]))
    };
    /// Global task manager.
    static ref TASK_MANAGER: LockedSharedTaskManager = {
        RwLock::new(TaskManager::empty())
    };
}

#[tokio::main]
async fn main() {
    let matches = App::new("mirror-cache")
        .version(crate_version!())
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .value_name("FILE")
                .help("Sets a custom config file. Default config.yml")
                .takes_value(true),
        )
        .get_matches();
    debug!("CLI args: {:?}", matches);
    let config_filename = matches
        .value_of("config")
        .unwrap_or("config.yml")
        .to_string();

    let app_settings = settings::Settings::new(&config_filename).unwrap();
    let port = app_settings.port;
    let metrics_port = app_settings.metrics_port;
    let hot_reload = app_settings.hot_reload.unwrap_or(false);
    let api = filters::root();

    // initialize the logger
    let mut log_builder = pretty_env_logger::formatted_builder();
    log_builder
        .filter_module("hyper::proto", log::LevelFilter::Error) // hide excessive logs
        .filter_module("tracing::span", log::LevelFilter::Error)
        .filter_module("tokio_util::codec", log::LevelFilter::Error)
        .filter_module("sled::pagecache", log::LevelFilter::Error)
        .filter_level(app_settings.get_log_level())
        .init();

    // initialize global static TASK_MANAGER and RE_SET_LIST
    let mut tm = TaskManager::new(app_settings.clone());
    tm.refresh_config(&app_settings);
    {
        let mut global_tm = TASK_MANAGER.write().await;
        *global_tm = tm;
        let mut global_re_set_list = RE_SET_LIST.write().await;
        *global_re_set_list = create_re_set_list(&app_settings.rules);
    }

    // init metrics
    let builder = PrometheusBuilder::new();
    builder
        .idle_timeout(
            MetricKindMask::COUNTER | MetricKindMask::HISTOGRAM,
            Some(std::time::Duration::from_secs(10)),
        )
        .listen_address(([127, 0, 0, 1], metrics_port))
        .install()
        .expect("failed to install Prometheus recorder");
    metric::register_counters();
    register_rules_metrics(&app_settings.rules);

    if hot_reload {
        // Watcher::
        // We listen to file changes by giving Notify
        // a function that will get called when events happen.
        // To make sure that the config lives as long as the function
        // we need to move the ownership of the config inside the function
        // To learn more about move please read [Using move Closures with Threads](https://doc.rust-lang.org/book/ch16-01-threads.html?highlight=move#using-move-closures-with-threads)
        let config_filename_clone = config_filename.clone();
        let mut watcher =
            RecommendedWatcher::new(move |result: std::result::Result<Event, notify::Error>| {
                file_watch_handler(&config_filename_clone, result)
            })
            .unwrap();
        watcher
            .watch(Path::new(&config_filename), RecursiveMode::Recursive)
            .unwrap();
        info!(
            "Configuration hot reloading is enabled! Watching: {}",
            &config_filename
        );
    }

    warp::serve(api).run(([127, 0, 0, 1], port)).await;
}

fn file_watch_handler(config_filename: &str, result: std::result::Result<Event, notify::Error>) {
    let event = result.unwrap();
    println!(" -- {:?}", event);
    if event.kind.is_modify() {
        util::sleep_ms(2000);
        // update config:
        futures::executor::block_on(async {
            match settings::Settings::new(config_filename) {
                Ok(settings) => {
                    TASK_MANAGER.write().await.refresh_config(&settings);
                    let mut re_set_list = RE_SET_LIST.write().await;
                    *re_set_list = create_re_set_list(&settings.rules);
                    register_rules_metrics(&settings.rules);
                    info!("config updated");
                }
                Err(e) => {
                    error!("Failed to load config: {}. Use the original config.", e);
                }
            }
        })
    }
}

fn create_re_set_list(rules: &[Rule]) -> (RegexSet, Vec<Regex>) {
    let rules_strings: Vec<String> = rules.iter().map(|rule| rule.path.clone()).collect();
    let set = RegexSet::new(&rules_strings).unwrap();
    let list = rules
        .iter()
        .map(|rule| Regex::new(&rule.path).unwrap())
        .collect();
    (set, list)
}

/// Register metrics for each rule.
/// - counter - cache hit
/// - counter - cache miss
/// - counter - incoming requests
/// - counter - successful requests
/// - counter - failed requests
fn register_rules_metrics(rules: &[Rule]) {
    for rule in rules {
        register_counter!(metric::COUNTER_CACHE_HIT, "Cache hit count", "rule" => rule_label(rule));
        register_counter!(metric::COUNTER_CACHE_MISS, "Cache miss count", "rule" => rule_label(rule));
        register_counter!(metric::COUNTER_REQ, "Incoming requests count", "rule" => rule_label(rule));
        register_counter!(metric::COUNTER_REQ_SUCCESS, "Incoming requests count (success)", "rule" => rule_label(rule));
        register_counter!(metric::COUNTER_REQ_FAILURE, "Incoming requests count (failure)", "rule" => rule_label(rule));
    }
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
        let rules_regex_set_list = RE_SET_LIST.read().await;
        let matched_indices: Vec<usize> =
            rules_regex_set_list.0.matches(&path).into_iter().collect();
        if matched_indices.is_empty() {
            // No matching rule
            return Err(warp::reject());
        }
        let idx = *matched_indices.first().unwrap();
        let re = &rules_regex_set_list.1[idx];
        let rule = config.rules.get(idx).unwrap();
        let upstream = rule.upstream.clone();
        trace!("matched by rule #{}: {}", idx, &rule.path);
        increment_counter!(metric::COUNTER_REQ, "rule" => rule_label(rule));
        let replaced = re.replace_all(&path, &upstream);
        let task = Task {
            rule_id: idx,
            url: String::from(replaced),
        };
        let tm_resp = tm.resolve_task(&task).await;
        match tm_resp.1 {
            CacheHitMiss::Hit => {
                increment_counter!(metric::COUNTER_CACHE_HIT, "rule" => rule_label(rule))
            }
            CacheHitMiss::Miss => {
                increment_counter!(metric::COUNTER_CACHE_MISS, "rule" => rule_label(rule))
            }
        };
        match tm_resp.0 {
            Ok(data) => {
                let mut resp = data.into_response();
                if let Some(options) = &rule.options {
                    if let Some(content_type) = &options.content_type {
                        resp = warp::reply::with_header(resp, "content-type", content_type)
                            .into_response();
                    }
                }
                increment_counter!(metric::COUNTER_REQ_FAILURE, "rule" => rule_label(rule));
                Ok(resp)
            }
            Err(e) => {
                increment_counter!(metric::COUNTER_REQ_FAILURE, "rule" => rule_label(rule));
                match e {
                    Error::UpstreamRequestError(res) => {
                        let resp = warp::http::Response::builder()
                            .status(res.status())
                            .body(res.bytes().await.unwrap().into())
                            .unwrap();
                        Ok(resp)
                    }
                    _ => Err(warp::reject::custom(e)),
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::settings::Settings;
    use lazy_static::lazy_static;
    use warp::http::StatusCode;
    use warp::test::request;
    use warp::Filter;

    async fn setup() {
        lazy_static! {
            /// Initialize logger only once.
            static ref LOGGER: () = {
                let mut log_builder = pretty_env_logger::formatted_builder();
                log_builder
                    .filter_module("sled", log::LevelFilter::Info)
                    .filter_level(log::LevelFilter::Trace)
                    .target(pretty_env_logger::env_logger::Target::Stdout)
                    .init();
            };
        };

        let _ = &LOGGER;
        let settings = get_settings();
        TASK_MANAGER.write().await.refresh_config(&settings);
        let mut global_re_set_list = RE_SET_LIST.write().await;
        *global_re_set_list = create_re_set_list(&settings.rules);
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
        assert!(resp_text.contains("http://localhost:9001/pypi"));
    }
}
