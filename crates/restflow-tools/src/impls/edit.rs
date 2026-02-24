//! Edit tool for precise string replacement in files.
//!
//! Provides old_string/new_string replacement with 3-level fallback matching
//! and optional post-edit LSP diagnostics.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::fs;

use super::file_tracker::FileTracker;
use crate::{Result, Tool, ToolOutput};
use restflow_traits::cache::AgentCache;
use restflow_traits::store::DiagnosticsProvider;

/// Maximum number of LSP diagnostic errors to include in output.
const MAX_DIAG_ERRORS: usize = 20;

/// Timeout for waiting on LSP diagnostics after an edit.
const LSP_TIMEOUT: Duration = Duration::from_secs(3);

// ── Error types ─────────────────────────────────────────────────────

#[derive(Debug)]
pub enum EditError {
    IdenticalStrings,
    NotFound,
    MultipleMatches { count: usize },
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IdenticalStrings => write!(f, "old_string and new_string are identical"),
            Self::NotFound => write!(
                f,
                "old_string not found in file. Make sure the string matches exactly, including whitespace and indentation."
            ),
            Self::MultipleMatches { count } => write!(
                f,
                "old_string matched {count} locations. Provide more surrounding context to make the match unique, or set replace_all to true."
            ),
        }
    }
}

// ── Replacement engine ──────────────────────────────────────────────

/// 3-level fallback replacement.
///
/// 1. Exact match
/// 2. Line-level whitespace normalization (trim each line before compare)
/// 3. Indentation-flexible (strip minimum common indent before compare)
///
/// Returns the replaced content or an error.
pub fn replace(
    content: &str,
    old: &str,
    new: &str,
    replace_all: bool,
) -> std::result::Result<String, EditError> {
    if old == new {
        return Err(EditError::IdenticalStrings);
    }

    // Level 1: exact match
    if let Some(result) = try_exact(content, old, new, replace_all) {
        return result;
    }

    // Level 2: whitespace-normalized
    if let Some(result) = try_whitespace_normalized(content, old, new, replace_all) {
        return result;
    }

    // Level 3: indentation-flexible
    if let Some(result) = try_indent_flexible(content, old, new, replace_all) {
        return result;
    }

    Err(EditError::NotFound)
}

/// Level 1: exact substring match.
fn try_exact(
    content: &str,
    old: &str,
    new: &str,
    replace_all: bool,
) -> Option<std::result::Result<String, EditError>> {
    let first = content.find(old)?;

    if replace_all {
        return Some(Ok(content.replace(old, new)));
    }

    // Uniqueness check
    if content[first + 1..].contains(old) {
        let count = content.matches(old).count();
        return Some(Err(EditError::MultipleMatches { count }));
    }

    let mut result = String::with_capacity(content.len() - old.len() + new.len());
    result.push_str(&content[..first]);
    result.push_str(new);
    result.push_str(&content[first + old.len()..]);
    Some(Ok(result))
}

/// Level 2: compare lines after trimming trailing whitespace on each line.
fn try_whitespace_normalized(
    content: &str,
    old: &str,
    new: &str,
    replace_all: bool,
) -> Option<std::result::Result<String, EditError>> {
    let old_lines: Vec<&str> = old.lines().collect();
    if old_lines.is_empty() {
        return None;
    }

    let content_lines: Vec<&str> = content.lines().collect();
    let mut match_positions = Vec::new();

    'outer: for i in 0..content_lines.len().saturating_sub(old_lines.len() - 1) {
        for (j, old_line) in old_lines.iter().enumerate() {
            if content_lines[i + j].trim_end() != old_line.trim_end() {
                continue 'outer;
            }
        }
        match_positions.push(i);
    }

    if match_positions.is_empty() {
        return None;
    }

    if !replace_all && match_positions.len() > 1 {
        return Some(Err(EditError::MultipleMatches {
            count: match_positions.len(),
        }));
    }

    // Apply replacement on the original content using byte offsets
    Some(Ok(apply_line_replacements(
        content,
        &content_lines,
        &old_lines,
        new,
        &match_positions,
    )))
}

/// Level 3: strip minimum common indentation from both old_string and content
/// window before comparing.
fn try_indent_flexible(
    content: &str,
    old: &str,
    new: &str,
    replace_all: bool,
) -> Option<std::result::Result<String, EditError>> {
    let old_lines: Vec<&str> = old.lines().collect();
    if old_lines.is_empty() {
        return None;
    }

    let old_deindented = deindent_lines(&old_lines);
    let content_lines: Vec<&str> = content.lines().collect();
    let mut match_positions = Vec::new();

    for i in 0..content_lines.len().saturating_sub(old_lines.len() - 1) {
        let window = &content_lines[i..i + old_lines.len()];
        let window_deindented = deindent_lines(window);
        if window_deindented == old_deindented {
            match_positions.push(i);
        }
    }

    if match_positions.is_empty() {
        return None;
    }

    if !replace_all && match_positions.len() > 1 {
        return Some(Err(EditError::MultipleMatches {
            count: match_positions.len(),
        }));
    }

    Some(Ok(apply_line_replacements(
        content,
        &content_lines,
        &old_lines,
        new,
        &match_positions,
    )))
}

/// Remove the minimum common leading whitespace from a slice of lines.
fn deindent_lines(lines: &[&str]) -> Vec<String> {
    let min_indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);

    lines
        .iter()
        .map(|l| {
            if l.len() >= min_indent {
                l[min_indent..].to_string()
            } else {
                l.to_string()
            }
        })
        .collect()
}

/// Replace matched line ranges in the original content, preserving byte-level
/// accuracy for unmatched regions.
fn apply_line_replacements(
    content: &str,
    content_lines: &[&str],
    old_lines: &[&str],
    new: &str,
    positions: &[usize],
) -> String {
    let old_line_count = old_lines.len();

    // Build byte offset map: line_index -> byte start in `content`
    let mut line_offsets: Vec<usize> = Vec::with_capacity(content_lines.len() + 1);
    let mut offset = 0;
    for line in content_lines {
        line_offsets.push(offset);
        offset += line.len();
        // Account for the newline character if present
        if offset < content.len() {
            offset += if content.as_bytes().get(offset) == Some(&b'\r') {
                if content.as_bytes().get(offset + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                }
            } else {
                1
            };
        }
    }
    line_offsets.push(content.len());

    let mut result = String::with_capacity(content.len());
    let mut last_end = 0;

    for &pos in positions {
        let match_start = line_offsets[pos];
        let match_end = line_offsets[pos + old_line_count];

        result.push_str(&content[last_end..match_start]);
        result.push_str(new);

        // If the matched range ended before EOF and replacement doesn't end
        // with newline but original did, preserve it
        if match_end > match_start
            && !new.ends_with('\n')
            && match_end <= content.len()
            && content[match_start..match_end].ends_with('\n')
        {
            result.push('\n');
        }

        last_end = match_end;
    }

    result.push_str(&content[last_end..]);
    result
}

// ── Tool implementation ─────────────────────────────────────────────

#[derive(Clone)]
pub struct EditTool {
    base_dir: Option<PathBuf>,
    tracker: Arc<FileTracker>,
    diagnostics: Option<Arc<dyn DiagnosticsProvider>>,
    cache_manager: Option<Arc<dyn AgentCache>>,
}

impl EditTool {
    pub fn with_tracker(tracker: Arc<FileTracker>) -> Self {
        Self {
            base_dir: None,
            tracker,
            diagnostics: None,
            cache_manager: None,
        }
    }

    pub fn with_base_dir(mut self, base_dir: impl Into<PathBuf>) -> Self {
        self.base_dir = Some(base_dir.into());
        self
    }

    pub fn with_diagnostics_provider(mut self, provider: Arc<dyn DiagnosticsProvider>) -> Self {
        self.diagnostics = Some(provider);
        self
    }

    pub fn with_cache_manager(mut self, cache: Arc<dyn AgentCache>) -> Self {
        self.cache_manager = Some(cache);
        self
    }

    fn resolve_path(&self, path: &str) -> std::result::Result<PathBuf, String> {
        let path = PathBuf::from(path);

        if let Some(base) = &self.base_dir {
            let resolved = if path.is_absolute() {
                path
            } else {
                base.join(&path)
            };

            if resolved.exists() {
                let canonical = resolved.canonicalize().map_err(|e| e.to_string())?;
                let canonical_base = base.canonicalize().map_err(|e| e.to_string())?;
                if !canonical.starts_with(&canonical_base) {
                    return Err(format!(
                        "Path '{}' escapes allowed base directory '{}'.",
                        canonical.display(),
                        canonical_base.display()
                    ));
                }
                return Ok(canonical);
            }

            let canonical_base = if base.exists() {
                base.canonicalize().map_err(|e| e.to_string())?
            } else {
                normalize_path(base)
            };
            let normalized = normalize_path(&resolved);
            if !normalized.starts_with(&canonical_base) {
                return Err(format!(
                    "Path '{}' escapes allowed base directory '{}'.",
                    normalized.display(),
                    canonical_base.display()
                ));
            }
            Ok(normalized)
        } else if path.is_absolute() {
            Ok(path)
        } else {
            let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
            Ok(cwd.join(path))
        }
    }

    /// Invalidate caches for the given path and its parent directories.
    async fn invalidate_caches(&self, path: &Path) {
        if let Some(cache) = &self.cache_manager {
            cache.invalidate_file(path).await;
            let mut current = path.parent();
            while let Some(directory) = current {
                cache
                    .invalidate_search_dir(&directory.to_string_lossy())
                    .await;
                current = directory.parent();
            }
        }
    }

    /// Notify LSP and wait for diagnostics, returning error-only entries.
    async fn run_diagnostics(&self, path: &Path) -> Option<String> {
        let provider = self.diagnostics.as_ref()?;

        if provider.ensure_open(path).await.is_err() {
            return None;
        }

        if let Ok(content) = fs::read_to_string(path).await {
            let _ = provider.did_change(path, &content).await;
        }

        let diags = match provider.wait_for_diagnostics(path, LSP_TIMEOUT).await {
            Ok(d) => d,
            Err(_) => return None,
        };

        let errors: Vec<String> = diags
            .iter()
            .filter(|d| {
                matches!(
                    d.severity,
                    Some(lsp_types::DiagnosticSeverity::ERROR) | None
                )
            })
            .take(MAX_DIAG_ERRORS)
            .map(|d| {
                format!(
                    "ERROR [{}:{}] {}",
                    d.range.start.line + 1,
                    d.range.start.character + 1,
                    d.message
                )
            })
            .collect();

        if errors.is_empty() {
            return None;
        }

        let path_str = path.display();
        let mut output = format!("\nLSP errors detected:\n<diagnostics file=\"{path_str}\">\n");
        for line in &errors {
            output.push_str(line);
            output.push('\n');
        }
        output.push_str("</diagnostics>");
        Some(output)
    }
}

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }

    fn description(&self) -> &str {
        "Make precise text replacements in a file using old_string/new_string matching. \
         Supports exact match, whitespace-normalized match, and indentation-flexible match. \
         Use replace_all to replace all occurrences."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file to edit"
                },
                "old_string": {
                    "type": "string",
                    "description": "The exact text to find and replace"
                },
                "new_string": {
                    "type": "string",
                    "description": "The replacement text"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "Replace all occurrences (default: false)",
                    "default": false
                }
            },
            "required": ["file_path", "old_string", "new_string"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::ToolError::Tool("Missing 'file_path' argument".into()))?;
        let old_string = args
            .get("old_string")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::ToolError::Tool("Missing 'old_string' argument".into()))?;
        let new_string = args
            .get("new_string")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::ToolError::Tool("Missing 'new_string' argument".into()))?;
        let replace_all = args
            .get("replace_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Resolve path
        let path = match self.resolve_path(file_path) {
            Ok(p) => p,
            Err(e) => return Ok(ToolOutput::error(e)),
        };

        // Guard: file must exist
        if !path.exists() {
            return Ok(ToolOutput::error(format!(
                "File not found: {}",
                path.display()
            )));
        }

        // Guard: must have been read first
        if !self.tracker.has_been_read(&path) {
            return Ok(ToolOutput::error(format!(
                "You must read {} before editing it. Read the file first to understand its current content.",
                path.display()
            )));
        }

        // Guard: external modification
        match self.tracker.check_external_modification(&path).await {
            Ok(true) => {
                return Ok(ToolOutput::error(format!(
                    "File {} has been modified externally since it was read. Read it again before editing.",
                    path.display()
                )));
            }
            Ok(false) => {}
            Err(e) => {
                return Ok(ToolOutput::error(format!(
                    "Cannot check file modification time: {e}"
                )));
            }
        }

        // Read current content
        let content = match fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => {
                return Ok(ToolOutput::error(format!("Cannot read file: {e}")));
            }
        };

        // Apply replacement
        let new_content = match replace(&content, old_string, new_string, replace_all) {
            Ok(c) => c,
            Err(e) => return Ok(ToolOutput::error(e.to_string())),
        };

        // Count changed lines for summary
        let old_line_count = content.lines().count();
        let new_line_count = new_content.lines().count();
        let lines_changed = new_line_count.abs_diff(old_line_count);

        // Write back
        if let Err(e) = fs::write(&path, &new_content).await {
            return Ok(ToolOutput::error(format!("Cannot write file: {e}")));
        }

        self.tracker.record_write(&path);
        self.invalidate_caches(&path).await;

        // Build output message
        let mut msg = format!(
            "Edit applied to {} ({} lines changed)",
            path.display(),
            lines_changed
        );

        // LSP diagnostics (synchronous wait)
        if let Some(diag_output) = self.run_diagnostics(&path).await {
            msg.push_str(&diag_output);
        }

        Ok(ToolOutput::success(json!({
            "message": msg,
            "path": path.display().to_string(),
            "lines_changed": lines_changed,
        })))
    }
}

/// Normalize a path without canonicalizing (for non-existent paths).
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── Replacer pure-function tests ────────────────────────────────

    #[test]
    fn test_exact_match_unique() {
        let content = "fn main() {\n    println!(\"hello\");\n}\n";
        let result = replace(content, "println!(\"hello\")", "println!(\"world\")", false);
        assert_eq!(
            result.unwrap(),
            "fn main() {\n    println!(\"world\");\n}\n"
        );
    }

    #[test]
    fn test_exact_match_not_found() {
        let content = "fn main() {}";
        let result = replace(content, "nonexistent", "replacement", false);
        assert!(matches!(result.unwrap_err(), EditError::NotFound));
    }

    #[test]
    fn test_exact_match_multiple_rejected() {
        let content = "aaa bbb aaa";
        let result = replace(content, "aaa", "ccc", false);
        assert!(matches!(
            result.unwrap_err(),
            EditError::MultipleMatches { count: 2 }
        ));
    }

    #[test]
    fn test_replace_all() {
        let content = "aaa bbb aaa";
        let result = replace(content, "aaa", "ccc", true).unwrap();
        assert_eq!(result, "ccc bbb ccc");
    }

    #[test]
    fn test_identical_strings_error() {
        let result = replace("content", "same", "same", false);
        assert!(matches!(result.unwrap_err(), EditError::IdenticalStrings));
    }

    #[test]
    fn test_whitespace_normalized_matches() {
        let content = "fn foo() {  \n    bar();  \n}\n";
        // old_string has no trailing spaces
        let old = "fn foo() {\n    bar();\n}";
        let new = "fn foo() {\n    baz();\n}";
        let result = replace(content, old, new, false).unwrap();
        assert!(result.contains("baz()"));
    }

    #[test]
    fn test_indentation_flexible_matches() {
        let content = "        if true {\n            do_stuff();\n        }\n";
        // old_string uses less indentation
        let old = "    if true {\n        do_stuff();\n    }";
        let new = "    if true {\n        do_other();\n    }";
        let result = replace(content, old, new, false).unwrap();
        assert!(result.contains("do_other()"));
    }

    #[test]
    fn test_fallback_chain_prefers_exact() {
        // Content that matches at all levels: exact should win
        let content = "hello world\n";
        let result = replace(content, "hello world", "goodbye world", false).unwrap();
        assert_eq!(result, "goodbye world\n");
    }

    // ── Tool integration tests ──────────────────────────────────────

    #[tokio::test]
    async fn test_edit_tool_basic() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        let file_path = base.join("test.txt");
        tokio::fs::write(&file_path, "line1\nline2\nline3\n")
            .await
            .unwrap();

        let tracker = Arc::new(FileTracker::new());
        tracker.record_read(&file_path);

        let tool = EditTool::with_tracker(tracker).with_base_dir(&base);

        let output = tool
            .execute(json!({
                "file_path": file_path.to_str().unwrap(),
                "old_string": "line2",
                "new_string": "modified"
            }))
            .await
            .unwrap();

        assert!(output.success);
        let content = tokio::fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "line1\nmodified\nline3\n");
    }

    #[tokio::test]
    async fn test_edit_tool_read_guard() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        let file_path = base.join("test.txt");
        tokio::fs::write(&file_path, "content").await.unwrap();

        let tracker = Arc::new(FileTracker::new());
        // Intentionally do NOT record a read

        let tool = EditTool::with_tracker(tracker).with_base_dir(&base);

        let output = tool
            .execute(json!({
                "file_path": file_path.to_str().unwrap(),
                "old_string": "content",
                "new_string": "replaced"
            }))
            .await
            .unwrap();

        assert!(!output.success);
        assert!(output.error.as_deref().unwrap_or("").contains("must read"));
    }
}
