//! File operations tool for AI agents
//!
//! Provides file system operations with:
//! - Read files with line numbers and pagination
//! - Write/append files with auto-creation of parent directories
//! - List directory contents with glob pattern matching
//! - Search files with regex
//! - Delete files
//! - Check file existence
//! - Optional base directory restriction for security
//!
//! # Example
//!
//! ```ignore
//! let tool = FileTool::new();
//! let output = tool.execute(serde_json::json!({
//!     "action": "read",
//!     "path": "/tmp/test.txt"
//! })).await?;
//! ```

mod types;

#[cfg(test)]
mod tests;

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncWriteExt;

use super::file_tracker::FileTracker;
use super::shared::is_likely_binary;
use crate::Result;
use crate::ToolAction;
use crate::check_security;
use crate::security::SecurityGate;
use crate::{Tool, ToolErrorCategory, ToolOutput};
use restflow_traits::cache::{AgentCache, CachedSearchResult, SearchMatch as CachedSearchMatch};
use restflow_traits::store::DiagnosticsProvider;
use types::{
    default_batch_line_limit, default_batch_max_matches, default_batch_max_size,
    default_context_lines, default_continue_on_error, file_parameters_schema,
};

pub use types::FileAction;

/// Maximum file size to read (1MB)
const DEFAULT_MAX_READ_BYTES: usize = 1_000_000;

/// Default number of lines to read
const DEFAULT_LINE_LIMIT: usize = 2000;

/// Maximum entries to return in directory listing
const MAX_LIST_ENTRIES: usize = 1000;

/// Maximum search matches to return
const MAX_SEARCH_MATCHES: usize = 100;

/// Maximum files allowed in a batch read
const MAX_BATCH_READ_FILES: usize = 20;

/// Maximum paths allowed in a batch exists check
const MAX_BATCH_EXISTS_PATHS: usize = 50;

/// Maximum locations allowed in a batch search
const MAX_BATCH_SEARCH_LOCATIONS: usize = 10;

/// Maximum parallel workers for file batch operations
const BATCH_IO_CONCURRENCY: usize = 8;

/// Default max lines per file in batch read
const DEFAULT_BATCH_LINE_LIMIT: usize = 500;

/// Default max file size per file in batch read
const DEFAULT_BATCH_MAX_FILE_SIZE: usize = 500_000;

/// Default max matches for batch search
const DEFAULT_BATCH_MAX_MATCHES: usize = 100;

/// Default context lines for batch search
const DEFAULT_BATCH_CONTEXT_LINES: usize = 2;

/// File operations tool
#[derive(Clone)]
pub struct FileTool {
    /// Base directory for file operations (security boundary)
    base_dir: Option<PathBuf>,
    /// Whether file operations require an explicit base directory.
    require_base_dir: bool,
    /// Maximum file size to read in bytes
    max_read_bytes: usize,
    /// Track file reads/writes for external modification detection
    tracker: Arc<FileTracker>,
    /// Optional diagnostics provider
    diagnostics: Option<Arc<dyn DiagnosticsProvider>>,
    /// Optional cache manager for file/search operations
    cache_manager: Option<Arc<dyn AgentCache>>,
    /// Optional security gate
    security_gate: Option<Arc<dyn SecurityGate>>,
    /// Agent identifier for security checks
    agent_id: Option<String>,
    /// Task identifier for security checks
    task_id: Option<String>,
}

impl Default for FileTool {
    fn default() -> Self {
        Self::new()
    }
}

impl FileTool {
    /// Create a new FileTool with default settings
    pub fn new() -> Self {
        Self::with_tracker(Arc::new(FileTracker::new()))
    }

    pub fn with_tracker(tracker: Arc<FileTracker>) -> Self {
        Self {
            base_dir: None,
            require_base_dir: false,
            max_read_bytes: DEFAULT_MAX_READ_BYTES,
            tracker,
            diagnostics: None,
            cache_manager: None,
            security_gate: None,
            agent_id: None,
            task_id: None,
        }
    }

    /// Set base directory for file operations (security boundary)
    /// All paths will be resolved relative to this directory
    pub fn with_base_dir(mut self, base: impl Into<PathBuf>) -> Self {
        self.base_dir = Some(base.into());
        self
    }

    pub fn require_base_dir(mut self) -> Self {
        self.require_base_dir = true;
        self
    }

    /// Set maximum read size in bytes
    pub fn with_max_read(mut self, bytes: usize) -> Self {
        self.max_read_bytes = bytes;
        self
    }

    /// Attach a diagnostics provider.
    pub fn with_diagnostics_provider(mut self, provider: Arc<dyn DiagnosticsProvider>) -> Self {
        self.diagnostics = Some(provider);
        self
    }

    /// Attach a cache manager for file and search operations
    pub fn with_cache_manager(mut self, cache_manager: Arc<dyn AgentCache>) -> Self {
        self.cache_manager = Some(cache_manager);
        self
    }
    pub fn with_security(
        mut self,
        security_gate: Arc<dyn SecurityGate>,
        agent_id: impl Into<String>,
        task_id: impl Into<String>,
    ) -> Self {
        self.security_gate = Some(security_gate);
        self.agent_id = Some(agent_id.into());
        self.task_id = Some(task_id.into());
        self
    }

    /// Resolve and validate a path against the base directory
    fn resolve_path(&self, path: &str) -> std::result::Result<PathBuf, String> {
        super::path_utils::resolve_path_with_policy(
            path,
            self.base_dir.as_deref(),
            self.require_base_dir,
        )
    }

    /// Read file with line numbers
    async fn read_file(&self, path: &str, offset: usize, limit: Option<usize>) -> ToolOutput {
        let path = match self.resolve_path(path) {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };

        // Single syscall: get metadata without following symlinks
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return ToolOutput::error(format!("File not found: {}", path.display()));
            }
            Err(e) => return ToolOutput::error(format!("Cannot read metadata: {}", e)),
        };

        if metadata.file_type().is_symlink() {
            return ToolOutput::error(format!("Symlinks are not allowed: {}", path.display()));
        }

        if !metadata.file_type().is_file() {
            return ToolOutput::error(format!("Not a file: {}", path.display()));
        }

        if metadata.len() as usize > self.max_read_bytes {
            return ToolOutput::error(format!(
                "File too large ({} bytes). Maximum: {} bytes. Use offset/limit for partial reads.",
                metadata.len(),
                self.max_read_bytes
            ));
        }

        if let Some(cache) = &self.cache_manager
            && let Some(content) = cache.get_file(&path, &metadata).await
        {
            return Self::format_file_output(&path, &content, offset, limit);
        }

        let content = match fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => return ToolOutput::error(format!("Cannot read file: {}", e)),
        };

        self.tracker.record_read(&path);
        if let Some(cache) = &self.cache_manager {
            cache.put_file(&path, content.clone(), &metadata).await;
        }

        Self::format_file_output(&path, &content, offset, limit)
    }

    fn format_file_output(
        path: &Path,
        content: &str,
        offset: usize,
        limit: Option<usize>,
    ) -> ToolOutput {
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        let line_limit = limit.unwrap_or(DEFAULT_LINE_LIMIT);
        let start = offset.min(total_lines);
        let end = (start + line_limit).min(total_lines);

        let selected: Vec<String> = lines[start..end]
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{:4} | {}", start + i + 1, line))
            .collect();

        ToolOutput::success(serde_json::json!({
            "path": path.display().to_string(),
            "total_lines": total_lines,
            "showing": format!("{}-{}", start + 1, end),
            "content": selected.join("\n"),
        }))
    }

    fn format_search_output(
        search_path: &str,
        pattern: &str,
        result: CachedSearchResult,
    ) -> ToolOutput {
        let matches: Vec<Value> = result
            .matches
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "file": entry.file.clone(),
                    "line": entry.line,
                    "content": entry.content.clone(),
                })
            })
            .collect();

        ToolOutput::success(serde_json::json!({
            "pattern": pattern,
            "search_path": search_path,
            "match_count": matches.len(),
            "truncated": result.truncated,
            "total_files_searched": result.total_files_searched,
            "matches": matches,
        }))
    }

    /// Write or append to a file
    async fn write_file(&self, path: &str, content: &str, append: bool) -> ToolOutput {
        let path = match self.resolve_path(path) {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };

        if std::fs::symlink_metadata(&path).is_ok() && !self.tracker.has_been_read(&path) {
            return ToolOutput::error(format!(
                "You must read {} before writing to it. Read the file first to understand its current content.",
                path.display()
            ));
        }

        match self.tracker.check_external_modification(&path).await {
            Ok(true) => {
                return ToolOutput::error(format!(
                    "File {} has been modified externally since it was read. Read it again before writing.",
                    path.display()
                ));
            }
            Ok(false) => {}
            Err(e) => {
                return ToolOutput::error(format!("Cannot check file modification time: {}", e));
            }
        }

        // Create parent directories if needed
        if let Some(parent) = path.parent()
            && std::fs::symlink_metadata(parent).is_err()
            && let Err(e) = fs::create_dir_all(parent).await
        {
            return ToolOutput::error(format!("Cannot create directory: {}", e));
        }

        let result = if append {
            let mut file = match fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .await
            {
                Ok(f) => f,
                Err(e) => return ToolOutput::error(format!("Cannot open file: {}", e)),
            };
            file.write_all(content.as_bytes()).await
        } else {
            fs::write(&path, content).await
        };

        match result {
            Ok(()) => {
                self.tracker.record_write(&path);

                if let Some(cache) = &self.cache_manager {
                    cache.invalidate_file(&path).await;
                    let mut current = path.parent();
                    while let Some(directory) = current {
                        cache
                            .invalidate_search_dir(&directory.to_string_lossy())
                            .await;
                        current = directory.parent();
                    }
                }

                if let Some(provider) = self.diagnostics.clone() {
                    let path = path.clone();
                    tokio::spawn(async move {
                        let _ = provider.ensure_open(&path).await;
                        if let Ok(latest_content) = fs::read_to_string(&path).await {
                            let _ = provider.did_change(&path, &latest_content).await;
                        }
                    });
                }

                ToolOutput::success(serde_json::json!({
                    "path": path.display().to_string(),
                    "bytes_written": content.len(),
                    "action": if append { "appended" } else { "written" },
                }))
            }
            Err(e) => ToolOutput::error(format!("Cannot write file: {}", e)),
        }
    }

    /// List directory contents
    async fn list_dir(&self, path: &str, recursive: bool, pattern: Option<&str>) -> ToolOutput {
        let path = match self.resolve_path(path) {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };

        if !path.exists() {
            return ToolOutput::error(format!("Directory not found: {}", path.display()));
        }

        if !path.is_dir() {
            return ToolOutput::error(format!("Not a directory: {}", path.display()));
        }

        let mut entries: Vec<Value> = Vec::new();

        if recursive {
            self.list_recursive(&path, &mut entries, pattern, &path)
                .await;
        } else {
            let mut read_dir = match fs::read_dir(&path).await {
                Ok(rd) => rd,
                Err(e) => return ToolOutput::error(format!("Cannot read directory: {}", e)),
            };

            while let Ok(Some(entry)) = read_dir.next_entry().await {
                if entries.len() >= MAX_LIST_ENTRIES {
                    break;
                }

                let name = entry.file_name().to_string_lossy().to_string();
                if let Some(p) = pattern
                    && !glob_match(p, &name)
                {
                    continue;
                }

                let file_type = match entry.file_type().await {
                    Ok(ft) => {
                        if ft.is_dir() {
                            "dir"
                        } else if ft.is_symlink() {
                            "symlink"
                        } else {
                            "file"
                        }
                    }
                    Err(_) => "unknown",
                };

                let size = match entry.metadata().await {
                    Ok(m) => Some(m.len()),
                    Err(_) => None,
                };

                entries.push(serde_json::json!({
                    "name": name,
                    "type": file_type,
                    "size": size,
                }));
            }
        }

        let truncated = entries.len() >= MAX_LIST_ENTRIES;

        ToolOutput::success(serde_json::json!({
            "path": path.display().to_string(),
            "count": entries.len(),
            "truncated": truncated,
            "entries": entries,
        }))
    }

    /// Recursively list directory contents
    #[allow(clippy::only_used_in_recursion)]
    fn list_recursive<'a>(
        &'a self,
        dir: &'a Path,
        entries: &'a mut Vec<Value>,
        pattern: Option<&'a str>,
        base: &'a Path,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            if entries.len() >= MAX_LIST_ENTRIES {
                return;
            }

            let mut read_dir = match fs::read_dir(dir).await {
                Ok(rd) => rd,
                Err(_) => return,
            };

            while let Ok(Some(entry)) = read_dir.next_entry().await {
                if entries.len() >= MAX_LIST_ENTRIES {
                    break;
                }

                let name = entry.file_name().to_string_lossy().to_string();
                let entry_path = entry.path();
                let relative_path = entry_path
                    .strip_prefix(base)
                    .unwrap_or(&entry_path)
                    .to_string_lossy()
                    .to_string();

                let file_type = match entry.file_type().await {
                    Ok(ft) => {
                        if ft.is_dir() {
                            "dir"
                        } else if ft.is_symlink() {
                            "symlink"
                        } else {
                            "file"
                        }
                    }
                    Err(_) => "unknown",
                };

                // Apply pattern filter
                if let Some(p) = pattern
                    && !glob_match(p, &name)
                    && !glob_match(p, &relative_path)
                {
                    // Still recurse into directories even if they don't match
                    if file_type == "dir" {
                        self.list_recursive(&entry_path, entries, pattern, base)
                            .await;
                    }
                    continue;
                }

                let size = match entry.metadata().await {
                    Ok(m) => Some(m.len()),
                    Err(_) => None,
                };

                entries.push(serde_json::json!({
                    "path": relative_path,
                    "name": name,
                    "type": file_type,
                    "size": size,
                }));

                // Recurse into directories
                if file_type == "dir" {
                    self.list_recursive(&entry_path, entries, pattern, base)
                        .await;
                }
            }
        })
    }

    /// Search for text in files
    async fn search_files(
        &self,
        path: &str,
        pattern: &str,
        file_pattern: Option<&str>,
    ) -> ToolOutput {
        let path = match self.resolve_path(path) {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };

        let search_path = path.display().to_string();
        if let Some(cache) = &self.cache_manager
            && let Some(cached) = cache.get_search(pattern, &search_path, file_pattern).await
        {
            return Self::format_search_output(&search_path, pattern, cached);
        }

        let regex = match Regex::new(pattern) {
            Ok(r) => r,
            Err(e) => return ToolOutput::error(format!("Invalid regex pattern: {}", e)),
        };

        let mut matches: Vec<CachedSearchMatch> = Vec::new();
        let mut truncated = false;
        let mut total_files_searched = 0;
        self.search_recursive(
            &path,
            &regex,
            file_pattern,
            &mut matches,
            &mut truncated,
            &mut total_files_searched,
            &path,
        )
        .await;

        let result = CachedSearchResult {
            matches,
            total_files_searched,
            truncated,
        };

        if let Some(cache) = &self.cache_manager {
            cache
                .put_search(pattern, &search_path, file_pattern, result.clone())
                .await;
        }

        Self::format_search_output(&search_path, pattern, result)
    }

    /// Recursively search for text in files
    #[allow(clippy::too_many_arguments)]
    fn search_recursive<'a>(
        &'a self,
        dir: &'a Path,
        regex: &'a Regex,
        file_pattern: Option<&'a str>,
        matches: &'a mut Vec<CachedSearchMatch>,
        truncated: &'a mut bool,
        total_files_searched: &'a mut usize,
        base: &'a Path,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            if matches.len() >= MAX_SEARCH_MATCHES {
                *truncated = true;
                return;
            }

            if dir.is_file() {
                // Search in single file
                let name = dir
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if let Some(p) = file_pattern
                    && !glob_match(p, &name)
                {
                    return;
                }
                self.search_in_file(dir, regex, matches, truncated, total_files_searched, base)
                    .await;
                return;
            }

            let mut read_dir = match fs::read_dir(dir).await {
                Ok(rd) => rd,
                Err(_) => return,
            };

            while let Ok(Some(entry)) = read_dir.next_entry().await {
                if matches.len() >= MAX_SEARCH_MATCHES {
                    *truncated = true;
                    break;
                }

                let entry_path = entry.path();
                let file_type = match entry.file_type().await {
                    Ok(ft) => ft,
                    Err(_) => continue,
                };

                if file_type.is_dir() {
                    // Skip hidden directories
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with('.') {
                        continue;
                    }
                    self.search_recursive(
                        &entry_path,
                        regex,
                        file_pattern,
                        matches,
                        truncated,
                        total_files_searched,
                        base,
                    )
                    .await;
                } else if file_type.is_file() {
                    let name = entry.file_name().to_string_lossy().to_string();

                    // Skip binary-looking files
                    if is_likely_binary(&name) {
                        continue;
                    }

                    // Apply file pattern filter
                    if let Some(p) = file_pattern
                        && !glob_match(p, &name)
                    {
                        continue;
                    }

                    self.search_in_file(
                        &entry_path,
                        regex,
                        matches,
                        truncated,
                        total_files_searched,
                        base,
                    )
                    .await;
                }
            }
        })
    }

    /// Search for pattern in a single file
    async fn search_in_file(
        &self,
        file: &Path,
        regex: &Regex,
        matches: &mut Vec<CachedSearchMatch>,
        truncated: &mut bool,
        total_files_searched: &mut usize,
        base: &Path,
    ) {
        let content = match fs::read_to_string(file).await {
            Ok(c) => c,
            Err(_) => return, // Skip files that can't be read as text
        };

        *total_files_searched += 1;

        let relative_path = file
            .strip_prefix(base)
            .unwrap_or(file)
            .to_string_lossy()
            .to_string();

        for (line_num, line) in content.lines().enumerate() {
            if matches.len() >= MAX_SEARCH_MATCHES {
                *truncated = true;
                break;
            }

            if regex.is_match(line) {
                matches.push(CachedSearchMatch {
                    file: relative_path.clone(),
                    line: line_num + 1,
                    content: line.chars().take(200).collect::<String>(),
                });
            }
        }
    }

    /// Delete a file
    async fn delete_file(&self, path: &str) -> ToolOutput {
        let path = match self.resolve_path(path) {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };

        // Single syscall: get metadata without following symlinks
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return ToolOutput::error(format!("File not found: {}", path.display()));
            }
            Err(e) => return ToolOutput::error(format!("Cannot read metadata: {}", e)),
        };

        if metadata.file_type().is_symlink() {
            return ToolOutput::error(format!("Symlinks are not allowed: {}", path.display()));
        }

        // Enforce read-before-write: must read file before deleting
        if metadata.file_type().is_file() && !self.tracker.has_been_read(&path) {
            return ToolOutput::error(format!(
                "You must read {} before deleting it. Read the file first to understand what you are deleting.",
                path.display()
            ));
        }

        if metadata.file_type().is_dir() {
            match fs::remove_dir_all(&path).await {
                Ok(()) => ToolOutput::success(serde_json::json!({
                    "path": path.display().to_string(),
                    "deleted": true,
                    "type": "directory",
                })),
                Err(e) => ToolOutput::error(format!("Cannot delete directory: {}", e)),
            }
        } else {
            match fs::remove_file(&path).await {
                Ok(()) => ToolOutput::success(serde_json::json!({
                    "path": path.display().to_string(),
                    "deleted": true,
                    "type": "file",
                })),
                Err(e) => ToolOutput::error(format!("Cannot delete file: {}", e)),
            }
        }
    }

    /// Check if a path exists
    async fn check_exists(&self, path: &str) -> ToolOutput {
        let path = match self.resolve_path(path) {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };

        // Single syscall: get metadata without following symlinks
        let (exists, file_type, size) = match fs::symlink_metadata(&path).await {
            Ok(meta) => {
                let ft = meta.file_type();
                let type_str = if ft.is_symlink() {
                    "symlink"
                } else if ft.is_dir() {
                    "directory"
                } else {
                    "file"
                };
                let size = if ft.is_file() { Some(meta.len()) } else { None };
                (true, type_str, size)
            }
            Err(_) => (false, "none", None),
        };

        ToolOutput::success(serde_json::json!({
            "path": path.display().to_string(),
            "exists": exists,
            "type": file_type,
            "size": size,
        }))
    }
}

/// Batch read parameters
#[derive(Debug, Clone, Deserialize)]
pub struct BatchReadParams {
    /// List of file paths to read
    pub paths: Vec<String>,
    /// Maximum lines per file
    #[serde(default = "default_batch_line_limit")]
    pub line_limit: usize,
    /// Skip files larger than this size (bytes)
    #[serde(default = "default_batch_max_size")]
    pub max_file_size: usize,
    /// Continue on errors and return partial results
    #[serde(default = "default_continue_on_error")]
    pub continue_on_error: bool,
}

/// Batch exists parameters
#[derive(Debug, Clone, Deserialize)]
pub struct BatchExistsParams {
    /// List of paths to check
    pub paths: Vec<String>,
}

/// Batch search parameters
#[derive(Debug, Clone, Deserialize)]
pub struct BatchSearchParams {
    /// Search pattern (regex)
    pub pattern: String,
    /// List of directories or globs to search
    pub locations: Vec<String>,
    /// Maximum total matches to return
    #[serde(default = "default_batch_max_matches")]
    pub max_matches: usize,
    /// Context lines to include before/after matches
    #[serde(default = "default_context_lines")]
    pub context_lines: usize,
}

/// Result for a single file in batch read
#[derive(Debug, Clone, Serialize)]
pub struct BatchReadResult {
    pub path: String,
    pub success: bool,
    pub content: Option<String>,
    pub error: Option<String>,
    pub line_count: Option<usize>,
    pub truncated: bool,
}

/// Result for a single path in batch exists
#[derive(Debug, Clone, Serialize)]
pub struct BatchExistsResult {
    pub path: String,
    pub exists: bool,
    pub is_file: bool,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub error: Option<String>,
}

/// Aggregated search result per location
#[derive(Debug, Clone, Serialize)]
pub struct BatchSearchResult {
    pub location: String,
    pub matches: Vec<SearchMatch>,
    pub match_count: usize,
    pub error: Option<String>,
}

/// Single search match with context
#[derive(Debug, Clone, Serialize)]
pub struct SearchMatch {
    pub file: String,
    pub line_number: usize,
    pub content: String,
    pub context_before: Vec<String>,
    pub context_after: Vec<String>,
}

impl FileTool {
    /// Execute batch read operation
    async fn batch_read(&self, params: BatchReadParams) -> ToolOutput {
        let BatchReadParams {
            paths,
            line_limit,
            max_file_size,
            continue_on_error,
        } = params;

        if paths.len() > MAX_BATCH_READ_FILES {
            return ToolOutput::error(format!(
                "Batch size {} exceeds maximum of {}",
                paths.len(),
                MAX_BATCH_READ_FILES
            ));
        }

        let indexed_paths: Vec<(usize, String)> = paths.into_iter().enumerate().collect();

        let mut indexed_results: Vec<(usize, BatchReadResult)> =
            stream::iter(indexed_paths.into_iter().map(|(idx, path)| async move {
                (
                    idx,
                    self.read_single_for_batch(&path, line_limit, max_file_size)
                        .await,
                )
            }))
            .buffer_unordered(BATCH_IO_CONCURRENCY)
            .collect()
            .await;
        indexed_results.sort_by_key(|(idx, _)| *idx);
        let results: Vec<BatchReadResult> = indexed_results
            .into_iter()
            .map(|(_, result)| result)
            .collect();

        let successful = results.iter().filter(|r| r.success).count();
        let failed = results.len() - successful;

        let mut summary = format!(
            "Read {} files ({} successful, {} failed)",
            results.len(),
            successful,
            failed
        );

        if failed > 0 && continue_on_error {
            summary.push_str(". Returned partial results.");
        }

        ToolOutput {
            success: failed == 0 || continue_on_error,
            result: serde_json::json!({
                "summary": summary,
                "total": results.len(),
                "successful": successful,
                "failed": failed,
                "results": results,
            }),
            error: if failed > 0 && !continue_on_error {
                Some(format!("{} files failed to read", failed))
            } else {
                None
            },
            error_category: None,
            retryable: None,
            retry_after_ms: None,
        }
    }

    async fn read_single_for_batch(
        &self,
        path: &str,
        line_limit: usize,
        max_file_size: usize,
    ) -> BatchReadResult {
        let resolved = match self.resolve_path(path) {
            Ok(p) => p,
            Err(e) => {
                return BatchReadResult {
                    path: path.to_string(),
                    success: false,
                    content: None,
                    error: Some(e),
                    line_count: None,
                    truncated: false,
                };
            }
        };

        let metadata = match fs::symlink_metadata(&resolved).await {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return BatchReadResult {
                    path: resolved.display().to_string(),
                    success: false,
                    content: None,
                    error: Some(format!("File not found: {}", resolved.display())),
                    line_count: None,
                    truncated: false,
                };
            }
            Err(e) => {
                return BatchReadResult {
                    path: resolved.display().to_string(),
                    success: false,
                    content: None,
                    error: Some(format!("Cannot read metadata: {}", e)),
                    line_count: None,
                    truncated: false,
                };
            }
        };

        if !metadata.file_type().is_file() {
            return BatchReadResult {
                path: resolved.display().to_string(),
                success: false,
                content: None,
                error: Some(format!("Not a file: {}", resolved.display())),
                line_count: None,
                truncated: false,
            };
        }

        if metadata.len() as usize > max_file_size {
            return BatchReadResult {
                path: resolved.display().to_string(),
                success: false,
                content: None,
                error: Some(format!(
                    "File too large: {} bytes (max: {} bytes). Use offset and limit parameters for partial reads.",
                    metadata.len(),
                    max_file_size
                )),
                line_count: None,
                truncated: false,
            };
        }

        match fs::read_to_string(&resolved).await {
            Ok(content) => {
                self.tracker.record_read(&resolved);
                let lines: Vec<&str> = content.lines().collect();
                let line_count = lines.len();
                let truncated = line_count > line_limit;
                let content = if truncated {
                    lines[..line_limit].join("\n")
                } else {
                    content
                };

                BatchReadResult {
                    path: resolved.display().to_string(),
                    success: true,
                    content: Some(content),
                    error: None,
                    line_count: Some(line_count),
                    truncated,
                }
            }
            Err(e) => BatchReadResult {
                path: resolved.display().to_string(),
                success: false,
                content: None,
                error: Some(format!("Cannot read file: {}", e)),
                line_count: None,
                truncated: false,
            },
        }
    }

    async fn check_exists_for_batch(&self, path: &str) -> BatchExistsResult {
        let resolved = match self.resolve_path(path) {
            Ok(p) => p,
            Err(e) => {
                return BatchExistsResult {
                    path: path.to_string(),
                    exists: false,
                    is_file: false,
                    is_dir: false,
                    size: None,
                    error: Some(e),
                };
            }
        };

        match fs::symlink_metadata(&resolved).await {
            Ok(meta) => {
                let ft = meta.file_type();
                BatchExistsResult {
                    path: resolved.display().to_string(),
                    exists: true,
                    is_file: ft.is_file(),
                    is_dir: ft.is_dir(),
                    size: if ft.is_file() { Some(meta.len()) } else { None },
                    error: None,
                }
            }
            Err(_) => BatchExistsResult {
                path: resolved.display().to_string(),
                exists: false,
                is_file: false,
                is_dir: false,
                size: None,
                error: None,
            },
        }
    }

    /// Execute batch exists operation
    async fn batch_exists(&self, params: BatchExistsParams) -> ToolOutput {
        if params.paths.len() > MAX_BATCH_EXISTS_PATHS {
            return ToolOutput::error(format!(
                "Batch size {} exceeds maximum of {}",
                params.paths.len(),
                MAX_BATCH_EXISTS_PATHS
            ));
        }

        let indexed_paths: Vec<(usize, String)> = params.paths.into_iter().enumerate().collect();

        let mut indexed_results: Vec<(usize, BatchExistsResult)> = stream::iter(
            indexed_paths
                .into_iter()
                .map(|(idx, path)| async move { (idx, self.check_exists_for_batch(&path).await) }),
        )
        .buffer_unordered(BATCH_IO_CONCURRENCY)
        .collect()
        .await;
        indexed_results.sort_by_key(|(idx, _)| *idx);
        let results: Vec<BatchExistsResult> = indexed_results
            .into_iter()
            .map(|(_, result)| result)
            .collect();

        let existing = results.iter().filter(|r| r.exists).count();

        ToolOutput::success(serde_json::json!({
            "total": results.len(),
            "existing": existing,
            "results": results,
        }))
    }

    /// Execute batch search operation
    async fn batch_search(&self, params: BatchSearchParams) -> ToolOutput {
        if params.locations.len() > MAX_BATCH_SEARCH_LOCATIONS {
            return ToolOutput::error(format!(
                "Location count {} exceeds maximum of {}",
                params.locations.len(),
                MAX_BATCH_SEARCH_LOCATIONS
            ));
        }

        let regex = match Regex::new(&params.pattern) {
            Ok(r) => r,
            Err(e) => return ToolOutput::error(format!("Invalid regex: {}", e)),
        };

        let mut results: Vec<BatchSearchResult> = Vec::new();
        let mut total_matches = 0usize;

        for location in &params.locations {
            if total_matches >= params.max_matches {
                break;
            }

            let remaining = params.max_matches - total_matches;
            let result = self
                .search_location(location, &regex, remaining, params.context_lines)
                .await;
            total_matches += result.match_count;
            results.push(result);
        }

        ToolOutput::success(serde_json::json!({
            "pattern": params.pattern,
            "total_matches": total_matches,
            "locations_searched": results.len(),
            "truncated": total_matches >= params.max_matches,
            "results": results,
        }))
    }

    async fn search_location(
        &self,
        location: &str,
        regex: &Regex,
        max_matches: usize,
        context_lines: usize,
    ) -> BatchSearchResult {
        let mut matches: Vec<SearchMatch> = Vec::new();

        let error = if has_glob(location) {
            (self
                .search_glob_location(location, regex, max_matches, context_lines, &mut matches)
                .await)
                .err()
        } else {
            match self.resolve_path(location) {
                Ok(path) => {
                    self.search_path_with_context(
                        &path,
                        regex,
                        max_matches,
                        context_lines,
                        &mut matches,
                    )
                    .await;
                    None
                }
                Err(e) => Some(e),
            }
        };

        BatchSearchResult {
            location: location.to_string(),
            matches: matches.clone(),
            match_count: matches.len(),
            error,
        }
    }

    async fn search_glob_location(
        &self,
        location: &str,
        regex: &Regex,
        max_matches: usize,
        context_lines: usize,
        matches: &mut Vec<SearchMatch>,
    ) -> std::result::Result<(), String> {
        let (base, pattern) = split_glob_base(location);
        let base = if base.is_empty() { "." } else { base };
        let base_path = self.resolve_path(base)?;
        let pattern = if pattern.is_empty() { "*" } else { pattern };

        self.search_path_with_context_filtered(
            &base_path,
            regex,
            max_matches,
            context_lines,
            matches,
            Some(pattern),
            &base_path,
        )
        .await;

        Ok(())
    }

    fn search_path_with_context<'a>(
        &'a self,
        path: &'a Path,
        regex: &'a Regex,
        max_matches: usize,
        context_lines: usize,
        matches: &'a mut Vec<SearchMatch>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        self.search_path_with_context_filtered(
            path,
            regex,
            max_matches,
            context_lines,
            matches,
            None,
            path,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn search_path_with_context_filtered<'a>(
        &'a self,
        path: &'a Path,
        regex: &'a Regex,
        max_matches: usize,
        context_lines: usize,
        matches: &'a mut Vec<SearchMatch>,
        path_glob: Option<&'a str>,
        base: &'a Path,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            if matches.len() >= max_matches {
                return;
            }

            if path.is_file() {
                if let Some(glob) = path_glob {
                    let rel = normalize_path_for_glob(path, base);
                    if !glob_match(glob, &rel) {
                        return;
                    }
                }

                self.search_in_file_with_context(
                    path,
                    regex,
                    max_matches,
                    context_lines,
                    matches,
                    base,
                )
                .await;
                return;
            }

            let mut read_dir = match fs::read_dir(path).await {
                Ok(rd) => rd,
                Err(_) => return,
            };

            while let Ok(Some(entry)) = read_dir.next_entry().await {
                if matches.len() >= max_matches {
                    break;
                }

                let entry_path = entry.path();
                let file_type = match entry.file_type().await {
                    Ok(ft) => ft,
                    Err(_) => continue,
                };

                if file_type.is_dir() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with('.') {
                        continue;
                    }
                    self.search_path_with_context_filtered(
                        &entry_path,
                        regex,
                        max_matches,
                        context_lines,
                        matches,
                        path_glob,
                        base,
                    )
                    .await;
                } else if file_type.is_file() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if is_likely_binary(&name) {
                        continue;
                    }

                    if let Some(glob) = path_glob {
                        let rel = normalize_path_for_glob(&entry_path, base);
                        if !glob_match(glob, &rel) {
                            continue;
                        }
                    }

                    self.search_in_file_with_context(
                        &entry_path,
                        regex,
                        max_matches,
                        context_lines,
                        matches,
                        base,
                    )
                    .await;
                }
            }
        })
    }

    async fn search_in_file_with_context(
        &self,
        file: &Path,
        regex: &Regex,
        max_matches: usize,
        context_lines: usize,
        matches: &mut Vec<SearchMatch>,
        base: &Path,
    ) {
        let content = match fs::read_to_string(file).await {
            Ok(c) => c,
            Err(_) => return,
        };

        let relative_path = file
            .strip_prefix(base)
            .unwrap_or(file)
            .to_string_lossy()
            .to_string();

        let lines: Vec<&str> = content.lines().collect();

        for (line_index, line) in lines.iter().enumerate() {
            if matches.len() >= max_matches {
                break;
            }

            if regex.is_match(line) {
                let start = line_index.saturating_sub(context_lines);
                let end = (line_index + 1 + context_lines).min(lines.len());
                let context_before = lines[start..line_index]
                    .iter()
                    .map(|line| line.to_string())
                    .collect();
                let context_after = lines[(line_index + 1)..end]
                    .iter()
                    .map(|line| line.to_string())
                    .collect();

                matches.push(SearchMatch {
                    file: relative_path.clone(),
                    line_number: line_index + 1,
                    content: line.to_string(),
                    context_before,
                    context_after,
                });
            }
        }
    }
}

#[async_trait]
impl Tool for FileTool {
    fn name(&self) -> &str {
        "file"
    }

    fn description(&self) -> &str {
        "Perform file and directory operations: read, write, list, search, delete, exists, and batch variants. Use this for file content workflows; for shell command execution, use bash."
    }

    fn parameters_schema(&self) -> Value {
        file_parameters_schema()
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let action: FileAction = serde_json::from_value(input)?;

        async fn check_paths_inner(
            security_gate: Option<&dyn SecurityGate>,
            agent_id: Option<&str>,
            task_id: Option<&str>,
            operation: &str,
            paths: &[String],
        ) -> Result<Option<String>> {
            for path in paths {
                let action = ToolAction {
                    tool_name: "file".to_string(),
                    operation: operation.to_string(),
                    target: path.clone(),
                    summary: format!("File {} {}", operation, path),
                };
                if let Some(message) =
                    check_security(security_gate, action, agent_id, task_id).await?
                {
                    return Ok(Some(message));
                }
            }
            Ok(None)
        }

        match &action {
            FileAction::Read { path, .. } => {
                if let Some(message) = check_paths_inner(
                    self.security_gate.as_deref(),
                    self.agent_id.as_deref(),
                    self.task_id.as_deref(),
                    "read",
                    std::slice::from_ref(path),
                )
                .await?
                {
                    return Ok(ToolOutput::error(message));
                }
            }
            FileAction::Write { path, .. } => {
                if let Some(message) = check_paths_inner(
                    self.security_gate.as_deref(),
                    self.agent_id.as_deref(),
                    self.task_id.as_deref(),
                    "write",
                    std::slice::from_ref(path),
                )
                .await?
                {
                    return Ok(ToolOutput::error(message));
                }
            }
            FileAction::List { path, .. } => {
                if let Some(message) = check_paths_inner(
                    self.security_gate.as_deref(),
                    self.agent_id.as_deref(),
                    self.task_id.as_deref(),
                    "list",
                    std::slice::from_ref(path),
                )
                .await?
                {
                    return Ok(ToolOutput::error(message));
                }
            }
            FileAction::Search { path, .. } => {
                if let Some(message) = check_paths_inner(
                    self.security_gate.as_deref(),
                    self.agent_id.as_deref(),
                    self.task_id.as_deref(),
                    "search",
                    std::slice::from_ref(path),
                )
                .await?
                {
                    return Ok(ToolOutput::error(message));
                }
            }
            FileAction::Delete { path } => {
                if let Some(message) = check_paths_inner(
                    self.security_gate.as_deref(),
                    self.agent_id.as_deref(),
                    self.task_id.as_deref(),
                    "delete",
                    std::slice::from_ref(path),
                )
                .await?
                {
                    return Ok(ToolOutput::error(message));
                }
            }
            FileAction::Exists { path } => {
                if let Some(message) = check_paths_inner(
                    self.security_gate.as_deref(),
                    self.agent_id.as_deref(),
                    self.task_id.as_deref(),
                    "exists",
                    std::slice::from_ref(path),
                )
                .await?
                {
                    return Ok(ToolOutput::error(message));
                }
            }
            FileAction::BatchRead { paths, .. } => {
                if let Some(message) = check_paths_inner(
                    self.security_gate.as_deref(),
                    self.agent_id.as_deref(),
                    self.task_id.as_deref(),
                    "read",
                    paths,
                )
                .await?
                {
                    return Ok(ToolOutput::error(message));
                }
            }
            FileAction::BatchExists { paths } => {
                if let Some(message) = check_paths_inner(
                    self.security_gate.as_deref(),
                    self.agent_id.as_deref(),
                    self.task_id.as_deref(),
                    "exists",
                    paths,
                )
                .await?
                {
                    return Ok(ToolOutput::error(message));
                }
            }
            FileAction::BatchSearch { locations, .. } => {
                if let Some(message) = check_paths_inner(
                    self.security_gate.as_deref(),
                    self.agent_id.as_deref(),
                    self.task_id.as_deref(),
                    "search",
                    locations,
                )
                .await?
                {
                    return Ok(ToolOutput::error(message));
                }
            }
        }

        let output = match action {
            FileAction::Read {
                path,
                offset,
                limit,
            } => self.read_file(&path, offset, limit).await,
            FileAction::Write {
                path,
                content,
                append,
            } => self.write_file(&path, &content, append).await,
            FileAction::List {
                path,
                recursive,
                pattern,
            } => self.list_dir(&path, recursive, pattern.as_deref()).await,
            FileAction::Search {
                path,
                pattern,
                file_pattern,
            } => {
                self.search_files(&path, &pattern, file_pattern.as_deref())
                    .await
            }
            FileAction::Delete { path } => self.delete_file(&path).await,
            FileAction::Exists { path } => self.check_exists(&path).await,
            FileAction::BatchRead {
                paths,
                line_limit,
                max_file_size,
                continue_on_error,
            } => {
                self.batch_read(BatchReadParams {
                    paths,
                    line_limit,
                    max_file_size,
                    continue_on_error,
                })
                .await
            }
            FileAction::BatchExists { paths } => {
                self.batch_exists(BatchExistsParams { paths }).await
            }
            FileAction::BatchSearch {
                pattern,
                locations,
                max_matches,
                context_lines,
            } => {
                self.batch_search(BatchSearchParams {
                    pattern,
                    locations,
                    max_matches,
                    context_lines,
                })
                .await
            }
        };

        Ok(output.classify_if_error(classify_file_error_message))
    }
}

fn classify_file_error_message(message: &str) -> (ToolErrorCategory, bool, Option<u64>) {
    let normalized = message.to_ascii_lowercase();

    if normalized.contains("not found")
        || normalized.contains("no such file")
        || normalized.contains("no such directory")
    {
        return (ToolErrorCategory::NotFound, false, None);
    }

    if normalized.contains("permission denied")
        || normalized.contains("operation not permitted")
        || normalized.contains("access denied")
    {
        return (ToolErrorCategory::Auth, false, None);
    }

    if normalized.contains("invalid regex")
        || normalized.contains("invalid path")
        || normalized.contains("escapes allowed base directory")
        || normalized.contains("too many")
        || normalized.contains("invalid")
    {
        return (ToolErrorCategory::Config, false, None);
    }

    (ToolErrorCategory::Execution, false, None)
}

/// Simple glob matching (supports * and ?)
fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();

    glob_match_helper(&pattern_chars, &text_chars)
}

fn glob_match_helper(pattern: &[char], text: &[char]) -> bool {
    match (pattern.first(), text.first()) {
        (None, None) => true,
        (Some('*'), _) => {
            // * matches zero or more characters
            glob_match_helper(&pattern[1..], text)
                || (!text.is_empty() && glob_match_helper(pattern, &text[1..]))
        }
        (Some('?'), Some(_)) => {
            // ? matches exactly one character
            glob_match_helper(&pattern[1..], &text[1..])
        }
        (Some(p), Some(t)) if *p == *t => glob_match_helper(&pattern[1..], &text[1..]),
        (Some(_), None) => {
            // Check if remaining pattern is all *
            pattern.iter().all(|c| *c == '*')
        }
        _ => false,
    }
}

/// Determine if a string contains glob characters
fn has_glob(value: &str) -> bool {
    value.contains('*') || value.contains('?')
}

/// Split a glob pattern into its base directory and the glob pattern
fn split_glob_base(value: &str) -> (&str, &str) {
    let mut split_index = value.len();
    for (idx, ch) in value.char_indices() {
        if ch == '*' || ch == '?' {
            split_index = idx;
            break;
        }
    }

    if split_index == value.len() {
        return (value, "");
    }

    let base = &value[..split_index];
    let base = base.trim_end_matches('/');
    let pattern = value.trim_start_matches(base).trim_start_matches('/');
    (base, pattern)
}

fn normalize_path_for_glob(path: &Path, base: &Path) -> String {
    let relative = path.strip_prefix(base).unwrap_or(path);
    relative
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}
