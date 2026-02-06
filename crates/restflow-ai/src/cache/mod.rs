mod file_cache;
mod permission_cache;
mod search_cache;

pub use file_cache::{CacheStats, FileContentCache};
pub use permission_cache::PermissionCache;
pub use search_cache::{CachedSearchResult, SearchCache, SearchMatch};

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
            files: Arc::new(FileContentCache::new(100, 1_000_000)),
            permissions: Arc::new(PermissionCache::new(Duration::from_secs(3600))),
            search: Arc::new(SearchCache::new(Duration::from_secs(30), 50)),
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
