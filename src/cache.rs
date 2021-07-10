use crate::util;

/**
 * Cache holds static config for the cache.
 */
#[derive(Clone, PartialEq, Hash)]
pub struct Cache {
	root_dir: String,
	size_limit: u64,
}

#[allow(dead_code)]
impl Cache {
	pub fn cache_path(self, path: &str) -> String {
		format!("{}/{}", self.root_dir, path)
	}
}

#[derive(Hash, Eq, PartialEq, Debug)]
pub struct CacheEntry {
	pub valid: bool,
	pub path: String,
	pub atime: i64, // last access timestamp
}

impl CacheEntry {
	pub fn new(path: &str) -> CacheEntry {
		CacheEntry {
			path: path.to_string(),
			valid: true,
			atime: util::now(),
		}
	}

	pub fn to_redis_multiple_fields(&self) -> Vec<(&str, String)> {
		vec![
			("valid", String::from(if self.valid { "1" } else { "0" })),
			("path", self.path.clone()),
			("atime", self.atime.to_string()),
		]
	}
}
