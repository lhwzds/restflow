mod file_cache;
mod permission_cache;
mod search_cache;

pub use file_cache::{CacheStats, FileContentCache};
pub use permission_cache::PermissionCache;
pub use search_cache::{CachedSearchResult, SearchCache, SearchMatch};

use restflow_traits::{
    DEFAULT_AGENT_CACHE_FILE_MAX_BYTES, DEFAULT_AGENT_CACHE_FILE_MAX_ENTRIES,
    DEFAULT_AGENT_CACHE_PERMISSION_TTL_SECS, DEFAULT_AGENT_CACHE_SEARCH_MAX_ENTRIES,
    DEFAULT_AGENT_CACHE_SEARCH_TTL_SECS,
};
use std::sync::Arc;
use std::time::Duration;

/// Unified cache manager for agent session
#[derive(Clone, Debug)]
pub struct AgentCacheManager {
    pub files: Arc<FileContentCache>,
    pub permissions: Arc<PermissionCache>,
    pub search: Arc<SearchCache>,
}

impl AgentCacheManager {
    pub fn new() -> Self {
        Self {
            files: Arc::new(FileContentCache::new(
                DEFAULT_AGENT_CACHE_FILE_MAX_ENTRIES,
                DEFAULT_AGENT_CACHE_FILE_MAX_BYTES,
            )),
            permissions: Arc::new(PermissionCache::new(Duration::from_secs(
                DEFAULT_AGENT_CACHE_PERMISSION_TTL_SECS,
            ))),
            search: Arc::new(SearchCache::new(
                Duration::from_secs(DEFAULT_AGENT_CACHE_SEARCH_TTL_SECS),
                DEFAULT_AGENT_CACHE_SEARCH_MAX_ENTRIES,
            )),
        }
    }

    /// Clear all caches for session end
    pub async fn clear_all(&self) {
        self.files.clear().await;
        self.search.invalidate_directory("").await;
    }
}

impl Default for AgentCacheManager {
    fn default() -> Self {
        Self::new()
    }
}

// ── AgentCache trait implementation ──────────────────────────────────

use restflow_traits::cache::AgentCache;

#[async_trait::async_trait]
impl AgentCache for AgentCacheManager {
    async fn get_file(
        &self,
        path: &std::path::Path,
        metadata: &std::fs::Metadata,
    ) -> Option<String> {
        self.files.get_with_metadata(path, metadata).await
    }

    async fn put_file(
        &self,
        path: &std::path::Path,
        content: String,
        metadata: &std::fs::Metadata,
    ) {
        self.files.put(path, content, metadata).await;
    }

    async fn invalidate_file(&self, path: &std::path::Path) {
        self.files.invalidate(path).await;
    }

    async fn get_search(
        &self,
        pattern: &str,
        dir: &str,
        file_pattern: Option<&str>,
    ) -> Option<CachedSearchResult> {
        self.search.get(pattern, dir, file_pattern).await
    }

    async fn put_search(
        &self,
        pattern: &str,
        dir: &str,
        file_pattern: Option<&str>,
        result: CachedSearchResult,
    ) {
        self.search.put(pattern, dir, file_pattern, result).await;
    }

    async fn invalidate_search_dir(&self, dir: &str) {
        self.search.invalidate_directory(dir).await;
    }
}
