use crate::cache::{CacheData, CachePolicy, LruRedisCache, NoCache, TtlRedisCache};
use crate::error::Error;
use crate::error::Result;
use crate::metric;
use crate::settings::Settings;
use crate::settings::{Policy, PolicyType};
use crate::util;

use bytes::Bytes;
use futures::Stream;
use futures::StreamExt;
use metrics::{histogram, increment_counter};
use std::collections::HashMap;
use std::collections::HashSet;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Task {
    PypiIndexTask { pkg_name: String },
    PypiPackagesTask { pkg_path: String },
    AnacondaIndexTask { path: String },
    AnacondaPackagesTask { path: String },
    Others { rule_id: RuleId, url: String },
}

pub enum TaskResponse {
    StringResponse(String),
    BytesResponse(Bytes),
    StreamResponse(Pin<Box<dyn Stream<Item = Result<Bytes>> + Send>>),
    // RawResponse(impl warp::Reply)
    Redirect(warp::reply::WithHeader<warp::http::StatusCode>),
}

impl From<String> for TaskResponse {
    fn from(s: String) -> TaskResponse {
        TaskResponse::StringResponse(s)
    }
}

impl From<CacheData> for TaskResponse {
    fn from(cache_data: CacheData) -> TaskResponse {
        match cache_data {
            CacheData::TextData(text) => text.into(),
            CacheData::BytesData(bytes) => TaskResponse::BytesResponse(bytes),
            CacheData::ByteStream(stream, ..) => TaskResponse::StreamResponse(Box::pin(stream)),
        }
    }
}

impl warp::Reply for TaskResponse {
    fn into_response(self) -> warp::reply::Response {
        match self {
            TaskResponse::StringResponse(content) => warp::reply::Response::new(content.into()),
            TaskResponse::BytesResponse(bytes) => warp::reply::Response::new(bytes.into()),
            TaskResponse::StreamResponse(stream) => {
                warp::reply::Response::new(warp::hyper::Body::wrap_stream(stream))
            }
            TaskResponse::Redirect(r) => r.into_response(),
        }
    }
}

impl Task {
    pub fn rewrite_upstream(&self, input: String, to: &str) -> String {
        match &self {
            Task::PypiIndexTask { .. } => util::pypi_index_rewrite(&input, to),
            _ => input,
        }
    }

    /// create a unique key for the current task
    pub fn to_key(&self) -> String {
        match &self {
            Task::PypiIndexTask { pkg_name, .. } => format!("pypi_index_{}", pkg_name),
            Task::PypiPackagesTask { pkg_path, .. } => String::from(pkg_path),
            Task::AnacondaIndexTask { path, .. } => format!("anaconda_index_{}", path),
            Task::AnacondaPackagesTask { path, .. } => format!("anaconda_{}", path),
            Task::Others { url, .. } => url
                .replace("http://", "http/")
                .replace("https://", "https/"),
        }
    }
}

pub type RuleId = usize;

#[derive(Clone)]
pub struct TaskManager {
    pub config: Settings,
    pub pypi_index_cache: Arc<dyn CachePolicy>,
    pub pypi_pkg_cache: Arc<dyn CachePolicy>,
    pub anaconda_index_cache: Arc<dyn CachePolicy>,
    pub anaconda_pkg_cache: Arc<dyn CachePolicy>,
    /// RuleId -> (cache, size_limit)
    pub rule_map: HashMap<RuleId, (Arc<dyn CachePolicy>, usize)>,
    task_set: Arc<RwLock<HashSet<Task>>>,
}

impl TaskManager {
    pub fn new(config: Settings) -> Self {
        TaskManager {
            config,
            pypi_index_cache: Arc::new(NoCache {}),
            pypi_pkg_cache: Arc::new(NoCache {}),
            anaconda_index_cache: Arc::new(NoCache {}),
            anaconda_pkg_cache: Arc::new(NoCache {}),
            rule_map: HashMap::new(),
            task_set: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    pub async fn resolve_task(&self, task: &Task) -> Result<TaskResponse> {
        // try get from cache
        let mut cache_result = None;
        let key = task.to_key();
        match &task {
            Task::PypiIndexTask { .. } => {
                if let Some(bytes) = self.get(&task, &key).await {
                    increment_counter!(metric::CNT_PYPI_INDEX_CACHE_HIT);
                    cache_result = Some(bytes)
                } else {
                    increment_counter!(metric::CNT_PYPI_INDEX_CACHE_MISS);
                }
            }
            Task::PypiPackagesTask { .. } => {
                if let Some(bytes) = self.get(&task, &key).await {
                    increment_counter!(metric::CNT_PYPI_PKGS_CACHE_HIT);
                    cache_result = Some(bytes)
                } else {
                    increment_counter!(metric::CNT_PYPI_PKGS_CACHE_MISS);
                }
            }
            Task::AnacondaIndexTask { .. } => {
                if let Some(bytes) = self.get(&task, &key).await {
                    increment_counter!(metric::CNT_ANACONDA_CACHE_HIT);
                    cache_result = Some(bytes)
                } else {
                    increment_counter!(metric::CNT_ANACONDA_CACHE_MISS);
                }
            }
            Task::AnacondaPackagesTask { .. } => {
                if let Some(bytes) = self.get(&task, &key).await {
                    increment_counter!(metric::CNT_ANACONDA_CACHE_HIT);
                    cache_result = Some(bytes)
                } else {
                    increment_counter!(metric::CNT_ANACONDA_CACHE_MISS);
                }
            }
            Task::Others { .. } => {
                if let Some(bytes) = self.get(&task, &key).await {
                    cache_result = Some(bytes);
                }
            }
        };
        if let Some(data) = cache_result {
            info!("[Request] [HIT] {:?}", &task);
            increment_counter!(metric::COUNTER_CACHE_HIT);
            return Ok(data.into());
        }
        increment_counter!(metric::COUNTER_CACHE_MISS);
        // cache miss
        // fetch from upstream
        let remote_url = self.resolve_task_upstream(&task);
        info!(
            "[Request] [MISS] {:?}, fetching from upstream: {}",
            &task, &remote_url
        );
        let resp = util::make_request(&remote_url).await;
        match resp {
            Ok(res) => {
                if !res.status().is_success() {
                    return Err(Error::UpstreamError(
                        res.status().canonical_reason().unwrap_or("unknown").into(),
                    ));
                }
                // if the response is too large, respond users with a redirect to upstream
                if let Some(content_length) = res.content_length() {
                    let size_limit = self.get_task_size_limit(&task);
                    if size_limit != 0 && size_limit < content_length as usize {
                        return Ok(TaskResponse::Redirect(warp::reply::with_header(
                            warp::http::StatusCode::FOUND,
                            "Location",
                            remote_url,
                        )));
                    }
                }
                // dispatch async cache task
                let _ = self.spawn_task(task.clone()).await;
                match &task {
                    Task::PypiIndexTask { .. } => {
                        let text_content = res.text().await.unwrap();
                        if let Some(url) = self.config.url.clone() {
                            Ok(task.rewrite_upstream(text_content, &url).into())
                        } else {
                            Ok(text_content.into())
                        }
                    }
                    _ => Ok(TaskResponse::StreamResponse(Box::pin(
                        res.bytes_stream()
                            .map(move |x| x.map_err(|e| Error::RequestError(e))),
                    ))),
                }
            }
            Err(e) => {
                error!("[Request] {:?} failed to fetch upstream: {}", &task, e);
                Err(e)
            }
        }
    }

    /// for each rule, create associated cache if the policy has not been created
    pub fn refresh_config(&mut self, settings: &Settings) {
        let app_settings = settings;
        let redis_url = app_settings.get_redis_url();
        let policies = app_settings.policies.clone();
        let pypi_index_rule = app_settings.builtin.clone().pypi_index;
        let pypi_pkg_rule = app_settings.builtin.clone().pypi_packages;
        let anaconda_index_rule = app_settings.builtin.clone().anaconda_index;
        let anaconda_pkg_rule = app_settings.builtin.clone().anaconda_packages;

        let mut tm = self;
        tm.config = app_settings.clone();

        let mut policy_map: HashSet<String> = HashSet::new(); // used to avoid create duplicated cache if some rules share the same policy
                                                              // get active policy set
        policy_map.insert(pypi_index_rule.policy.clone());
        policy_map.insert(pypi_pkg_rule.policy.clone());
        policy_map.insert(anaconda_index_rule.policy.clone());
        policy_map.insert(anaconda_pkg_rule.policy.clone());
        for rule in &app_settings.rules {
            policy_map.insert(rule.policy.clone());
        }

        let mut cache_map: HashMap<String, Arc<dyn CachePolicy>> = HashMap::new();
        let redis_client = redis::Client::open(redis_url).expect("failed to connect to redis");
        // create cache for each policy
        for policy in &policy_map {
            let cache =
                Self::create_cache_from_rule(&policy, &policies, Some(redis_client.clone()));
            cache_map.insert(policy.to_string(), cache.unwrap());
        }

        let pypi_index_cache = cache_map.get(&pypi_index_rule.policy).unwrap().clone();
        let pypi_pkg_cache = cache_map.get(&pypi_pkg_rule.policy).unwrap().clone();
        let anaconda_index_cache = cache_map.get(&anaconda_index_rule.policy).unwrap().clone();
        let anaconda_pkg_cache = cache_map.get(&anaconda_pkg_rule.policy).unwrap().clone();
        tm.pypi_index_cache = pypi_index_cache;
        tm.pypi_pkg_cache = pypi_pkg_cache;
        tm.anaconda_index_cache = anaconda_index_cache;
        tm.anaconda_pkg_cache = anaconda_pkg_cache;

        tm.rule_map.clear();
        for (idx, rule) in app_settings.rules.iter().enumerate() {
            debug!("creating rule #{}: {:?}", idx, rule);
            let cache = cache_map.get(&rule.policy).unwrap().clone();
            tm.rule_map.insert(
                idx,
                (
                    cache,
                    rule.size_limit
                        .clone()
                        .map_or(0, |x| bytefmt::parse(x).unwrap() as usize),
                ),
            );
        }
    }

    fn create_cache_from_rule(
        policy_name: &str,
        policies: &Vec<Policy>,
        redis_client: Option<redis::Client>,
    ) -> Result<Arc<dyn CachePolicy>> {
        let policy_ident = policy_name;
        for (idx, p) in policies.iter().enumerate() {
            if p.name == policy_ident {
                let policy_type = p.typ;
                match policy_type {
                    PolicyType::Lru => {
                        return Ok(Arc::new(LruRedisCache::new(
                            p.path.as_ref().unwrap(),
                            p.size.unwrap_or(0),
                            redis_client.unwrap(),
                            &format!("lru_rule_{}", idx),
                        )));
                    }
                    PolicyType::Ttl => {
                        return Ok(Arc::new(TtlRedisCache::new(
                            p.path.as_ref().unwrap(),
                            p.timeout.unwrap_or(0),
                            redis_client.unwrap(),
                            &format!("ttl_rule_{}", idx),
                        )));
                    }
                };
            }
        }
        Err(Error::ConfigInvalid(format!(
            "No such policy: {}",
            policy_ident
        )))
    }

    async fn taskset_contains(&self, t: &Task) -> bool {
        self.task_set.read().await.contains(t)
    }

    async fn taskset_add(&self, t: Task) {
        self.task_set.write().await.insert(t);
    }

    async fn taskset_remove(task_set: Arc<RwLock<HashSet<Task>>>, t: &Task) {
        task_set.write().await.remove(t);
    }

    async fn taskset_len(task_set: Arc<RwLock<HashSet<Task>>>) -> usize {
        let len = task_set.read().await.len();
        histogram!(metric::HG_TASKS_LEN, len as f64);
        len
    }

    /// Spawn an async task
    async fn spawn_task(&self, task: Task) {
        increment_counter!(metric::COUNTER_TASKS_BG);
        if self.taskset_contains(&task).await {
            info!("[TASK] ignored existing task: {:?}", task);
            return;
        }
        self.taskset_add(task.clone()).await;
        let task_set_len = Self::taskset_len(self.task_set.clone()).await;
        info!("[TASK] [len={}] + {:?}", task_set_len, task);
        let c;
        let mut rewrite = false;
        let mut to_url = None;
        match &task {
            Task::PypiIndexTask { .. } => {
                c = self.pypi_index_cache.clone();
                to_url = self.config.url.clone();
                rewrite = true;
            }
            Task::PypiPackagesTask { .. } => {
                c = self.pypi_pkg_cache.clone();
            }
            Task::AnacondaIndexTask { .. } => {
                c = self.anaconda_index_cache.clone();
            }
            Task::AnacondaPackagesTask { .. } => {
                c = self.anaconda_pkg_cache.clone();
            }
            Task::Others { rule_id, .. } => {
                c = self.get_cache_for_cache_rule(*rule_id).unwrap();
            }
        };
        let task_clone = task.clone();
        let upstream_url = self.resolve_task_upstream(&task_clone);
        let task_list_ptr = self.task_set.clone();
        // spawn an async download task
        tokio::spawn(async move {
            let resp = util::make_request(&upstream_url).await;
            match resp {
                Ok(res) => {
                    if res.status().is_success() {
                        if rewrite {
                            let content = res.text().await.ok();
                            if content.is_none() {
                                increment_counter!(metric::CNT_TASKS_BG_FAILURE);
                                return;
                            }
                            let mut content = content.unwrap();
                            if let Some(to_url) = to_url {
                                content = task_clone.rewrite_upstream(content, &to_url);
                            };
                            c.put(&task_clone.to_key(), content.into()).await;
                        } else {
                            let len = res.content_length();
                            let bytestream = res.bytes_stream();
                            c.put(
                                &task_clone.to_key(),
                                CacheData::ByteStream(
                                    Box::new(
                                        bytestream
                                            .map(move |x| x.map_err(|e| Error::RequestError(e))),
                                    ),
                                    len.map(|x| x as usize),
                                ),
                            )
                            .await;
                        }
                        increment_counter!(metric::CNT_TASKS_BG_SUCCESS);
                    } else {
                        warn!(
                            "[TASK] ❌ failed to fetch upstream: {}, Task {:?}",
                            res.status().canonical_reason().unwrap_or("unknown"),
                            &task_clone
                        );
                        increment_counter!(metric::CNT_TASKS_BG_FAILURE);
                    }
                }
                Err(e) => {
                    increment_counter!(metric::CNT_TASKS_BG_FAILURE);
                    error!(
                        "[TASK] ❌ failed to fetch upstream: {}, Task {:?}",
                        e, &task_clone
                    );
                }
            };
            Self::taskset_remove(task_list_ptr.clone(), &task_clone).await;
            Self::taskset_len(task_list_ptr).await;
        });
    }

    /// get task result from cache
    pub async fn get(&self, task_type: &Task, key: &str) -> Option<CacheData> {
        match &task_type {
            Task::PypiIndexTask { .. } => self.pypi_index_cache.get(key).await,
            Task::PypiPackagesTask { .. } => self.pypi_pkg_cache.get(key).await,
            Task::AnacondaIndexTask { .. } => self.anaconda_index_cache.get(key).await,
            Task::AnacondaPackagesTask { .. } => self.anaconda_pkg_cache.get(key).await,
            Task::Others { rule_id, .. } => match self.get_cache_for_cache_rule(*rule_id) {
                Some(cache) => cache.get(key).await,
                None => {
                    error!("Failed to get cache for rule #{} from cache map", rule_id);
                    None
                }
            },
        }
    }

    pub fn resolve_task_upstream(&self, task_type: &Task) -> String {
        match &task_type {
            Task::PypiIndexTask { pkg_name, .. } => {
                format!("{}/{}", &self.config.builtin.pypi_index.upstream, pkg_name)
            }
            Task::PypiPackagesTask { pkg_path, .. } => format!(
                "{}/{}",
                &self.config.builtin.pypi_packages.upstream, pkg_path
            ),
            Task::AnacondaIndexTask { path } => {
                format!("{}/{}", &self.config.builtin.anaconda_index.upstream, path)
            }
            Task::AnacondaPackagesTask { path } => {
                format!(
                    "{}/{}",
                    &self.config.builtin.anaconda_packages.upstream, path
                )
            }
            Task::Others { url, .. } => url.clone(),
        }
    }

    pub fn get_cache_for_cache_rule(&self, rule_id: RuleId) -> Option<Arc<dyn CachePolicy>> {
        match self.rule_map.get(&rule_id) {
            Some(tuple) => Some(tuple.0.clone()),
            None => None,
        }
    }

    pub fn get_task_size_limit(&self, task: &Task) -> usize {
        match task {
            Task::PypiIndexTask { .. } => self
                .config
                .builtin
                .pypi_index
                .size_limit
                .as_ref()
                .map_or(0, |x| bytefmt::parse(x).unwrap() as usize),
            Task::PypiPackagesTask { .. } => self
                .config
                .builtin
                .pypi_packages
                .size_limit
                .as_ref()
                .map_or(0, |x| bytefmt::parse(x).unwrap() as usize),
            Task::AnacondaIndexTask { .. } => self
                .config
                .builtin
                .anaconda_index
                .size_limit
                .as_ref()
                .map_or(0, |x| bytefmt::parse(x).unwrap() as usize),
            Task::AnacondaPackagesTask { .. } => self
                .config
                .builtin
                .anaconda_packages
                .size_limit
                .as_ref()
                .map_or(0, |x| bytefmt::parse(x).unwrap() as usize),
            Task::Others { rule_id, .. } => self.rule_map.get(&rule_id).unwrap().1,
        }
    }
}
