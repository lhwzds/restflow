//! Glob pattern file matching tool for AI agents.
//!
//! Provides fast file name pattern matching with:
//! - Full glob syntax (`**`, `{a,b}`, `[abc]`, `?`, `*`)
//! - Results sorted by modification time (newest first)
//! - Automatic skipping of hidden/generated directories
//! - Configurable base directory

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tokio::fs;

use crate::Result;
use crate::{Tool, ToolOutput};

/// Maximum entries to return
const MAX_RESULTS: usize = 1000;

/// Directories to skip during traversal
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    ".node_modules",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    ".tox",
    "target",
    "dist",
    "build",
    ".next",
    ".nuxt",
    ".venv",
    "venv",
];

#[derive(Debug, Deserialize)]
struct GlobInput {
    pattern: String,
    path: Option<String>,
}

/// Glob pattern matching tool that finds files by name patterns.
#[derive(Clone)]
pub struct GlobTool {
    base_dir: Option<PathBuf>,
}

impl Default for GlobTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobTool {
    pub fn new() -> Self {
        Self { base_dir: None }
    }

    pub fn with_base_dir(mut self, base: impl Into<PathBuf>) -> Self {
        self.base_dir = Some(base.into());
        self
    }

    fn resolve_base(&self, path: Option<&str>) -> PathBuf {
        if let Some(p) = path {
            PathBuf::from(p)
        } else if let Some(base) = &self.base_dir {
            base.clone()
        } else {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        }
    }
}

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "Fast file pattern matching tool. Supports glob patterns like \"**/*.rs\" or \"src/**/*.ts\". Returns matching file paths sorted by modification time (newest first)."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "description": "Glob pattern to match files (e.g. \"**/*.rs\", \"src/{main,lib}.rs\")",
                    "type": "string"
                },
                "path": {
                    "description": "Base directory to search in. Defaults to current working directory.",
                    "type": "string"
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: GlobInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(err) => return Ok(ToolOutput::error(format!("Invalid input: {}", err))),
        };

        let base = self.resolve_base(params.path.as_deref());
        if !base.is_dir() {
            return Ok(ToolOutput::error(format!(
                "Directory not found: {}",
                base.display()
            )));
        }

        // Split pattern into prefix path + glob part.
        // E.g. "src/**/*.rs" -> walk from base/src with pattern "**/*.rs"
        let (walk_root, glob_pattern) = split_pattern(&base, &params.pattern);

        if !walk_root.is_dir() {
            return Ok(ToolOutput::success(json!({
                "files": [],
                "total": 0
            })));
        }

        let mut matches: Vec<(String, SystemTime)> = Vec::new();
        walk_and_match(&walk_root, &walk_root, &glob_pattern, &mut matches).await;

        // Sort by mtime descending (newest first)
        matches.sort_by(|a, b| b.1.cmp(&a.1));

        let total = matches.len();
        let truncated = total > MAX_RESULTS;
        let files: Vec<String> = matches.into_iter().take(MAX_RESULTS).map(|(p, _)| p).collect();

        Ok(ToolOutput::success(json!({
            "files": files,
            "total": total,
            "truncated": truncated
        })))
    }
}

/// Split a glob pattern into a concrete prefix (for walking) and the glob remainder.
///
/// E.g. `src/components/**/*.tsx` -> (base/src/components, `**/*.tsx`)
/// E.g. `**/*.rs` -> (base, `**/*.rs`)
fn split_pattern(base: &Path, pattern: &str) -> (PathBuf, String) {
    let parts: Vec<&str> = pattern.split('/').collect();
    let mut prefix = base.to_path_buf();
    let mut glob_start = 0;

    for (i, part) in parts.iter().enumerate() {
        // If the part contains glob metacharacters, stop here
        if part.contains('*') || part.contains('?') || part.contains('[') || part.contains('{') {
            glob_start = i;
            break;
        }
        prefix = prefix.join(part);
        glob_start = i + 1;
    }

    let glob_part = if glob_start < parts.len() {
        parts[glob_start..].join("/")
    } else {
        // Pattern was entirely concrete — match the exact file
        String::new()
    };

    (prefix, glob_part)
}

/// Recursively walk directories and collect glob-matching files.
#[async_recursion::async_recursion]
async fn walk_and_match(
    root: &Path,
    dir: &Path,
    pattern: &str,
    results: &mut Vec<(String, SystemTime)>,
) {
    if results.len() >= MAX_RESULTS * 2 {
        return; // Stop early if we have more than enough
    }

    let mut entries = match fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(_) => return,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        let file_name = match entry.file_name().into_string() {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Skip hidden directories and known generated dirs
        if (file_name.starts_with('.') || SKIP_DIRS.contains(&file_name.as_str()))
            && path.is_dir()
        {
            continue;
        }

        if path.is_dir() {
            walk_and_match(root, &path, pattern, results).await;
        } else {
            let relative = match path.strip_prefix(root) {
                Ok(r) => r.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/"),
                Err(_) => continue,
            };

            let matched = if pattern.is_empty() {
                // Exact file match (pattern was entirely concrete)
                relative.is_empty() || path == root
            } else {
                glob_match::glob_match(pattern, &relative)
            };

            if matched {
                let mtime = entry
                    .metadata()
                    .await
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                results.push((path.to_string_lossy().to_string(), mtime));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::fs;

    async fn setup_test_dir() -> tempfile::TempDir {
        let dir = tempdir().unwrap();
        let base = dir.path();

        // Create structure:
        // src/main.rs
        // src/lib.rs
        // src/utils/helpers.rs
        // src/utils/config.rs
        // tests/test_main.rs
        // .git/config
        // node_modules/pkg/index.js
        fs::create_dir_all(base.join("src/utils")).await.unwrap();
        fs::create_dir_all(base.join("tests")).await.unwrap();
        fs::create_dir_all(base.join(".git")).await.unwrap();
        fs::create_dir_all(base.join("node_modules/pkg")).await.unwrap();

        fs::write(base.join("src/main.rs"), "fn main() {}").await.unwrap();
        fs::write(base.join("src/lib.rs"), "pub mod utils;").await.unwrap();
        fs::write(base.join("src/utils/helpers.rs"), "pub fn help() {}").await.unwrap();
        fs::write(base.join("src/utils/config.rs"), "pub fn cfg() {}").await.unwrap();
        fs::write(base.join("tests/test_main.rs"), "#[test] fn t() {}").await.unwrap();
        fs::write(base.join(".git/config"), "[core]").await.unwrap();
        fs::write(base.join("node_modules/pkg/index.js"), "module.exports = {}").await.unwrap();

        dir
    }

    #[tokio::test]
    async fn test_glob_star_pattern() {
        let dir = setup_test_dir().await;
        let tool = GlobTool::new().with_base_dir(dir.path());

        let out = tool.execute(json!({ "pattern": "*.rs" })).await.unwrap();
        assert!(out.success);
        // No .rs files at root level
        assert_eq!(out.result["total"], 0);
    }

    #[tokio::test]
    async fn test_glob_double_star() {
        let dir = setup_test_dir().await;
        let tool = GlobTool::new().with_base_dir(dir.path());

        let out = tool.execute(json!({ "pattern": "**/*.rs" })).await.unwrap();
        assert!(out.success);
        let files = out.result["files"].as_array().unwrap();
        // Should find: src/main.rs, src/lib.rs, src/utils/helpers.rs, src/utils/config.rs, tests/test_main.rs
        assert_eq!(files.len(), 5);
    }

    #[tokio::test]
    async fn test_glob_brace_expansion() {
        let dir = setup_test_dir().await;
        let tool = GlobTool::new().with_base_dir(dir.path());

        let out = tool
            .execute(json!({ "pattern": "src/{main,lib}.rs" }))
            .await
            .unwrap();
        assert!(out.success);
        let files = out.result["files"].as_array().unwrap();
        assert_eq!(files.len(), 2);
    }

    #[tokio::test]
    async fn test_glob_sorted_by_mtime() {
        let dir = setup_test_dir().await;
        let tool = GlobTool::new().with_base_dir(dir.path());

        // Write files with slight delay to ensure different mtimes
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        fs::write(dir.path().join("src/main.rs"), "fn main() { /* updated */ }")
            .await
            .unwrap();

        let out = tool.execute(json!({ "pattern": "src/*.rs" })).await.unwrap();
        assert!(out.success);
        let files = out.result["files"].as_array().unwrap();
        assert_eq!(files.len(), 2);
        // Newest file should be first (main.rs was just updated)
        let first = files[0].as_str().unwrap();
        assert!(first.contains("main.rs"));
    }

    #[tokio::test]
    async fn test_glob_skip_hidden_dirs() {
        let dir = setup_test_dir().await;
        let tool = GlobTool::new().with_base_dir(dir.path());

        let out = tool.execute(json!({ "pattern": "**/*" })).await.unwrap();
        assert!(out.success);
        let files = out.result["files"].as_array().unwrap();
        // Should not include .git/config or node_modules/pkg/index.js
        for f in files {
            let path = f.as_str().unwrap();
            assert!(!path.contains(".git/"), "Should skip .git: {}", path);
            assert!(
                !path.contains("node_modules/"),
                "Should skip node_modules: {}",
                path
            );
        }
    }

    #[tokio::test]
    async fn test_glob_empty_results() {
        let dir = setup_test_dir().await;
        let tool = GlobTool::new().with_base_dir(dir.path());

        let out = tool.execute(json!({ "pattern": "**/*.py" })).await.unwrap();
        assert!(out.success);
        assert_eq!(out.result["total"], 0);
        assert!(out.result["files"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_glob_with_path_prefix() {
        let dir = setup_test_dir().await;
        let tool = GlobTool::new().with_base_dir(dir.path());

        let out = tool
            .execute(json!({ "pattern": "src/utils/**/*.rs" }))
            .await
            .unwrap();
        assert!(out.success);
        let files = out.result["files"].as_array().unwrap();
        assert_eq!(files.len(), 2); // helpers.rs and config.rs
    }

    #[tokio::test]
    async fn test_glob_nonexistent_dir() {
        let tool = GlobTool::new().with_base_dir("/nonexistent_path_xyz");

        let out = tool.execute(json!({ "pattern": "**/*.rs" })).await.unwrap();
        assert!(!out.success);
    }
}
