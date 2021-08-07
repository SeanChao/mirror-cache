use crate::cache;
use crate::cache::CachePolicy;
use crate::cache::NoCache;
use crate::error::Error::*;
use crate::error::Result;
use crate::settings::Settings;
use crate::util;
use reqwest::ClientBuilder;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;

pub type SharedTaskManager = Arc<TaskManager>;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Task {
    PypiIndexTask { pkg_name: String },
    PypiPackagesTask { pkg_path: String },
    AnacondaTask { path: String },
    Others { rule_id: RuleId, url: String },
}

impl Task {
    pub async fn resolve(&self, tm: Arc<TaskManager>) -> Result<cache::BytesArray> {
        // try get from cache
        let mut cache_result = None;
        let key = self.to_key();
        match &self {
            Task::PypiIndexTask { .. } => {
                if let Some(bytes) = tm.get(&self, &key) {
                    cache_result = Some(bytes)
                }
            }
            Task::PypiPackagesTask { .. } => {
                if let Some(bytes) = tm.get(&self, &key) {
                    cache_result = Some(bytes)
                }
            }
            Task::AnacondaTask { .. } => {
                if let Some(bytes) = tm.get(&self, &key) {
                    cache_result = Some(bytes)
                }
            }
            Task::Others { .. } => {
                if let Some(bytes) = tm.get(&self, &key) {
                    cache_result = Some(bytes);
                }
            }
        };
        if let Some(data) = cache_result {
            println!("[Request] {:?} [HIT]", &self);
            return Ok(data);
        }
        // cache miss, dispatch async cache task
        let _ = tm.add_task(self.clone()).await;
        // fetch from upstream
        let client = ClientBuilder::new().build().unwrap();
        let remote_url = tm.resolve_task_upstream(&self);
        println!("[Request] {:?} fetching from: {}", &self, &remote_url);
        let resp = client.get(remote_url).send().await;
        match resp {
            Ok(res) => match &self {
                Task::PypiIndexTask { .. } => {
                    let text_content = res.text().await.unwrap();
                    if let Some(url) = tm.config.url.clone() {
                        Ok(self
                            .rewrite_upstream(text_content, &url)
                            .as_bytes()
                            .to_vec())
                    } else {
                        Ok(text_content.as_bytes().to_vec())
                    }
                }
                _ => {
                    println!("[Request] {:?} ✔ fetched {:?}", &self, res.content_length());
                    Ok(res.bytes().await.unwrap().to_vec())
                }
            },
            Err(e) => {
                eprintln!("[Request] {:?} failed to fetch upstream: {}", &self, e);
                Err(RequestError(e))
            }
        }
    }

    pub fn rewrite_upstream(&self, input: String, to: &str) -> String {
        match &self {
            Task::PypiIndexTask { .. } => util::pypi_index_rewrite(&input, to),
            _ => input,
        }
    }

    pub fn to_key(&self) -> String {
        match &self {
            Task::PypiIndexTask { pkg_name, .. } => format!("pypi_index_{}", pkg_name),
            Task::PypiPackagesTask { pkg_path, .. } => String::from(pkg_path),
            Task::AnacondaTask { path, .. } => format!("anaconda_{}", path),
            Task::Others { url, .. } => url
                .replace("http://", "http/")
                .replace("https://", "https/"),
        }
    }
}

pub type RuleId = usize;

pub struct TaskManager {
    pub config: Settings,
    pub pypi_index_cache: Arc<dyn CachePolicy>,
    pub pypi_pkg_cache: Arc<dyn CachePolicy>,
    pub anaconda_cache: Arc<dyn CachePolicy>,
    pub cache_map: HashMap<RuleId, Arc<dyn CachePolicy>>,
    task_set: Arc<RwLock<HashSet<Task>>>,
}

impl TaskManager {
    pub fn new(config: Settings) -> Self {
        TaskManager {
            config,
            pypi_index_cache: Arc::new(NoCache {}),
            pypi_pkg_cache: Arc::new(NoCache {}),
            anaconda_cache: Arc::new(NoCache {}),
            cache_map: HashMap::new(),
            task_set: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    // add a task into task list
    async fn add_task(&self, task: Task) {
        if self.task_set.read().await.contains(&task) {
            println!("[TASK] ignored existing task: {:?}", task);
            return;
        }
        self.task_set.write().await.insert(task.clone());
        println!(
            "[TASK] [len={}] + {:?}",
            self.task_set.read().await.len(),
            task
        );
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
            Task::AnacondaTask { .. } => {
                c = self.anaconda_cache.clone();
            }
            Task::Others { rule_id, .. } => {
                c = self.get_cache_for_cache_rule(*rule_id).unwrap();
            }
        };
        let task_clone = task.clone();
        let upstream_url = self.resolve_task_upstream(&task_clone);
        let task_list_ptr = self.task_set.clone();
        tokio::spawn(async move {
            let client = ClientBuilder::new().build().unwrap();
            let resp = client.get(upstream_url).send().await;
            match resp {
                Ok(res) => {
                    if rewrite {
                        let mut content = res.text().await.unwrap();
                        if let Some(to_url) = to_url {
                            content = task_clone.rewrite_upstream(content, &to_url);
                        };
                        c.put(&task_clone.to_key(), content.as_bytes().to_vec());
                    } else {
                        c.put(&task_clone.to_key(), res.bytes().await.unwrap().to_vec());
                    }
                    println!("[TASK] ✔ {:?}", task_clone);
                }
                Err(e) => {
                    eprintln!("[TASK] ❌ failed to fetch upstream: {}", e);
                }
            };
            task_list_ptr.write().await.remove(&task_clone);
        });
    }

    pub fn get(&self, task_type: &Task, key: &str) -> Option<cache::BytesArray> {
        match &task_type {
            Task::PypiIndexTask { .. } => self.pypi_index_cache.get(key),
            Task::PypiPackagesTask { .. } => self.pypi_pkg_cache.get(key),
            Task::AnacondaTask { .. } => self.anaconda_cache.get(key),
            Task::Others { rule_id, .. } => match self.get_cache_for_cache_rule(*rule_id) {
                Some(cache) => cache.get(key),
                None => {
                    eprintln!("Failed to get cache for rule #{} from cache map", rule_id);
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
            Task::AnacondaTask { path } => {
                format!("{}/{}", &self.config.builtin.anaconda.upstream, path)
            }
            Task::Others { url, .. } => url.clone(),
        }
    }

    pub fn add_cache(&mut self, rule_id: RuleId, cache: Arc<dyn CachePolicy>) {
        // insert cache into cache map
        self.cache_map.insert(rule_id, cache);
    }

    pub fn get_cache_for_cache_rule(&self, rule_id: RuleId) -> Option<Arc<dyn CachePolicy>> {
        match self.cache_map.get(&rule_id) {
            Some(cache) => Some(cache.clone()),
            None => None,
        }
    }
}