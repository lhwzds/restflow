use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Result as AnyResult, anyhow};
use async_trait::async_trait;
use serde_json::Value;
use tokio::fs;

use crate::error::Result;
use crate::tools::file_tracker::FileTracker;
use crate::tools::traits::{Tool, ToolOutput};

mod apply;
mod parser;

use apply::apply_hunks;
use parser::{PatchOperation, parse_patch};

#[derive(Debug, Clone)]
pub struct PatchTool {
    base_dir: Option<PathBuf>,
    tracker: Arc<FileTracker>,
}

impl PatchTool {
    pub fn new(tracker: Arc<FileTracker>) -> Self {
        Self {
            base_dir: None,
            tracker,
        }
    }

    pub fn with_base_dir(mut self, base_dir: impl Into<PathBuf>) -> Self {
        self.base_dir = Some(base_dir.into());
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
                let canonical_base = if base.exists() {
                    base.canonicalize().map_err(|e| e.to_string())?
                } else {
                    normalize_path(base)
                };
                if !canonical.starts_with(&canonical_base) {
                    return Err(format!(
                        "Path '{}' escapes allowed base directory '{}'. All file operations must be within this directory.",
                        canonical.display(),
                        canonical_base.display()
                    ));
                }
                return Ok(canonical);
            }

            if base.exists() {
                let Some((ancestor, suffix)) = find_existing_ancestor(&resolved) else {
                    return Err(format!(
                        "Path '{}' escapes allowed base directory '{}'. All file operations must be within this directory.",
                        resolved.display(),
                        base.display()
                    ));
                };
                let canonical_parent = ancestor.canonicalize().map_err(|e| e.to_string())?;
                let candidate = normalize_path(&canonical_parent.join(suffix));
                let canonical_base = base.canonicalize().map_err(|e| e.to_string())?;
                if !candidate.starts_with(&canonical_base) {
                    return Err(format!(
                        "Path '{}' escapes allowed base directory '{}'. All file operations must be within this directory.",
                        candidate.display(),
                        canonical_base.display()
                    ));
                }
                return Ok(candidate);
            }

            let canonical_base = normalize_path(base);
            let normalized = normalize_path(&resolved);
            if !normalized.starts_with(&canonical_base) {
                return Err(format!(
                    "Path '{}' escapes allowed base directory '{}'. All file operations must be within this directory.",
                    normalized.display(),
                    canonical_base.display()
                ));
            }

            Ok(normalized)
        } else if path.is_absolute() {
            Ok(path)
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(&path))
                .map_err(|e| e.to_string())
        }
    }
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
                    "description": "Patch text using *** Update/Add/Delete File headers"
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::traits::Tool;

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
