//! Multi-edit tool for applying multiple replacements to a single file atomically.
//!
//! All edits succeed or the file is not modified.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::fs;

use super::edit::{EditError, replace};
use super::file_tracker::FileTracker;
use super::shared::{LSP_DIAGNOSTIC_TIMEOUT, MAX_LSP_DIAGNOSTIC_ERRORS};
use crate::{Result, Tool, ToolOutput};
use restflow_traits::cache::AgentCache;
use restflow_traits::store::DiagnosticsProvider;

#[derive(Clone)]
pub struct MultiEditTool {
    base_dir: Option<PathBuf>,
    tracker: Arc<FileTracker>,
    diagnostics: Option<Arc<dyn DiagnosticsProvider>>,
    cache_manager: Option<Arc<dyn AgentCache>>,
}

impl MultiEditTool {
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
        super::path_utils::resolve_path(path, self.base_dir.as_deref())
    }

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

    async fn run_diagnostics(&self, path: &Path) -> Option<String> {
        let provider = self.diagnostics.as_ref()?;

        if provider.ensure_open(path).await.is_err() {
            return None;
        }

        if let Ok(content) = fs::read_to_string(path).await {
            let _ = provider.did_change(path, &content).await;
        }

        let diags = match provider
            .wait_for_diagnostics(path, LSP_DIAGNOSTIC_TIMEOUT)
            .await
        {
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
            .take(MAX_LSP_DIAGNOSTIC_ERRORS)
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
impl Tool for MultiEditTool {
    fn name(&self) -> &str {
        "multiedit"
    }

    fn description(&self) -> &str {
        "Apply multiple text replacements to a single file atomically. \
         All edits must succeed or the file is not modified."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file to edit"
                },
                "edits": {
                    "type": "array",
                    "description": "List of edit operations to apply sequentially",
                    "items": {
                        "type": "object",
                        "properties": {
                            "old_string": {
                                "type": "string",
                                "description": "The exact text to find"
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
                        "required": ["old_string", "new_string"]
                    }
                }
            },
            "required": ["file_path", "edits"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::ToolError::Tool("Missing 'file_path' argument".into()))?;

        let edits = args
            .get("edits")
            .and_then(|v| v.as_array())
            .ok_or_else(|| crate::ToolError::Tool("Missing 'edits' array argument".into()))?;

        if edits.is_empty() {
            return Ok(ToolOutput::error("'edits' array must not be empty"));
        }

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

        // Apply all edits sequentially in memory
        let mut current = content.clone();
        for (i, edit) in edits.iter().enumerate() {
            let old_string = edit
                .get("old_string")
                .and_then(|v| v.as_str())
                .ok_or_else(|| crate::ToolError::Tool(format!("Edit {i}: missing 'old_string'")))?;
            let new_string = edit
                .get("new_string")
                .and_then(|v| v.as_str())
                .ok_or_else(|| crate::ToolError::Tool(format!("Edit {i}: missing 'new_string'")))?;
            let replace_all_edit = edit
                .get("replace_all")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            match replace(&current, old_string, new_string, replace_all_edit) {
                Ok(result) => current = result,
                Err(EditError::IdenticalStrings) => {
                    return Ok(ToolOutput::error(format!(
                        "Edit {i}: old_string and new_string are identical"
                    )));
                }
                Err(EditError::NotFound) => {
                    return Ok(ToolOutput::error(format!(
                        "Edit {i}: old_string not found in file (after applying previous edits). \
                         No changes were written to disk."
                    )));
                }
                Err(EditError::MultipleMatches { count }) => {
                    return Ok(ToolOutput::error(format!(
                        "Edit {i}: old_string matched {count} locations. \
                         No changes were written to disk."
                    )));
                }
            }
        }

        // All edits succeeded; write once
        let old_line_count = content.lines().count();
        let new_line_count = current.lines().count();
        let lines_changed = new_line_count.abs_diff(old_line_count);

        if let Err(e) = fs::write(&path, &current).await {
            return Ok(ToolOutput::error(format!("Cannot write file: {e}")));
        }

        self.tracker.record_write(&path);
        self.invalidate_caches(&path).await;

        let mut msg = format!(
            "{} edits applied to {} ({} lines changed)",
            edits.len(),
            path.display(),
            lines_changed
        );

        if let Some(diag_output) = self.run_diagnostics(&path).await {
            msg.push_str(&diag_output);
        }

        Ok(ToolOutput::success(json!({
            "message": msg,
            "path": path.display().to_string(),
            "edits_applied": edits.len(),
            "lines_changed": lines_changed,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_multiedit_sequential() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        let file_path = base.join("test.txt");
        tokio::fs::write(&file_path, "aaa\nbbb\nccc\n")
            .await
            .unwrap();

        let tracker = Arc::new(FileTracker::new());
        tracker.record_read(&file_path);

        let tool = MultiEditTool::with_tracker(tracker).with_base_dir(&base);

        let output = tool
            .execute(json!({
                "file_path": file_path.to_str().unwrap(),
                "edits": [
                    { "old_string": "aaa", "new_string": "xxx" },
                    { "old_string": "ccc", "new_string": "zzz" }
                ]
            }))
            .await
            .unwrap();

        assert!(output.success);
        let content = tokio::fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "xxx\nbbb\nzzz\n");
    }

    #[tokio::test]
    async fn test_multiedit_atomic_failure() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        let file_path = base.join("test.txt");
        tokio::fs::write(&file_path, "aaa\nbbb\nccc\n")
            .await
            .unwrap();

        let tracker = Arc::new(FileTracker::new());
        tracker.record_read(&file_path);

        let tool = MultiEditTool::with_tracker(tracker).with_base_dir(&base);

        let output = tool
            .execute(json!({
                "file_path": file_path.to_str().unwrap(),
                "edits": [
                    { "old_string": "aaa", "new_string": "xxx" },
                    { "old_string": "nonexistent", "new_string": "yyy" }
                ]
            }))
            .await
            .unwrap();

        assert!(!output.success);
        // File should NOT have been modified
        let content = tokio::fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "aaa\nbbb\nccc\n");
    }
}
