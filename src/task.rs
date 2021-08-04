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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Task {
    PypiIndexTask { pkg_name: String, upstream: String },
    PypiPackagesTask { pkg_path: String },
    AnacondaTask { path: String },
    _Others,
}

impl Task {
    pub async fn resolve(&self, tm: &TaskManager) -> Result<cache::BytesArray> {
        // cache
        let mut cache_result = None;
        let mut relative_path = String::new();
        match &self {
            Task::PypiIndexTask { pkg_name, .. } => {
                let key = format!("pypi_index_{}", pkg_name);
                relative_path = key.clone();
                if let Some(bytes) = tm.get(&self, &key) {
                    cache_result = Some(bytes)
                }
            }
            Task::PypiPackagesTask { pkg_path, .. } => {
                relative_path = pkg_path.clone();
                if let Some(bytes) = tm.get(&self, &pkg_path) {
                    cache_result = Some(bytes)
                }
            }
            Task::AnacondaTask { path, .. } => {
                let key = format!("anaconda_index_{}", path);
                relative_path = path.clone();
                if let Some(bytes) = tm.get(&self, &key) {
                    cache_result = Some(bytes)
                }
            }
            _ => (),
        };
        if let Some(data) = cache_result {
            println!("[HIT]");
            return Ok(data);
        }
        // dispatch a cache task
        println!("add_task");
        let _f = tm.add_task(self.clone()).await;
        // cache miss, fetch from upstream
        let client = ClientBuilder::new().build().unwrap();
        let remote_url = tm.resolve_task_upstream(&self, &relative_path);
        println!("[Proxy] fetching from: {}", &remote_url);
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
                    println!("✔ fetched {:?}", res.content_length());
                    Ok(res.bytes().await.unwrap().to_vec())
                }
            },
            Err(e) => {
                eprintln!("failed to fetch upstream: {}", e);
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
            _ => "".to_string(),
        }
    }

    pub fn relative_path(&self) -> String {
        match &self {
            Task::PypiIndexTask { pkg_name, .. } => pkg_name.clone(),
            Task::PypiPackagesTask { pkg_path } => pkg_path.clone(),
            Task::AnacondaTask { path } => path.clone(),
            _ => "".to_string(),
        }
    }
}

pub struct TaskManager {
    task_list: HashSet<Task>,
    config: Settings,
    pub pypi_index_cache: Arc<dyn CachePolicy>,
    pub pypi_pkg_cache: Arc<dyn CachePolicy>,
    pub anaconda_cache: Arc<dyn CachePolicy>,
    _cache_map: HashMap<String, Arc<dyn CachePolicy>>,
}

impl TaskManager {
    pub fn new(config: Settings) -> Self {
        TaskManager {
            task_list: HashSet::new(),
            config,
            pypi_index_cache: Arc::new(NoCache {}),
            pypi_pkg_cache: Arc::new(NoCache {}),
            anaconda_cache: Arc::new(NoCache {}),
            _cache_map: HashMap::new(),
        }
    }

    // add a task into task list
    async fn add_task(&self, task: Task) {
        if self.task_list.contains(&task) {
            println!("ingore task: {:?}", task);
            return;
        }
        println!("added task: {:?}", task);
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
            _ => c = Arc::new(NoCache {}),
        };
        let task_clone = task.clone();
        let upstream_url = self.resolve_task_upstream(&task_clone, &task_clone.relative_path());
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
                    println!("task completed: {:?}", task);
                }
                Err(e) => {
                    eprintln!("failed to fetch upstream: {}", e);
                }
            };
        });
    }

    pub fn get(&self, task_type: &Task, key: &str) -> Option<cache::BytesArray> {
        match &task_type {
            Task::PypiIndexTask { .. } => self.pypi_index_cache.get(key),
            Task::PypiPackagesTask { .. } => self.pypi_pkg_cache.get(key),
            Task::AnacondaTask { .. } => self.anaconda_cache.get(key),
            _ => None,
        }
    }

    pub fn resolve_task_upstream(&self, task_type: &Task, link: &str) -> String {
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
            _ => link.to_string(),
        }
    }
}
