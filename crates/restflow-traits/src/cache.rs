//! Caching abstractions for agent tools.
//!
//! Defines data types and the [`AgentCache`] trait for file content and
//! search result caching, decoupling tool implementations from the concrete
//! cache manager in `restflow-ai`.

/// Cached search results.
#[derive(Debug, Clone)]
pub struct CachedSearchResult {
    pub matches: Vec<SearchMatch>,
    pub total_files_searched: usize,
    pub truncated: bool,
}

/// A single search match entry.
#[derive(Debug, Clone)]
pub struct SearchMatch {
    pub file: String,
    pub line: usize,
    pub content: String,
}

/// Unified file content and search result cache.
///
/// Abstracts `AgentCacheManager` (file cache + search cache) so that tool
/// implementations can use caching without depending on `restflow-ai`.
#[async_trait::async_trait]
pub trait AgentCache: Send + Sync {
    // ── File content cache ───────────────────────────────────────────

    /// Get cached file content if it matches the current metadata.
    async fn get_file(
        &self,
        path: &std::path::Path,
        metadata: &std::fs::Metadata,
    ) -> Option<String>;

    /// Store file content in the cache.
    async fn put_file(&self, path: &std::path::Path, content: String, metadata: &std::fs::Metadata);

    /// Invalidate cached content for a specific file.
    async fn invalidate_file(&self, path: &std::path::Path);

    // ── Search result cache ──────────────────────────────────────────

    /// Get cached search results.
    async fn get_search(
        &self,
        pattern: &str,
        dir: &str,
        file_pattern: Option<&str>,
    ) -> Option<CachedSearchResult>;

    /// Store search results in the cache.
    async fn put_search(
        &self,
        pattern: &str,
        dir: &str,
        file_pattern: Option<&str>,
        result: CachedSearchResult,
    );

    /// Invalidate all cached searches for a directory prefix.
    async fn invalidate_search_dir(&self, dir: &str);
}
