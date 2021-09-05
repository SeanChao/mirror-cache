use crate::error::Error;
use crate::error::Result;
use config::{Config, Environment, File};

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub port: u16,
    redis: Redis,
    pub url: Option<String>,
    pub log_level: String,
    pub rules: Vec<Rule>,
    pub policies: Vec<Policy>,
}

#[derive(Debug, Deserialize, Clone)]
struct Redis {
    url: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Rule {
    pub path: String,
    pub policy: String,
    pub upstream: String,
    pub size_limit: Option<String>,
    pub rewrite: Option<Vec<Rewrite>>,
    pub options: Option<Options>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Policy {
    pub name: String,
    #[serde(rename = "type")]
    pub typ: PolicyType,
    pub timeout: Option<u64>,
    pub size: Option<String>,
    pub path: Option<String>, // cache path
}

#[derive(Debug, Deserialize, Clone)]
pub struct Rewrite {
    pub from: String,
    pub to: String,
}

/// Options for rules
#[derive(Debug, Deserialize, Clone)]
pub struct Options {
    /// Override the content-type in the HTTP response header
    pub content_type: Option<String>,
}

#[derive(Debug, Deserialize, Copy, Clone)]
pub enum PolicyType {
    #[serde(rename = "LRU")]
    Lru,
    #[serde(rename = "TTL")]
    Ttl,
}

impl Settings {
    pub fn new() -> Result<Self> {
        Self::new_from("config", "app")
    }

    pub fn new_from(filename: &str, env_prefix: &str) -> Result<Self> {
        let mut s = Config::default();
        s.merge(File::with_name(filename))?;
        s.merge(Environment::with_prefix(env_prefix))?;
        match s.try_into() {
            Ok(settings) => Ok(settings),
            Err(e) => Err(Error::ConfigDeserializeError(e)),
        }
    }

    pub fn get_redis_url(&self) -> String {
        self.redis.url.clone()
    }

    /// parse log level string to log::LevelFilter enum.
    /// The default log level is `info`.
    pub fn get_log_level(&self) -> log::LevelFilter {
        match self.log_level.to_lowercase().as_str() {
            "error" => log::LevelFilter::Error,
            "warn" => log::LevelFilter::Warn,
            "info" => log::LevelFilter::Info,
            "debug" => log::LevelFilter::Debug,
            "trace" => log::LevelFilter::Trace,
            _ => log::LevelFilter::Info,
        }
    }
}
