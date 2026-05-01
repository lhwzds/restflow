use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

pub use restflow_traits::cache::{CachedSearchResult, SearchMatch};

/// Search cache key
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct SearchKey {
    pattern: String,
    directory: String,
    file_pattern: Option<String>,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    result: CachedSearchResult,
    created_at: Instant,
}

/// TTL-based search result cache
#[derive(Debug)]
pub struct SearchCache {
    cache: RwLock<HashMap<SearchKey, CacheEntry>>,
    ttl: Duration,
    max_entries: usize,
}

impl SearchCache {
    pub fn new(ttl: Duration, max_entries: usize) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            ttl,
            max_entries,
        }
    }

    /// Get cached search results
    pub async fn get(
        &self,
        pattern: &str,
        directory: &str,
        file_pattern: Option<&str>,
    ) -> Option<CachedSearchResult> {
        let key = SearchKey {
            pattern: pattern.to_string(),
            directory: directory.to_string(),
            file_pattern: file_pattern.map(String::from),
        };

        let cache = self.cache.read().await;
        cache.get(&key).and_then(|entry| {
            if entry.created_at.elapsed() < self.ttl {
                Some(entry.result.clone())
            } else {
                None
            }
        })
    }

    /// Store search results
    pub async fn put(
        &self,
        pattern: &str,
        directory: &str,
        file_pattern: Option<&str>,
        result: CachedSearchResult,
    ) {
        let key = SearchKey {
            pattern: pattern.to_string(),
            directory: directory.to_string(),
            file_pattern: file_pattern.map(String::from),
        };

        let mut cache = self.cache.write().await;
        if cache.len() >= self.max_entries {
            let now = Instant::now();
            cache.retain(|_, value| now.duration_since(value.created_at) < self.ttl);
        }

        cache.insert(
            key,
            CacheEntry {
                result,
                created_at: Instant::now(),
            },
        );
    }

    /// Invalidate cache for a directory (when files change)
    pub async fn invalidate_directory(&self, directory: &str) {
        let mut cache = self.cache.write().await;
        cache.retain(|key, _| !key.directory.starts_with(directory));
    }
}
