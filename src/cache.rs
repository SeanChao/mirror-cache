#[allow(dead_code)]
pub struct CacheConfig {
	root_dir: String,
}

#[allow(dead_code)]
impl CacheConfig {
	pub fn cache_path(self, path: &str) -> String {
		format!("{}/{}", self.root_dir, path)
	}
}

#[derive(Hash, Eq, PartialEq, Debug)]
pub struct CacheEntry {
	pub valid: bool,
	pub path: String,
}

impl CacheEntry {
	pub fn new(path: &str) -> CacheEntry {
		CacheEntry {
			path: path.to_string(),
			valid: true,
		}
	}

	pub fn to_redis_multiple_fields(&self) -> Vec<(&str, &str)> {
		let mut vec: Vec<(&str, &str)> = Vec::new();
		vec.push(("valid", if self.valid { "1" } else { "0" }));
		vec.push(("path", &self.path));
		vec
	}
}
