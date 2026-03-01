//! Grep-like content search tool for AI agents.
//!
//! Provides powerful content search with:
//! - Full regex syntax via the `regex` crate
//! - Context lines around matches (-A/-B/-C)
//! - Multiple output modes (content, files_with_matches, count)
//! - File type and glob filtering
//! - Case-insensitive and multiline matching
//! - Pagination (head_limit, offset)

use async_trait::async_trait;
use regex::{Regex, RegexBuilder};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tokio::fs;

use super::shared::{is_likely_binary, should_skip_grep_dir};
use crate::Result;
use crate::{Tool, ToolOutput};

/// Maximum total matches to collect before stopping
const MAX_TOTAL_MATCHES: usize = 5000;

/// Maximum file size to search (5 MB)
const MAX_FILE_SIZE: u64 = 5 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct GrepInput {
    pattern: String,
    path: Option<String>,
    glob: Option<String>,
    #[serde(rename = "type")]
    file_type: Option<String>,
    output_mode: Option<String>,
    #[serde(rename = "-A")]
    after_context: Option<usize>,
    #[serde(rename = "-B")]
    before_context: Option<usize>,
    #[serde(rename = "-C")]
    context: Option<usize>,
    #[serde(rename = "-i")]
    case_insensitive: Option<bool>,
    #[serde(rename = "-n")]
    show_line_numbers: Option<bool>,
    multiline: Option<bool>,
    head_limit: Option<usize>,
    offset: Option<usize>,
}

/// A single match with context
struct MatchResult {
    file: String,
    line_number: usize,
    line: String,
    before: Vec<(usize, String)>,
    after: Vec<(usize, String)>,
}

/// Grep-like content search tool.
#[derive(Clone)]
pub struct GrepTool {
    base_dir: Option<PathBuf>,
}

impl Default for GrepTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GrepTool {
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
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Search file contents using regex patterns. Supports context lines, output modes (content/files_with_matches/count), file type filtering, case-insensitive and multiline matching."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "description": "Regex pattern to search for",
                    "type": "string"
                },
                "path": {
                    "description": "File or directory to search in. Defaults to current working directory.",
                    "type": "string"
                },
                "glob": {
                    "description": "Glob pattern to filter files (e.g. \"*.rs\", \"*.{ts,tsx}\")",
                    "type": "string"
                },
                "type": {
                    "description": "File type filter (e.g. \"rust\", \"js\", \"py\", \"go\", \"java\")",
                    "type": "string"
                },
                "output_mode": {
                    "description": "Output mode: \"content\" (matching lines with context), \"files_with_matches\" (file paths only), \"count\" (match counts per file)",
                    "type": "string",
                    "enum": ["content", "files_with_matches", "count"],
                    "default": "content"
                },
                "-A": {
                    "description": "Number of lines to show after each match",
                    "type": "integer",
                    "minimum": 0
                },
                "-B": {
                    "description": "Number of lines to show before each match",
                    "type": "integer",
                    "minimum": 0
                },
                "-C": {
                    "description": "Number of context lines before and after each match",
                    "type": "integer",
                    "minimum": 0
                },
                "-i": {
                    "description": "Case insensitive search",
                    "type": "boolean"
                },
                "-n": {
                    "description": "Show line numbers (default: true for content mode)",
                    "type": "boolean",
                    "default": true
                },
                "multiline": {
                    "description": "Enable multiline mode where . matches newlines",
                    "type": "boolean"
                },
                "head_limit": {
                    "description": "Maximum number of results to return",
                    "type": "integer",
                    "minimum": 1
                },
                "offset": {
                    "description": "Skip first N results before applying head_limit",
                    "type": "integer",
                    "minimum": 0
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: GrepInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(err) => return Ok(ToolOutput::error(format!("Invalid input: {}", err))),
        };

        let case_insensitive = params.case_insensitive.unwrap_or(false);
        let multiline = params.multiline.unwrap_or(false);
        let show_line_numbers = params.show_line_numbers.unwrap_or(true);
        let output_mode = params.output_mode.as_deref().unwrap_or("content");
        let offset = params.offset.unwrap_or(0);
        let head_limit = params.head_limit.unwrap_or(0); // 0 = unlimited

        // Context lines: -C overrides -A/-B
        let before_ctx = params.context.or(params.before_context).unwrap_or(0);
        let after_ctx = params.context.or(params.after_context).unwrap_or(0);

        // Build regex
        let regex = match RegexBuilder::new(&params.pattern)
            .case_insensitive(case_insensitive)
            .dot_matches_new_line(multiline)
            .multi_line(multiline)
            .build()
        {
            Ok(r) => r,
            Err(err) => return Ok(ToolOutput::error(format!("Invalid regex: {}", err))),
        };

        let base = self.resolve_base(params.path.as_deref());

        // If path is a file, search just that file
        if base.is_file() {
            let search_opts = SearchOpts {
                output_mode,
                before_ctx,
                after_ctx,
                show_line_numbers,
                offset,
                head_limit,
            };
            return search_single_file(&base, &regex, &search_opts).await;
        }

        if !base.is_dir() {
            return Ok(ToolOutput::error(format!(
                "Path not found: {}",
                base.display()
            )));
        }

        // Collect all files
        let mut files = Vec::new();
        collect_files(&base, &params.glob, &params.file_type, &mut files).await;

        match output_mode {
            "files_with_matches" => {
                let mut matching_files = Vec::new();
                for file_path in &files {
                    if let Ok(content) = fs::read_to_string(file_path).await
                        && regex.is_match(&content)
                    {
                        matching_files.push(file_path.to_string_lossy().to_string());
                    }
                }

                let total = matching_files.len();
                let matching_files: Vec<_> = matching_files
                    .into_iter()
                    .skip(offset)
                    .take(if head_limit > 0 {
                        head_limit
                    } else {
                        usize::MAX
                    })
                    .collect();

                Ok(ToolOutput::success(json!({
                    "files": matching_files,
                    "total": total
                })))
            }
            "count" => {
                let mut counts: Vec<Value> = Vec::new();
                for file_path in &files {
                    if let Ok(content) = fs::read_to_string(file_path).await {
                        let count = regex.find_iter(&content).count();
                        if count > 0 {
                            counts.push(json!({
                                "file": file_path.to_string_lossy(),
                                "count": count
                            }));
                        }
                    }
                }

                let total = counts.len();
                let counts: Vec<_> = counts
                    .into_iter()
                    .skip(offset)
                    .take(if head_limit > 0 {
                        head_limit
                    } else {
                        usize::MAX
                    })
                    .collect();

                Ok(ToolOutput::success(json!({
                    "counts": counts,
                    "total": total
                })))
            }
            _ => {
                // "content" mode — collect matches with context
                let mut all_matches: Vec<MatchResult> = Vec::new();

                for file_path in &files {
                    if all_matches.len() >= MAX_TOTAL_MATCHES {
                        break;
                    }

                    let content = match fs::read_to_string(file_path).await {
                        Ok(c) => c,
                        Err(_) => continue,
                    };

                    let lines: Vec<&str> = content.lines().collect();
                    let file_str = file_path.to_string_lossy().to_string();

                    for (i, line) in lines.iter().enumerate() {
                        if regex.is_match(line) {
                            let before: Vec<(usize, String)> = if before_ctx > 0 {
                                let start = i.saturating_sub(before_ctx);
                                (start..i).map(|j| (j + 1, lines[j].to_string())).collect()
                            } else {
                                Vec::new()
                            };

                            let after: Vec<(usize, String)> = if after_ctx > 0 {
                                let end = (i + 1 + after_ctx).min(lines.len());
                                ((i + 1)..end)
                                    .map(|j| (j + 1, lines[j].to_string()))
                                    .collect()
                            } else {
                                Vec::new()
                            };

                            all_matches.push(MatchResult {
                                file: file_str.clone(),
                                line_number: i + 1,
                                line: line.to_string(),
                                before,
                                after,
                            });

                            if all_matches.len() >= MAX_TOTAL_MATCHES {
                                break;
                            }
                        }
                    }
                }

                let total = all_matches.len();
                let matches: Vec<_> = all_matches
                    .into_iter()
                    .skip(offset)
                    .take(if head_limit > 0 {
                        head_limit
                    } else {
                        usize::MAX
                    })
                    .collect();

                // Format output as text
                let mut output = String::new();
                let mut last_file = String::new();

                for m in &matches {
                    if m.file != last_file {
                        if !output.is_empty() {
                            output.push('\n');
                        }
                        output.push_str(&m.file);
                        output.push('\n');
                        last_file.clone_from(&m.file);
                    }

                    // Before context
                    for (ln, text) in &m.before {
                        if show_line_numbers {
                            output.push_str(&format!("{}-{}\n", ln, text));
                        } else {
                            output.push_str(&format!("-{}\n", text));
                        }
                    }

                    // Match line
                    if show_line_numbers {
                        output.push_str(&format!("{}:{}\n", m.line_number, m.line));
                    } else {
                        output.push_str(&format!(":{}\n", m.line));
                    }

                    // After context
                    for (ln, text) in &m.after {
                        if show_line_numbers {
                            output.push_str(&format!("{}-{}\n", ln, text));
                        } else {
                            output.push_str(&format!("-{}\n", text));
                        }
                    }

                    if !m.before.is_empty() || !m.after.is_empty() {
                        output.push_str("--\n");
                    }
                }

                Ok(ToolOutput::success(json!({
                    "output": output.trim_end(),
                    "match_count": total
                })))
            }
        }
    }
}

/// Options for single-file search
struct SearchOpts<'a> {
    output_mode: &'a str,
    before_ctx: usize,
    after_ctx: usize,
    show_line_numbers: bool,
    offset: usize,
    head_limit: usize,
}

/// Search a single file and return results.
async fn search_single_file(
    path: &Path,
    regex: &Regex,
    opts: &SearchOpts<'_>,
) -> Result<ToolOutput> {
    let content = match fs::read_to_string(path).await {
        Ok(c) => c,
        Err(err) => return Ok(ToolOutput::error(format!("Cannot read file: {}", err))),
    };

    let file_str = path.to_string_lossy().to_string();

    match opts.output_mode {
        "files_with_matches" => {
            if regex.is_match(&content) {
                Ok(ToolOutput::success(json!({
                    "files": [file_str],
                    "total": 1
                })))
            } else {
                Ok(ToolOutput::success(json!({
                    "files": [],
                    "total": 0
                })))
            }
        }
        "count" => {
            let count = regex.find_iter(&content).count();
            if count > 0 {
                Ok(ToolOutput::success(json!({
                    "counts": [{ "file": file_str, "count": count }],
                    "total": 1
                })))
            } else {
                Ok(ToolOutput::success(json!({
                    "counts": [],
                    "total": 0
                })))
            }
        }
        _ => {
            let lines: Vec<&str> = content.lines().collect();
            let mut matches: Vec<MatchResult> = Vec::new();

            for (i, line) in lines.iter().enumerate() {
                if regex.is_match(line) {
                    let before: Vec<(usize, String)> = if opts.before_ctx > 0 {
                        let start = i.saturating_sub(opts.before_ctx);
                        (start..i).map(|j| (j + 1, lines[j].to_string())).collect()
                    } else {
                        Vec::new()
                    };

                    let after: Vec<(usize, String)> = if opts.after_ctx > 0 {
                        let end = (i + 1 + opts.after_ctx).min(lines.len());
                        ((i + 1)..end)
                            .map(|j| (j + 1, lines[j].to_string()))
                            .collect()
                    } else {
                        Vec::new()
                    };

                    matches.push(MatchResult {
                        file: file_str.clone(),
                        line_number: i + 1,
                        line: line.to_string(),
                        before,
                        after,
                    });
                }
            }

            let total = matches.len();
            let matches: Vec<_> = matches
                .into_iter()
                .skip(opts.offset)
                .take(if opts.head_limit > 0 {
                    opts.head_limit
                } else {
                    usize::MAX
                })
                .collect();

            let mut output = String::new();
            if !matches.is_empty() {
                output.push_str(&file_str);
                output.push('\n');
            }

            for m in &matches {
                for (ln, text) in &m.before {
                    if opts.show_line_numbers {
                        output.push_str(&format!("{}-{}\n", ln, text));
                    } else {
                        output.push_str(&format!("-{}\n", text));
                    }
                }

                if opts.show_line_numbers {
                    output.push_str(&format!("{}:{}\n", m.line_number, m.line));
                } else {
                    output.push_str(&format!(":{}\n", m.line));
                }

                for (ln, text) in &m.after {
                    if opts.show_line_numbers {
                        output.push_str(&format!("{}-{}\n", ln, text));
                    } else {
                        output.push_str(&format!("-{}\n", text));
                    }
                }

                if !m.before.is_empty() || !m.after.is_empty() {
                    output.push_str("--\n");
                }
            }

            Ok(ToolOutput::success(json!({
                "output": output.trim_end(),
                "match_count": total
            })))
        }
    }
}

/// Recursively collect files matching optional glob and type filters.
#[async_recursion::async_recursion]
async fn collect_files(
    dir: &Path,
    glob_filter: &Option<String>,
    type_filter: &Option<String>,
    files: &mut Vec<PathBuf>,
) {
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
        if path.is_dir() {
            if should_skip_grep_dir(&file_name) {
                continue;
            }
            collect_files(&path, glob_filter, type_filter, files).await;
            continue;
        }

        // Skip binary files
        if is_likely_binary(&file_name) {
            continue;
        }

        // Check file size
        if let Ok(meta) = entry.metadata().await
            && meta.len() > MAX_FILE_SIZE
        {
            continue;
        }

        // Apply type filter
        if let Some(ft) = type_filter {
            let exts = extensions_for_type(ft);
            if !exts.is_empty() {
                let has_ext = exts
                    .iter()
                    .any(|ext| file_name.ends_with(&format!(".{}", ext)));
                if !has_ext {
                    continue;
                }
            }
        }

        // Apply glob filter
        if let Some(glob_pat) = glob_filter
            && !glob_match::glob_match(glob_pat, &file_name)
        {
            continue;
        }

        files.push(path);
    }
}

/// Map file type names to file extensions.
fn extensions_for_type(file_type: &str) -> &[&str] {
    match file_type {
        "rust" | "rs" => &["rs"],
        "js" | "javascript" => &["js", "jsx", "mjs"],
        "ts" | "typescript" => &["ts", "tsx", "mts"],
        "py" | "python" => &["py", "pyi"],
        "go" => &["go"],
        "java" => &["java"],
        "c" => &["c", "h"],
        "cpp" | "c++" => &["cpp", "hpp", "cc", "hh", "cxx"],
        "ruby" | "rb" => &["rb"],
        "swift" => &["swift"],
        "kotlin" | "kt" => &["kt", "kts"],
        "scala" => &["scala"],
        "php" => &["php"],
        "html" => &["html", "htm"],
        "css" => &["css"],
        "json" => &["json"],
        "yaml" | "yml" => &["yaml", "yml"],
        "toml" => &["toml"],
        "xml" => &["xml"],
        "sql" => &["sql"],
        "sh" | "shell" | "bash" => &["sh", "bash", "zsh"],
        "md" | "markdown" => &["md", "markdown"],
        "vue" => &["vue"],
        "svelte" => &["svelte"],
        _ => &[],
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

        fs::create_dir_all(base.join("src")).await.unwrap();
        fs::create_dir_all(base.join("tests")).await.unwrap();

        fs::write(
            base.join("src/main.rs"),
            "fn main() {\n    println!(\"Hello, world!\");\n}\n",
        )
        .await
        .unwrap();
        fs::write(
            base.join("src/lib.rs"),
            "pub fn greet(name: &str) -> String {\n    format!(\"Hello, {}!\", name)\n}\n",
        )
        .await
        .unwrap();
        fs::write(
            base.join("tests/test.rs"),
            "#[test]\nfn test_greet() {\n    assert_eq!(greet(\"world\"), \"Hello, world!\");\n}\n",
        )
        .await
        .unwrap();
        fs::write(base.join("src/config.json"), "{\"key\": \"value\"}\n")
            .await
            .unwrap();

        dir
    }

    #[tokio::test]
    async fn test_grep_basic_match() {
        let dir = setup_test_dir().await;
        let tool = GrepTool::new().with_base_dir(dir.path());

        let out = tool.execute(json!({ "pattern": "Hello" })).await.unwrap();
        assert!(out.success);
        assert!(out.result["match_count"].as_u64().unwrap() > 0);
        let text = out.result["output"].as_str().unwrap();
        assert!(text.contains("Hello"));
    }

    #[tokio::test]
    async fn test_grep_context_lines() {
        let dir = setup_test_dir().await;
        let tool = GrepTool::new().with_base_dir(dir.path());

        let out = tool
            .execute(json!({
                "pattern": "println",
                "-B": 1,
                "-A": 1
            }))
            .await
            .unwrap();
        assert!(out.success);
        let text = out.result["output"].as_str().unwrap();
        // Should contain context lines around "println"
        assert!(text.contains("fn main()"));
    }

    #[tokio::test]
    async fn test_grep_case_insensitive() {
        let dir = setup_test_dir().await;
        let tool = GrepTool::new().with_base_dir(dir.path());

        let out = tool
            .execute(json!({
                "pattern": "hello",
                "-i": true
            }))
            .await
            .unwrap();
        assert!(out.success);
        assert!(out.result["match_count"].as_u64().unwrap() > 0);
    }

    #[tokio::test]
    async fn test_grep_files_with_matches_mode() {
        let dir = setup_test_dir().await;
        let tool = GrepTool::new().with_base_dir(dir.path());

        let out = tool
            .execute(json!({
                "pattern": "fn",
                "output_mode": "files_with_matches"
            }))
            .await
            .unwrap();
        assert!(out.success);
        let files = out.result["files"].as_array().unwrap();
        // main.rs, lib.rs, and test.rs all contain "fn"
        assert!(files.len() >= 2);
    }

    #[tokio::test]
    async fn test_grep_count_mode() {
        let dir = setup_test_dir().await;
        let tool = GrepTool::new().with_base_dir(dir.path());

        let out = tool
            .execute(json!({
                "pattern": "fn",
                "output_mode": "count"
            }))
            .await
            .unwrap();
        assert!(out.success);
        let counts = out.result["counts"].as_array().unwrap();
        assert!(!counts.is_empty());
        for c in counts {
            assert!(c["count"].as_u64().unwrap() > 0);
        }
    }

    #[tokio::test]
    async fn test_grep_file_type_filter() {
        let dir = setup_test_dir().await;
        let tool = GrepTool::new().with_base_dir(dir.path());

        let out = tool
            .execute(json!({
                "pattern": "key",
                "type": "json",
                "output_mode": "files_with_matches"
            }))
            .await
            .unwrap();
        assert!(out.success);
        let files = out.result["files"].as_array().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].as_str().unwrap().ends_with("config.json"));
    }

    #[tokio::test]
    async fn test_grep_head_limit_offset() {
        let dir = setup_test_dir().await;
        let tool = GrepTool::new().with_base_dir(dir.path());

        let out = tool
            .execute(json!({
                "pattern": "fn",
                "head_limit": 1
            }))
            .await
            .unwrap();
        assert!(out.success);
        // Total match count should be > 1, but output limited to 1
        let total = out.result["match_count"].as_u64().unwrap();
        assert!(total >= 1);
        let text = out.result["output"].as_str().unwrap();
        // Only one file:line match should appear
        let match_lines: Vec<_> = text.lines().filter(|l| l.contains(':')).collect();
        assert_eq!(match_lines.len(), 1);
    }

    #[tokio::test]
    async fn test_grep_multiline() {
        let dir = setup_test_dir().await;
        let tool = GrepTool::new().with_base_dir(dir.path());

        // Without multiline, this should still work line-by-line
        let out = tool
            .execute(json!({
                "pattern": "pub fn",
                "output_mode": "files_with_matches"
            }))
            .await
            .unwrap();
        assert!(out.success);
        let files = out.result["files"].as_array().unwrap();
        assert!(files.iter().any(|f| f.as_str().unwrap().contains("lib.rs")));
    }

    #[tokio::test]
    async fn test_grep_skip_binary() {
        let dir = setup_test_dir().await;
        // Create a binary file
        fs::write(dir.path().join("image.png"), b"\x89PNG\r\n\x1a\n")
            .await
            .unwrap();

        let tool = GrepTool::new().with_base_dir(dir.path());

        let out = tool
            .execute(json!({
                "pattern": "PNG",
                "output_mode": "files_with_matches"
            }))
            .await
            .unwrap();
        assert!(out.success);
        let files = out.result["files"].as_array().unwrap();
        // png should be skipped
        assert!(!files.iter().any(|f| f.as_str().unwrap().contains(".png")));
    }

    #[tokio::test]
    async fn test_grep_glob_filter() {
        let dir = setup_test_dir().await;
        let tool = GrepTool::new().with_base_dir(dir.path());

        let out = tool
            .execute(json!({
                "pattern": "fn",
                "glob": "*.rs",
                "output_mode": "files_with_matches"
            }))
            .await
            .unwrap();
        assert!(out.success);
        let files = out.result["files"].as_array().unwrap();
        for f in files {
            assert!(f.as_str().unwrap().ends_with(".rs"));
        }
    }

    #[tokio::test]
    async fn test_grep_no_matches() {
        let dir = setup_test_dir().await;
        let tool = GrepTool::new().with_base_dir(dir.path());

        let out = tool
            .execute(json!({ "pattern": "NONEXISTENT_STRING_XYZ" }))
            .await
            .unwrap();
        assert!(out.success);
        assert_eq!(out.result["match_count"], 0);
    }
}
