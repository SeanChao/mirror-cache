use crate::util;

/**
 * Cache holds static config for the cache.
 */
#[derive(Clone)]
pub struct Cache {
	root_dir: String,
	pub size_limit: u64, // cache size in bytes(B)
	// redis_client: &redis::Client,
}

#[allow(dead_code)]
impl Cache {
	pub fn new(root_dir: &str, size_limit: u64) -> Self {
		Self {
			root_dir: String::from(root_dir),
			size_limit: size_limit,
		}
	}

	pub fn cache_path(&self, path: &str) -> String {
		format!("{}/{}", self.root_dir, path)
	}
}

#[derive(Hash, Eq, PartialEq, Debug)]
pub struct CacheEntry {
	pub valid: bool,
	pub path: String,
	pub size: u64,
	pub atime: i64, // last access timestamp
}

impl CacheEntry {
	pub fn new(path: &str, size: u64) -> CacheEntry {
		CacheEntry {
			path: path.to_string(),
			valid: true,
			size: size,
			atime: util::now(),
		}
	}

	/**
	 * Convert a cache entry to an array keys and values to be stored as redis hash
	 */
	pub fn to_redis_multiple_fields(&self) -> Vec<(&str, String)> {
		vec![
			("valid", String::from(if self.valid { "1" } else { "0" })),
			("path", self.path.clone()),
			("size", self.size.to_string()),
			("atime", self.atime.to_string()),
		]
	}
}
