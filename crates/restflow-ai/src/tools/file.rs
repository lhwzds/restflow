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

use async_trait::async_trait;
use lsp_types::DiagnosticSeverity;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use super::traits::{Tool, ToolOutput};
use crate::error::Result;
use crate::lsp::LspManager;

/// Maximum file size to read (1MB)
const DEFAULT_MAX_READ_BYTES: usize = 1_000_000;

/// Default number of lines to read
const DEFAULT_LINE_LIMIT: usize = 2000;

/// Maximum entries to return in directory listing
const MAX_LIST_ENTRIES: usize = 1000;

/// Maximum search matches to return
const MAX_SEARCH_MATCHES: usize = 100;

/// File operations tool
#[derive(Debug, Clone)]
pub struct FileTool {
    /// Base directory for file operations (security boundary)
    base_dir: Option<PathBuf>,
    /// Maximum file size to read in bytes
    max_read_bytes: usize,
    /// Optional LSP manager for diagnostics
    lsp_manager: Option<Arc<Mutex<LspManager>>>,
}

impl Default for FileTool {
    fn default() -> Self {
        Self::new()
    }
}

impl FileTool {
    /// Create a new FileTool with default settings
    pub fn new() -> Self {
        Self {
            base_dir: None,
            max_read_bytes: DEFAULT_MAX_READ_BYTES,
            lsp_manager: None,
        }
    }

    /// Set base directory for file operations (security boundary)
    /// All paths will be resolved relative to this directory
    pub fn with_base_dir(mut self, base: impl Into<PathBuf>) -> Self {
        self.base_dir = Some(base.into());
        self
    }

    /// Set maximum read size in bytes
    pub fn with_max_read(mut self, bytes: usize) -> Self {
        self.max_read_bytes = bytes;
        self
    }

    /// Attach an LSP manager for diagnostics.
    pub fn with_lsp_manager(mut self, manager: Arc<Mutex<LspManager>>) -> Self {
        self.lsp_manager = Some(manager);
        self
    }

    /// Resolve and validate a path against the base directory
    fn resolve_path(&self, path: &str) -> std::result::Result<PathBuf, String> {
        let path = PathBuf::from(path);

        if let Some(base) = &self.base_dir {
            let resolved = if path.is_absolute() {
                path
            } else {
                base.join(&path)
            };

            // Check if the canonical path is within base
            let canonical_base = if base.exists() {
                base.canonicalize().map_err(|e| e.to_string())?
            } else {
                normalize_path(base)
            };

            if resolved.exists() {
                let canonical = resolved.canonicalize().map_err(|e| e.to_string())?;
                if !canonical.starts_with(&canonical_base) {
                    return Err("Path escapes base directory".to_string());
                }
                return Ok(canonical);
            }

            if base.exists() {
                let Some((ancestor, suffix)) = find_existing_ancestor(&resolved) else {
                    return Err("Path escapes base directory".to_string());
                };
                let canonical_parent = ancestor.canonicalize().map_err(|e| e.to_string())?;
                let candidate = normalize_path(&canonical_parent.join(suffix));
                if !candidate.starts_with(&canonical_base) {
                    return Err("Path escapes base directory".to_string());
                }
                return Ok(candidate);
            }

            let normalized = normalize_path(&resolved);
            if !normalized.starts_with(&canonical_base) {
                return Err("Path escapes base directory".to_string());
            }

            Ok(normalized)
        } else {
            // No base directory restriction
            if path.is_absolute() {
                Ok(path)
            } else {
                // Resolve relative to current directory
                std::env::current_dir()
                    .map(|cwd| cwd.join(&path))
                    .map_err(|e| e.to_string())
            }
        }
    }

    /// Read file with line numbers
    async fn read_file(
        &self,
        path: &str,
        offset: usize,
        limit: Option<usize>,
    ) -> ToolOutput {
        let path = match self.resolve_path(path) {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };

        if !path.exists() {
            return ToolOutput::error(format!("File not found: {}", path.display()));
        }

        if !path.is_file() {
            return ToolOutput::error(format!("Not a file: {}", path.display()));
        }

        // Check file size first
        let metadata = match fs::metadata(&path).await {
            Ok(m) => m,
            Err(e) => return ToolOutput::error(format!("Cannot read metadata: {}", e)),
        };

        if metadata.len() as usize > self.max_read_bytes {
            return ToolOutput::error(format!(
                "File too large ({} bytes). Maximum: {} bytes. Use offset/limit for partial reads.",
                metadata.len(),
                self.max_read_bytes
            ));
        }

        let content = match fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => return ToolOutput::error(format!("Cannot read file: {}", e)),
        };

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

    /// Write or append to a file
    async fn write_file(
        &self,
        path: &str,
        content: &str,
        append: bool,
    ) -> ToolOutput {
        let path = match self.resolve_path(path) {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };

        // Create parent directories if needed
        if let Some(parent) = path.parent()
            && !parent.exists()
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
                let mut output = serde_json::json!({
                    "path": path.display().to_string(),
                    "bytes_written": content.len(),
                    "action": if append { "appended" } else { "written" },
                });

                if let Some(manager) = &self.lsp_manager {
                    let diagnostics = {
                        let mut manager = manager.lock().await;
                        manager.notify_change(&path, content).await
                    };

                    match diagnostics {
                        Ok(list) if !list.is_empty() => {
                            let formatted: Vec<Value> = list
                                .iter()
                                .map(|diag| {
                                    let severity = match diag.severity {
                                        Some(DiagnosticSeverity::ERROR) => "error",
                                        Some(DiagnosticSeverity::WARNING) => "warning",
                                        Some(DiagnosticSeverity::INFORMATION) => "information",
                                        Some(DiagnosticSeverity::HINT) => "hint",
                                        None => "unknown",
                                    };

                                    serde_json::json!({
                                        "severity": severity,
                                        "message": diag.message,
                                        "line": diag.range.start.line + 1,
                                        "character": diag.range.start.character + 1
                                    })
                                })
                                .collect();

                            if let Some(map) = output.as_object_mut() {
                                map.insert(
                                    "diagnostics".to_string(),
                                    serde_json::Value::Array(formatted),
                                );
                            }
                        }
                        Ok(_) => {}
                        Err(err) => {
                            if let Some(map) = output.as_object_mut() {
                                map.insert(
                                    "diagnostics_error".to_string(),
                                    serde_json::Value::String(err.to_string()),
                                );
                            }
                        }
                    }
                }

                ToolOutput::success(output)
            }
            Err(e) => ToolOutput::error(format!("Cannot write file: {}", e)),
        }
    }

    /// List directory contents
    async fn list_dir(
        &self,
        path: &str,
        recursive: bool,
        pattern: Option<&str>,
    ) -> ToolOutput {
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
            self.list_recursive(&path, &mut entries, pattern, &path).await;
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
                        self.list_recursive(&entry_path, entries, pattern, base).await;
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
                    self.list_recursive(&entry_path, entries, pattern, base).await;
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

        let regex = match Regex::new(pattern) {
            Ok(r) => r,
            Err(e) => return ToolOutput::error(format!("Invalid regex pattern: {}", e)),
        };

        let mut matches: Vec<Value> = Vec::new();
        self.search_recursive(&path, &regex, file_pattern, &mut matches, &path).await;

        ToolOutput::success(serde_json::json!({
            "pattern": pattern,
            "search_path": path.display().to_string(),
            "match_count": matches.len(),
            "truncated": matches.len() >= MAX_SEARCH_MATCHES,
            "matches": matches,
        }))
    }

    /// Recursively search for text in files
    fn search_recursive<'a>(
        &'a self,
        dir: &'a Path,
        regex: &'a Regex,
        file_pattern: Option<&'a str>,
        matches: &'a mut Vec<Value>,
        base: &'a Path,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            if matches.len() >= MAX_SEARCH_MATCHES {
                return;
            }

            if dir.is_file() {
                // Search in single file
                let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                if let Some(p) = file_pattern
                    && !glob_match(p, &name)
                {
                    return;
                }
                self.search_in_file(dir, regex, matches, base).await;
                return;
            }

            let mut read_dir = match fs::read_dir(dir).await {
                Ok(rd) => rd,
                Err(_) => return,
            };

            while let Ok(Some(entry)) = read_dir.next_entry().await {
                if matches.len() >= MAX_SEARCH_MATCHES {
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
                    self.search_recursive(&entry_path, regex, file_pattern, matches, base).await;
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

                    self.search_in_file(&entry_path, regex, matches, base).await;
                }
            }
        })
    }

    /// Search for pattern in a single file
    async fn search_in_file(
        &self,
        file: &Path,
        regex: &Regex,
        matches: &mut Vec<Value>,
        base: &Path,
    ) {
        let content = match fs::read_to_string(file).await {
            Ok(c) => c,
            Err(_) => return, // Skip files that can't be read as text
        };

        let relative_path = file
            .strip_prefix(base)
            .unwrap_or(file)
            .to_string_lossy()
            .to_string();

        for (line_num, line) in content.lines().enumerate() {
            if matches.len() >= MAX_SEARCH_MATCHES {
                break;
            }

            if regex.is_match(line) {
                matches.push(serde_json::json!({
                    "file": relative_path,
                    "line": line_num + 1,
                    "content": line.chars().take(200).collect::<String>(),
                }));
            }
        }
    }

    /// Delete a file
    async fn delete_file(&self, path: &str) -> ToolOutput {
        let path = match self.resolve_path(path) {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };

        if !path.exists() {
            return ToolOutput::error(format!("File not found: {}", path.display()));
        }

        if path.is_dir() {
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

        let exists = path.exists();
        let file_type = if exists {
            if path.is_dir() {
                "directory"
            } else if path.is_symlink() {
                "symlink"
            } else {
                "file"
            }
        } else {
            "none"
        };

        let size = if exists && path.is_file() {
            fs::metadata(&path).await.ok().map(|m| m.len())
        } else {
            None
        };

        ToolOutput::success(serde_json::json!({
            "path": path.display().to_string(),
            "exists": exists,
            "type": file_type,
            "size": size,
        }))
    }
}

/// Input parameters for file operations
#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum FileAction {
    Read {
        path: String,
        #[serde(default)]
        offset: usize,
        #[serde(default)]
        limit: Option<usize>,
    },
    Write {
        path: String,
        content: String,
        #[serde(default)]
        append: bool,
    },
    List {
        path: String,
        #[serde(default)]
        recursive: bool,
        #[serde(default)]
        pattern: Option<String>,
    },
    Search {
        path: String,
        pattern: String,
        #[serde(default)]
        file_pattern: Option<String>,
    },
    Delete {
        path: String,
    },
    Exists {
        path: String,
    },
}

/// Output for file read operation (for reference/documentation)
#[allow(dead_code)]
#[derive(Debug, Serialize)]
pub struct FileReadOutput {
    pub path: String,
    pub total_lines: usize,
    pub showing: String,
    pub content: String,
}

/// Output for file write operation (for reference/documentation)
#[allow(dead_code)]
#[derive(Debug, Serialize)]
pub struct FileWriteOutput {
    pub path: String,
    pub bytes_written: usize,
    pub action: String,
}

#[async_trait]
impl Tool for FileTool {
    fn name(&self) -> &str {
        "file"
    }

    fn description(&self) -> &str {
        "File operations: read, write, list, search, delete. \
         Use 'read' to view file contents with line numbers, 'write' to create/modify files, \
         'list' to see directory contents, 'search' to find text in files using regex."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["read", "write", "list", "search", "delete", "exists"],
                    "description": "The file operation to perform"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory path"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write (for write action)"
                },
                "pattern": {
                    "type": "string",
                    "description": "Search pattern (regex for search, glob for list)"
                },
                "file_pattern": {
                    "type": "string",
                    "description": "Filter files by glob pattern (for search action)"
                },
                "append": {
                    "type": "boolean",
                    "description": "Append to file instead of overwrite"
                },
                "recursive": {
                    "type": "boolean",
                    "description": "List directories recursively"
                },
                "offset": {
                    "type": "integer",
                    "description": "Start reading from this line number (0-indexed)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum lines to read"
                }
            },
            "required": ["action", "path"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let action: FileAction = serde_json::from_value(input)?;

        let output = match action {
            FileAction::Read { path, offset, limit } => {
                self.read_file(&path, offset, limit).await
            }
            FileAction::Write { path, content, append } => {
                self.write_file(&path, &content, append).await
            }
            FileAction::List { path, recursive, pattern } => {
                self.list_dir(&path, recursive, pattern.as_deref()).await
            }
            FileAction::Search { path, pattern, file_pattern } => {
                self.search_files(&path, &pattern, file_pattern.as_deref()).await
            }
            FileAction::Delete { path } => {
                self.delete_file(&path).await
            }
            FileAction::Exists { path } => {
                self.check_exists(&path).await
            }
        };

        Ok(output)
    }
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
            glob_match_helper(&pattern[1..], text) ||
            (!text.is_empty() && glob_match_helper(pattern, &text[1..]))
        }
        (Some('?'), Some(_)) => {
            // ? matches exactly one character
            glob_match_helper(&pattern[1..], &text[1..])
        }
        (Some(p), Some(t)) if *p == *t => {
            glob_match_helper(&pattern[1..], &text[1..])
        }
        (Some(_), None) => {
            // Check if remaining pattern is all *
            pattern.iter().all(|c| *c == '*')
        }
        _ => false,
    }
}

/// Normalize a path without canonicalizing (for non-existent paths)
fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                result.pop();
            }
            std::path::Component::CurDir => {}
            c => result.push(c),
        }
    }
    result
}

fn find_existing_ancestor(path: &Path) -> Option<(PathBuf, PathBuf)> {
    let mut ancestor = path.to_path_buf();
    loop {
        if ancestor.exists() {
            let suffix = path
                .strip_prefix(&ancestor)
                .unwrap_or_else(|_| Path::new(""))
                .to_path_buf();
            return Some((ancestor, suffix));
        }

        if !ancestor.pop() {
            return None;
        }
    }
}

/// Check if a file is likely binary based on extension
fn is_likely_binary(name: &str) -> bool {
    let binary_extensions = [
        ".exe", ".dll", ".so", ".dylib", ".a", ".o", ".obj",
        ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".ico", ".webp",
        ".mp3", ".mp4", ".avi", ".mov", ".mkv", ".wav", ".flac",
        ".zip", ".tar", ".gz", ".bz2", ".xz", ".7z", ".rar",
        ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx",
        ".wasm", ".pyc", ".pyo", ".class", ".jar",
        ".ttf", ".otf", ".woff", ".woff2", ".eot",
    ];
    
    let lower = name.to_lowercase();
    binary_extensions.iter().any(|ext| lower.ends_with(ext))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_file_tool_new() {
        let tool = FileTool::new();
        assert!(tool.base_dir.is_none());
        assert_eq!(tool.max_read_bytes, DEFAULT_MAX_READ_BYTES);
    }

    #[test]
    fn test_file_tool_with_base_dir() {
        let tool = FileTool::new().with_base_dir("/tmp");
        assert_eq!(tool.base_dir, Some(PathBuf::from("/tmp")));
    }

    #[test]
    fn test_file_tool_with_max_read() {
        let tool = FileTool::new().with_max_read(50_000);
        assert_eq!(tool.max_read_bytes, 50_000);
    }

    #[test]
    fn test_file_tool_name() {
        let tool = FileTool::new();
        assert_eq!(tool.name(), "file");
    }

    #[test]
    fn test_file_tool_description() {
        let tool = FileTool::new();
        assert!(tool.description().contains("File operations"));
    }

    #[test]
    fn test_file_tool_schema() {
        let tool = FileTool::new();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["action"].is_object());
        assert!(schema["properties"]["path"].is_object());
    }

    #[test]
    fn test_glob_match_exact() {
        assert!(glob_match("hello", "hello"));
        assert!(!glob_match("hello", "world"));
    }

    #[test]
    fn test_glob_match_wildcard() {
        assert!(glob_match("*.rs", "main.rs"));
        assert!(glob_match("*.rs", "test.rs"));
        assert!(!glob_match("*.rs", "main.txt"));
    }

    #[test]
    fn test_glob_match_question() {
        assert!(glob_match("test?.rs", "test1.rs"));
        assert!(glob_match("test?.rs", "testa.rs"));
        assert!(!glob_match("test?.rs", "test12.rs"));
    }

    #[test]
    fn test_glob_match_complex() {
        assert!(glob_match("src/*.rs", "src/main.rs"));
        assert!(glob_match("**/test.rs", "src/test.rs"));
        assert!(glob_match("*.?s", "file.rs"));
    }

    #[test]
    fn test_is_likely_binary() {
        assert!(is_likely_binary("image.png"));
        assert!(is_likely_binary("archive.zip"));
        assert!(is_likely_binary("video.MP4"));
        assert!(!is_likely_binary("code.rs"));
        assert!(!is_likely_binary("readme.md"));
    }

    #[test]
    fn test_file_action_read_deserialization() {
        let action: FileAction = serde_json::from_value(serde_json::json!({
            "action": "read",
            "path": "/tmp/test.txt"
        }))
        .unwrap();

        match action {
            FileAction::Read { path, offset, limit } => {
                assert_eq!(path, "/tmp/test.txt");
                assert_eq!(offset, 0);
                assert!(limit.is_none());
            }
            _ => panic!("Expected Read action"),
        }
    }

    #[test]
    fn test_file_action_write_deserialization() {
        let action: FileAction = serde_json::from_value(serde_json::json!({
            "action": "write",
            "path": "/tmp/test.txt",
            "content": "hello world"
        }))
        .unwrap();

        match action {
            FileAction::Write { path, content, append } => {
                assert_eq!(path, "/tmp/test.txt");
                assert_eq!(content, "hello world");
                assert!(!append);
            }
            _ => panic!("Expected Write action"),
        }
    }

    #[test]
    fn test_file_action_list_deserialization() {
        let action: FileAction = serde_json::from_value(serde_json::json!({
            "action": "list",
            "path": "/tmp",
            "recursive": true,
            "pattern": "*.rs"
        }))
        .unwrap();

        match action {
            FileAction::List { path, recursive, pattern } => {
                assert_eq!(path, "/tmp");
                assert!(recursive);
                assert_eq!(pattern, Some("*.rs".to_string()));
            }
            _ => panic!("Expected List action"),
        }
    }

    #[tokio::test]
    async fn test_write_and_read_file() {
        let temp_dir = TempDir::new().unwrap();
        let tool = FileTool::new();
        
        let file_path = temp_dir.path().join("test.txt").display().to_string();
        
        // Write file
        let output = tool.execute(serde_json::json!({
            "action": "write",
            "path": &file_path,
            "content": "line 1\nline 2\nline 3"
        })).await.unwrap();
        
        assert!(output.success);
        
        // Read file
        let output = tool.execute(serde_json::json!({
            "action": "read",
            "path": &file_path
        })).await.unwrap();
        
        assert!(output.success);
        assert!(output.result["total_lines"].as_u64().unwrap() == 3);
    }

    #[tokio::test]
    async fn test_write_append() {
        let temp_dir = TempDir::new().unwrap();
        let tool = FileTool::new();
        
        let file_path = temp_dir.path().join("append.txt").display().to_string();
        
        // Write initial content
        tool.execute(serde_json::json!({
            "action": "write",
            "path": &file_path,
            "content": "first\n"
        })).await.unwrap();
        
        // Append more content
        tool.execute(serde_json::json!({
            "action": "write",
            "path": &file_path,
            "content": "second\n",
            "append": true
        })).await.unwrap();
        
        // Read and verify
        let output = tool.execute(serde_json::json!({
            "action": "read",
            "path": &file_path
        })).await.unwrap();
        
        let content = output.result["content"].as_str().unwrap();
        assert!(content.contains("first"));
        assert!(content.contains("second"));
    }

    #[tokio::test]
    async fn test_list_directory() {
        let temp_dir = TempDir::new().unwrap();
        let tool = FileTool::new();
        
        // Create some files
        fs::write(temp_dir.path().join("file1.txt"), "content").await.unwrap();
        fs::write(temp_dir.path().join("file2.rs"), "content").await.unwrap();
        fs::create_dir(temp_dir.path().join("subdir")).await.unwrap();
        
        let output = tool.execute(serde_json::json!({
            "action": "list",
            "path": temp_dir.path().display().to_string()
        })).await.unwrap();
        
        assert!(output.success);
        assert!(output.result["count"].as_u64().unwrap() >= 3);
    }

    #[tokio::test]
    async fn test_list_with_pattern() {
        let temp_dir = TempDir::new().unwrap();
        let tool = FileTool::new();
        
        // Create files
        fs::write(temp_dir.path().join("file1.txt"), "content").await.unwrap();
        fs::write(temp_dir.path().join("file2.rs"), "content").await.unwrap();
        fs::write(temp_dir.path().join("file3.txt"), "content").await.unwrap();
        
        let output = tool.execute(serde_json::json!({
            "action": "list",
            "path": temp_dir.path().display().to_string(),
            "pattern": "*.txt"
        })).await.unwrap();
        
        assert!(output.success);
        assert_eq!(output.result["count"].as_u64().unwrap(), 2);
    }

    #[tokio::test]
    async fn test_search_files() {
        let temp_dir = TempDir::new().unwrap();
        let tool = FileTool::new();
        
        // Create files with content
        fs::write(temp_dir.path().join("file1.txt"), "hello world\ngoodbye world").await.unwrap();
        fs::write(temp_dir.path().join("file2.txt"), "no match here").await.unwrap();
        
        let output = tool.execute(serde_json::json!({
            "action": "search",
            "path": temp_dir.path().display().to_string(),
            "pattern": "world"
        })).await.unwrap();
        
        assert!(output.success);
        assert!(output.result["match_count"].as_u64().unwrap() >= 2);
    }

    #[tokio::test]
    async fn test_exists() {
        let temp_dir = TempDir::new().unwrap();
        let tool = FileTool::new();
        
        let file_path = temp_dir.path().join("exists.txt");
        fs::write(&file_path, "content").await.unwrap();
        
        // Check existing file
        let output = tool.execute(serde_json::json!({
            "action": "exists",
            "path": file_path.display().to_string()
        })).await.unwrap();
        
        assert!(output.success);
        assert!(output.result["exists"].as_bool().unwrap());
        assert_eq!(output.result["type"].as_str().unwrap(), "file");
        
        // Check non-existing file
        let output = tool.execute(serde_json::json!({
            "action": "exists",
            "path": temp_dir.path().join("nonexistent.txt").display().to_string()
        })).await.unwrap();
        
        assert!(output.success);
        assert!(!output.result["exists"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn test_delete_file() {
        let temp_dir = TempDir::new().unwrap();
        let tool = FileTool::new();
        
        let file_path = temp_dir.path().join("delete_me.txt");
        fs::write(&file_path, "content").await.unwrap();
        
        assert!(file_path.exists());
        
        let output = tool.execute(serde_json::json!({
            "action": "delete",
            "path": file_path.display().to_string()
        })).await.unwrap();
        
        assert!(output.success);
        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_read_with_offset_and_limit() {
        let temp_dir = TempDir::new().unwrap();
        let tool = FileTool::new();
        
        let file_path = temp_dir.path().join("lines.txt");
        fs::write(&file_path, "line 0\nline 1\nline 2\nline 3\nline 4").await.unwrap();
        
        let output = tool.execute(serde_json::json!({
            "action": "read",
            "path": file_path.display().to_string(),
            "offset": 1,
            "limit": 2
        })).await.unwrap();
        
        assert!(output.success);
        let content = output.result["content"].as_str().unwrap();
        assert!(content.contains("line 1"));
        assert!(content.contains("line 2"));
        assert!(!content.contains("line 0"));
        assert!(!content.contains("line 3"));
    }

    #[tokio::test]
    async fn test_base_dir_restriction() {
        let temp_dir = TempDir::new().unwrap();
        let tool = FileTool::new().with_base_dir(temp_dir.path());
        
        // Try to escape base directory
        let output = tool.execute(serde_json::json!({
            "action": "read",
            "path": "../../../etc/passwd"
        })).await.unwrap();
        
        assert!(!output.success);
        assert!(output.error.as_ref().unwrap().contains("escapes base directory"));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_base_dir_symlink_escape_blocked() {
        use std::os::unix::fs::symlink;

        let base_dir = TempDir::new().unwrap();
        let outside_dir = TempDir::new().unwrap();
        let tool = FileTool::new().with_base_dir(base_dir.path());

        let link_path = base_dir.path().join("link");
        symlink(outside_dir.path(), &link_path).unwrap();

        let output = tool
            .execute(serde_json::json!({
                "action": "write",
                "path": "link/newfile.txt",
                "content": "nope"
            }))
            .await
            .unwrap();

        assert!(!output.success);
        assert!(output.error.as_ref().unwrap().contains("escapes base directory"));
    }

    #[tokio::test]
    async fn test_read_nonexistent_file() {
        let tool = FileTool::new();
        
        let output = tool.execute(serde_json::json!({
            "action": "read",
            "path": "/nonexistent/path/file.txt"
        })).await.unwrap();
        
        assert!(!output.success);
        assert!(output.error.as_ref().unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_write_creates_parent_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let tool = FileTool::new();
        
        let deep_path = temp_dir.path().join("a/b/c/file.txt");
        
        let output = tool.execute(serde_json::json!({
            "action": "write",
            "path": deep_path.display().to_string(),
            "content": "nested content"
        })).await.unwrap();
        
        assert!(output.success);
        assert!(deep_path.exists());
    }
}
