use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tokio::sync::RwLock;

/// Cached file entry with modification tracking
#[derive(Debug, Clone)]
struct CachedFile {
    content: String,
    mtime: SystemTime,
    size: u64,
    line_count: usize,
}

/// Session-scoped file content cache
#[derive(Debug)]
pub struct FileContentCache {
    cache: RwLock<HashMap<PathBuf, CachedFile>>,
    max_entries: usize,
    max_file_size: usize,
}

impl FileContentCache {
    pub fn new(max_entries: usize, max_file_size: usize) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            max_entries,
            max_file_size,
        }
    }

    /// Get file content if cached and still valid
    pub async fn get(&self, path: &Path) -> Option<String> {
        let meta = tokio::fs::metadata(path).await.ok()?;
        self.get_with_metadata(path, &meta).await
    }

    /// Get file content using existing metadata
    pub async fn get_with_metadata(
        &self,
        path: &Path,
        meta: &std::fs::Metadata,
    ) -> Option<String> {
        let cache = self.cache.read().await;
        let cached = cache.get(path)?;
        let mtime = meta.modified().ok()?;
        if mtime == cached.mtime && meta.len() == cached.size {
            Some(cached.content.clone())
        } else {
            None
        }
    }

    /// Store file content in cache
    pub async fn put(&self, path: &Path, content: String, meta: &std::fs::Metadata) {
        if content.len() > self.max_file_size {
            return;
        }

        let mut cache = self.cache.write().await;
        if cache.len() >= self.max_entries {
            if let Some(key) = cache.keys().next().cloned() {
                cache.remove(&key);
            }
        }

        let entry = CachedFile {
            content,
            mtime: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            size: meta.len(),
            line_count: 0,
        };

        cache.insert(path.to_path_buf(), entry);
    }

    /// Invalidate specific file
    pub async fn invalidate(&self, path: &Path) {
        let mut cache = self.cache.write().await;
        cache.remove(path);
    }

    /// Invalidate all files in a directory
    pub async fn invalidate_dir(&self, dir: &Path) {
        let mut cache = self.cache.write().await;
        cache.retain(|path, _| !path.starts_with(dir));
    }

    /// Clear entire cache
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        let cache = self.cache.read().await;
        CacheStats {
            entries: cache.len(),
            total_bytes: cache.values().map(|entry| entry.content.len()).sum(),
        }
    }
}

#[derive(Debug)]
pub struct CacheStats {
    pub entries: usize,
    pub total_bytes: usize,
}
