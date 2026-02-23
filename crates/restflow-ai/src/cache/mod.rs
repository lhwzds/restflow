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

// ── AgentCache trait implementation ──────────────────────────────────

use restflow_traits::cache::{
    AgentCache, CachedSearchResult as TraitCachedSearchResult,
    SearchMatch as TraitSearchMatch,
};

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
    ) -> Option<TraitCachedSearchResult> {
        self.search.get(pattern, dir, file_pattern).await.map(|r| {
            TraitCachedSearchResult {
                matches: r
                    .matches
                    .into_iter()
                    .map(|m| TraitSearchMatch {
                        file: m.file,
                        line: m.line,
                        content: m.content,
                    })
                    .collect(),
                total_files_searched: r.total_files_searched,
                truncated: r.truncated,
            }
        })
    }

    async fn put_search(
        &self,
        pattern: &str,
        dir: &str,
        file_pattern: Option<&str>,
        result: TraitCachedSearchResult,
    ) {
        let internal = CachedSearchResult {
            matches: result
                .matches
                .into_iter()
                .map(|m| SearchMatch {
                    file: m.file,
                    line: m.line,
                    content: m.content,
                })
                .collect(),
            total_files_searched: result.total_files_searched,
            truncated: result.truncated,
        };
        self.search.put(pattern, dir, file_pattern, internal).await;
    }

    async fn invalidate_search_dir(&self, dir: &str) {
        self.search.invalidate_directory(dir).await;
    }
}
