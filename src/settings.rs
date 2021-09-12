use crate::error::Error;
use crate::error::Result;
use config::{Config, Environment, File};

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub port: u16,
    pub metrics_port: u16,
    redis: Redis,
    pub sled: Sled,
    pub log_level: String,
    pub rules: Vec<Rule>,
    pub policies: Vec<Policy>,
}

#[derive(Debug, Deserialize, Clone)]
struct Redis {
    url: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Sled {
    pub metadata_path: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Rule {
    pub name: Option<String>,
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
    pub metadata_db: MetadataDb,
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

#[derive(Debug, Deserialize, Copy, Clone)]
pub enum MetadataDb {
    #[serde(rename = "sled")]
    Sled,
    #[serde(rename = "redis")]
    Redis,
}

impl Settings {
    pub fn default() -> Self {
        Settings {
            port: 9000,
            metrics_port: 9001,
            redis: Redis {
                url: "redis://localhost".to_string(),
            },
            sled: Sled {
                metadata_path: "sled/metadata".to_string(),
            },
            log_level: "info".to_string(),
            rules: vec![],
            policies: vec![],
        }
    }

    pub fn new(filename: &str) -> Result<Self> {
        Self::new_from(filename, "app")
    }

    pub fn new_from(filename: &str, env_prefix: &str) -> Result<Self> {
        let mut s = Config::default();
        s.merge(File::with_name(filename))?;
        s.merge(Environment::with_prefix(env_prefix))?;
        match s.try_into() {
            Ok(settings) => {
                let mut settings: Settings = settings;
                // name all unnamed rules
                for (idx, rule) in settings.rules.iter_mut().enumerate() {
                    if rule.name.is_none() {
                        rule.name = Some(format!("rule_{}", idx));
                    }
                }
                Ok(settings)
            }
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

pub fn rule_label(rule: &Rule) -> String {
    rule.name.clone().unwrap_or("unnamed_rules".to_string())
}

#[cfg(test)]
mod test {
    use super::*;

    macro_rules! new_rule {
        ($name: expr) => {
            Rule {
                name: $name,
                path: "".into(),
                policy: "".into(),
                upstream: "".into(),
                size_limit: None,
                rewrite: None,
                options: None,
            }
        };
    }

    #[test]
    fn get_rule_label_test() {
        let rule = new_rule!(Some("awesome".into()));
        assert_eq!(rule_label(&rule), "awesome");
        let rule = new_rule!(None);
        assert_eq!(rule_label(&rule), "unnamed_rules");
    }
}
