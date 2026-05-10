use std::iter::Peekable;
use std::path::{Path, PathBuf};
use std::str::Lines;
use std::sync::Arc;

use anyhow::{Result as AnyResult, anyhow};
use async_trait::async_trait;
use serde_json::Value;
use tokio::fs;

use super::file_tracker::FileTracker;
use crate::tools::Result;
use crate::tools::{Tool, ToolOutput};

#[derive(Debug, Clone)]
pub struct PatchTool {
    base_dir: Option<PathBuf>,
    require_base_dir: bool,
    tracker: Arc<FileTracker>,
}

impl PatchTool {
    pub fn new(tracker: Arc<FileTracker>) -> Self {
        Self {
            base_dir: None,
            require_base_dir: false,
            tracker,
        }
    }

    pub fn with_base_dir(mut self, base_dir: impl Into<PathBuf>) -> Self {
        self.base_dir = Some(base_dir.into());
        self
    }

    pub fn require_base_dir(mut self) -> Self {
        self.require_base_dir = true;
        self
    }

    fn resolve_path(&self, path: &str) -> std::result::Result<PathBuf, String> {
        crate::tools::impls::path_utils::resolve_path_with_policy(
            path,
            self.base_dir.as_deref(),
            self.require_base_dir,
        )
    }
}

#[derive(Debug, Clone)]
enum PatchOperation {
    Update { path: String, hunks: Vec<Hunk> },
    Add { path: String, content: String },
    Delete { path: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Hunk {
    context_before: Vec<String>,
    removals: Vec<String>,
    additions: Vec<String>,
    context_after: Vec<String>,
}

fn parse_patch(text: &str) -> AnyResult<Vec<PatchOperation>> {
    let mut operations = Vec::new();
    let mut lines = text.lines().peekable();

    while let Some(line) = lines.next() {
        if let Some(path) = line.strip_prefix("*** Update File: ") {
            let block = collect_block(&mut lines);
            let hunks = parse_hunks(&block)?;
            operations.push(PatchOperation::Update {
                path: path.trim().to_string(),
                hunks,
            });
            continue;
        }

        if let Some(path) = line.strip_prefix("*** Add File: ") {
            let block = collect_block(&mut lines);
            let content = parse_added_content(&block);
            operations.push(PatchOperation::Add {
                path: path.trim().to_string(),
                content,
            });
            continue;
        }

        if let Some(path) = line.strip_prefix("*** Delete File: ") {
            operations.push(PatchOperation::Delete {
                path: path.trim().to_string(),
            });
            continue;
        }
    }

    if operations.is_empty() {
        return Err(anyhow!("No valid patch operations found"));
    }

    Ok(operations)
}

fn collect_block(lines: &mut Peekable<Lines<'_>>) -> Vec<String> {
    let mut block = Vec::new();
    while let Some(line) = lines.peek() {
        if line.starts_with("*** ") {
            break;
        }
        block.push(lines.next().unwrap_or_default().to_string());
    }
    block
}

fn parse_hunks(block: &[String]) -> AnyResult<Vec<Hunk>> {
    if block.is_empty() {
        return Err(anyhow!("Update block is empty"));
    }

    if block.iter().any(|line| line.starts_with("@@")) {
        return parse_unified_hunks(block);
    }

    let mut hunks = Vec::new();
    let mut start = 0;

    for (idx, line) in block.iter().enumerate() {
        if line.trim() == "---" {
            if start < idx {
                hunks.push(parse_hunk_lines(&block[start..idx])?);
            }
            start = idx + 1;
        }
    }

    if start < block.len() {
        hunks.push(parse_hunk_lines(&block[start..])?);
    }

    if hunks.is_empty() {
        return Err(anyhow!("No hunks found in update block"));
    }

    Ok(hunks)
}

fn parse_hunk_lines(lines: &[String]) -> AnyResult<Hunk> {
    let mut context_before = Vec::new();
    let mut removals = Vec::new();
    let mut additions = Vec::new();
    let mut context_after = Vec::new();

    let mut in_changes = false;
    let mut finished_changes = false;

    for line in lines {
        if let Some(stripped) = line.strip_prefix('-') {
            if finished_changes {
                return Err(anyhow!("Change lines must be contiguous"));
            }
            in_changes = true;
            removals.push(stripped.to_string());
            continue;
        }

        if let Some(stripped) = line.strip_prefix('+') {
            if finished_changes {
                return Err(anyhow!("Change lines must be contiguous"));
            }
            in_changes = true;
            additions.push(stripped.to_string());
            continue;
        }

        if !in_changes {
            context_before.push(line.to_string());
        } else {
            finished_changes = true;
            context_after.push(line.to_string());
        }
    }

    if removals.is_empty() && additions.is_empty() {
        return Err(anyhow!("Hunk has no changes"));
    }

    Ok(Hunk {
        context_before,
        removals,
        additions,
        context_after,
    })
}

fn parse_unified_hunks(block: &[String]) -> AnyResult<Vec<Hunk>> {
    let mut hunks = Vec::new();
    let mut current = Vec::new();
    let mut saw_header = false;

    for line in block {
        if line.starts_with("--- ") || line.starts_with("+++ ") {
            continue;
        }
        if line.starts_with("@@") {
            if !current.is_empty() {
                hunks.push(parse_unified_hunk_lines(&current)?);
                current.clear();
            }
            saw_header = true;
            continue;
        }
        if saw_header {
            current.push(line.clone());
        }
    }

    if !current.is_empty() {
        hunks.push(parse_unified_hunk_lines(&current)?);
    }

    if hunks.is_empty() {
        return Err(anyhow!("No hunks found in update block"));
    }

    Ok(hunks)
}

fn parse_unified_hunk_lines(lines: &[String]) -> AnyResult<Hunk> {
    let mut lines = lines;
    while let Some((last, rest)) = lines.split_last()
        && last.is_empty()
    {
        lines = rest;
    }
    let normalized = lines
        .iter()
        .map(|line| {
            line.strip_prefix(' ')
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| line.to_string())
        })
        .collect::<Vec<_>>();
    parse_hunk_lines(&normalized)
}

fn parse_added_content(lines: &[String]) -> String {
    if lines.iter().any(|line| line.starts_with("@@")) {
        return parse_unified_added_content(lines);
    }
    let mut content_lines = Vec::new();
    for line in lines {
        if let Some(stripped) = line.strip_prefix('+') {
            content_lines.push(stripped.to_string());
        } else {
            content_lines.push(line.to_string());
        }
    }
    content_lines.join("\n")
}

fn parse_unified_added_content(lines: &[String]) -> String {
    let mut content_lines = Vec::new();
    let mut saw_header = false;

    for line in lines {
        if line.starts_with("--- ") || line.starts_with("+++ ") {
            continue;
        }
        if line.starts_with("@@") {
            saw_header = true;
            continue;
        }
        if !saw_header {
            continue;
        }
        if let Some(stripped) = line.strip_prefix('+') {
            content_lines.push(stripped.to_string());
        } else if let Some(stripped) = line.strip_prefix(' ') {
            content_lines.push(stripped.to_string());
        }
    }

    content_lines.join("\n")
}

fn apply_hunks(original: &str, hunks: &[Hunk]) -> AnyResult<String> {
    let mut lines: Vec<String> = original.lines().map(|line| line.to_string()).collect();

    for hunk in hunks {
        let position = find_hunk_position(&lines, hunk)?;
        let remove_count =
            hunk.context_before.len() + hunk.removals.len() + hunk.context_after.len();
        let mut new_lines = Vec::new();
        new_lines.extend(hunk.context_before.iter().cloned());
        new_lines.extend(hunk.additions.iter().cloned());
        new_lines.extend(hunk.context_after.iter().cloned());

        lines.splice(position..position + remove_count, new_lines);
    }

    Ok(lines.join("\n"))
}

fn find_hunk_position(lines: &[String], hunk: &Hunk) -> AnyResult<usize> {
    let mut search_lines: Vec<&str> = Vec::new();
    search_lines.extend(hunk.context_before.iter().map(String::as_str));
    search_lines.extend(hunk.removals.iter().map(String::as_str));
    search_lines.extend(hunk.context_after.iter().map(String::as_str));

    if search_lines.is_empty() {
        return Err(anyhow!("Hunk has no searchable context"));
    }
    if search_lines.len() > lines.len() {
        return Err(anyhow!("Could not find matching context for hunk"));
    }

    let mut first_match: Option<usize> = None;

    for i in 0..=lines.len() - search_lines.len() {
        let mut matched = true;
        for (offset, expected) in search_lines.iter().enumerate() {
            if lines[i + offset] != *expected {
                matched = false;
                break;
            }
        }
        if matched {
            if first_match.is_some() {
                return Err(anyhow!(
                    "Ambiguous patch: hunk matches at multiple locations. Add more context lines to disambiguate."
                ));
            }
            first_match = Some(i);
        }
    }

    first_match.ok_or_else(|| anyhow!("Could not find matching context for hunk"))
}

#[async_trait]
impl Tool for PatchTool {
    fn name(&self) -> &str {
        "patch"
    }

    fn description(&self) -> &str {
        "Apply structured multi-file patches (add, update, delete) in one operation."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "patch": {
                    "type": "string",
                    "description": "Patch text using *** Update/Add/Delete File headers. Update blocks accept either simple context/-old/+new lines or unified diff hunks with @@ headers."
                }
            },
            "required": ["patch"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let patch_text = match input.get("patch").and_then(|value| value.as_str()) {
            Some(value) => value,
            None => return Ok(ToolOutput::error("patch is required")),
        };

        let operations = match parse_patch(patch_text) {
            Ok(ops) => ops,
            Err(err) => return Ok(ToolOutput::error(err.to_string())),
        };

        match self.apply_operations(&operations).await {
            Ok(results) => Ok(ToolOutput::success(serde_json::json!({
                "results": results
            }))),
            Err(err) => Ok(ToolOutput::error(err.to_string())),
        }
    }
}

impl PatchTool {
    async fn apply_operations(&self, operations: &[PatchOperation]) -> AnyResult<Vec<String>> {
        let mut staged: Vec<StagedOperation> = Vec::new();

        for operation in operations {
            match operation {
                PatchOperation::Update { path, hunks } => {
                    let resolved = self.resolve_path(path).map_err(|err| anyhow!(err))?;
                    self.ensure_file_exists(&resolved)?;
                    if !self.tracker.has_been_read(&resolved) {
                        return Err(anyhow!(
                            "File {} has not been read. Read it before patching.",
                            resolved.display()
                        ));
                    }
                    if self.tracker.check_external_modification(&resolved).await? {
                        return Err(anyhow!(
                            "File {} modified externally. Read it first.",
                            resolved.display()
                        ));
                    }
                    let original = fs::read_to_string(&resolved).await?;
                    let patched = apply_hunks(&original, hunks)?;
                    staged.push(StagedOperation::Update {
                        path: resolved,
                        original,
                        patched,
                    });
                }
                PatchOperation::Add { path, content } => {
                    let resolved = self.resolve_path(path).map_err(|err| anyhow!(err))?;
                    if resolved.exists() {
                        return Err(anyhow!("File already exists: {}", resolved.display()));
                    }
                    staged.push(StagedOperation::Add {
                        path: resolved,
                        content: content.to_string(),
                    });
                }
                PatchOperation::Delete { path } => {
                    let resolved = self.resolve_path(path).map_err(|err| anyhow!(err))?;
                    if !self.tracker.has_been_read(&resolved) {
                        return Err(anyhow!(
                            "File {} has not been read. Read it before deleting.",
                            resolved.display()
                        ));
                    }
                    self.ensure_file_exists(&resolved)?;
                    if self.tracker.check_external_modification(&resolved).await? {
                        return Err(anyhow!(
                            "File {} modified externally. Read it first.",
                            resolved.display()
                        ));
                    }
                    let original = fs::read_to_string(&resolved).await?;
                    staged.push(StagedOperation::Delete {
                        path: resolved,
                        original,
                    });
                }
            }
        }

        let mut backups = Vec::new();
        let mut results = Vec::new();

        for op in &staged {
            let apply_result: AnyResult<()> = match op {
                StagedOperation::Update {
                    path,
                    original,
                    patched,
                } => {
                    backups.push(Backup {
                        path: path.clone(),
                        original: Some(original.clone()),
                    });
                    match fs::write(path, patched).await {
                        Ok(()) => {
                            self.tracker.record_write(path);
                            results.push(format!("Updated: {}", path.display()));
                            Ok(())
                        }
                        Err(err) => Err(err.into()),
                    }
                }
                StagedOperation::Add { path, content } => {
                    let create_result: AnyResult<()> = if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent).await.map_err(|err| err.into())
                    } else {
                        Ok(())
                    };

                    if let Err(err) = create_result {
                        Err(err)
                    } else {
                        backups.push(Backup {
                            path: path.clone(),
                            original: None,
                        });
                        match fs::write(path, content).await {
                            Ok(()) => {
                                self.tracker.record_write(path);
                                results.push(format!("Created: {}", path.display()));
                                Ok(())
                            }
                            Err(err) => Err(err.into()),
                        }
                    }
                }
                StagedOperation::Delete { path, original } => {
                    backups.push(Backup {
                        path: path.clone(),
                        original: Some(original.clone()),
                    });
                    match fs::remove_file(path).await {
                        Ok(()) => {
                            self.tracker.record_write(path);
                            results.push(format!("Deleted: {}", path.display()));
                            Ok(())
                        }
                        Err(err) => Err(err.into()),
                    }
                }
            };

            if let Err(err) = apply_result {
                rollback(&backups).await;
                return Err(err);
            }
        }

        Ok(results)
    }

    fn ensure_file_exists(&self, path: &Path) -> AnyResult<()> {
        if !path.exists() {
            return Err(anyhow!("File not found: {}", path.display()));
        }
        if !path.is_file() {
            return Err(anyhow!("Not a file: {}", path.display()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
enum StagedOperation {
    Update {
        path: PathBuf,
        original: String,
        patched: String,
    },
    Add {
        path: PathBuf,
        content: String,
    },
    Delete {
        path: PathBuf,
        original: String,
    },
}

#[derive(Debug, Clone)]
struct Backup {
    path: PathBuf,
    original: Option<String>,
}

async fn rollback(backups: &[Backup]) {
    for backup in backups.iter().rev() {
        match &backup.original {
            Some(content) => {
                let _ = fs::write(&backup.path, content).await;
            }
            None => {
                let _ = fs::remove_file(&backup.path).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Tool;

    #[tokio::test]
    async fn apply_operations_add_update_delete() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let tracker = Arc::new(FileTracker::new());
        let tool = PatchTool::new(tracker).with_base_dir(temp_dir.path());

        let file_path = temp_dir.path().join("example.txt");
        fs::write(&file_path, "line1\nline2\nline3").await.unwrap();
        let resolved = tool.resolve_path("example.txt").unwrap();
        tool.tracker.record_read(&resolved);

        let patch = "*** Update File: example.txt\nline1\n-line2\n+line2b\nline3\n*** Add File: new.txt\n+hello\n+world\n*** Delete File: example.txt";
        let operations = parse_patch(patch).unwrap();
        let result = tool.apply_operations(&operations).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn apply_operations_update_requires_read_first() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let tracker = Arc::new(FileTracker::new());
        let tool = PatchTool::new(tracker).with_base_dir(temp_dir.path());

        let file_path = temp_dir.path().join("example.txt");
        fs::write(&file_path, "line1\nline2\nline3").await.unwrap();

        let patch = "*** Update File: example.txt\nline1\n-line2\n+line2b\nline3";
        let operations = parse_patch(patch).unwrap();
        let result = tool.apply_operations(&operations).await;

        assert!(result.is_err());
        assert!(
            result
                .err()
                .unwrap()
                .to_string()
                .contains("has not been read")
        );
    }

    #[tokio::test]
    async fn patch_escape_error_includes_path_and_base_dir() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let tracker = Arc::new(FileTracker::new());
        let tool = PatchTool::new(tracker).with_base_dir(temp_dir.path());

        let output = tool
            .execute(serde_json::json!({
                "patch": "*** Add File: ../outside.txt\n+blocked"
            }))
            .await
            .unwrap();

        assert!(!output.success);
        let error = output.error.unwrap();
        assert!(error.contains("escapes allowed base directory"));
        assert!(error.contains(temp_dir.path().display().to_string().as_str()));
    }
}

#[tokio::test]
async fn apply_operations_delete_requires_read_first() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let tracker = Arc::new(FileTracker::new());
    let tool = PatchTool::new(tracker).with_base_dir(temp_dir.path());

    let file_path = temp_dir.path().join("to_delete.txt");
    fs::write(&file_path, "content").await.unwrap();

    // Delete without read first should fail
    let patch = "*** Delete File: to_delete.txt";
    let operations = parse_patch(patch).unwrap();
    let result = tool.apply_operations(&operations).await;

    assert!(result.is_err());
    assert!(
        result
            .err()
            .unwrap()
            .to_string()
            .contains("has not been read")
    );
}

#[tokio::test]
async fn apply_operations_delete_succeeds_after_read() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let tracker = Arc::new(FileTracker::new());
    let tool = PatchTool::new(tracker).with_base_dir(temp_dir.path());

    let file_path = temp_dir.path().join("to_delete.txt");
    fs::write(&file_path, "content").await.unwrap();

    // Read first
    let resolved = tool.resolve_path("to_delete.txt").unwrap();
    tool.tracker.record_read(&resolved);

    // Now delete should succeed
    let patch = "*** Delete File: to_delete.txt";
    let operations = parse_patch(patch).unwrap();
    let result = tool.apply_operations(&operations).await;

    assert!(result.is_ok());
}
