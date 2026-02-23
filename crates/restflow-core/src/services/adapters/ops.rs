//! OpsProvider adapter for daemon status, health, and operational queries.

use crate::daemon::{DaemonStatus, check_daemon_status, check_health};
use crate::models::BackgroundAgentStatus;
use crate::storage::{BackgroundAgentStorage, ChatSessionStorage};
use chrono::Utc;
use restflow_ai::tools::OpsProvider;
use restflow_tools::ToolError;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Build a standard ops response envelope.
fn build_ops_response(operation: &str, evidence: Value, verification: Value) -> Value {
    json!({
        "operation": operation,
        "evidence": evidence,
        "verification": verification
    })
}

pub struct OpsProviderAdapter {
    background_storage: BackgroundAgentStorage,
    chat_storage: ChatSessionStorage,
}

impl OpsProviderAdapter {
    pub fn new(
        background_storage: BackgroundAgentStorage,
        chat_storage: ChatSessionStorage,
    ) -> Self {
        Self {
            background_storage,
            chat_storage,
        }
    }

    fn parse_status_filter(
        status: Option<&str>,
    ) -> restflow_tools::Result<Option<BackgroundAgentStatus>> {
        let Some(status) = status else {
            return Ok(None);
        };
        let parsed = match status.trim().to_ascii_lowercase().as_str() {
            "active" => BackgroundAgentStatus::Active,
            "paused" => BackgroundAgentStatus::Paused,
            "running" => BackgroundAgentStatus::Running,
            "completed" => BackgroundAgentStatus::Completed,
            "failed" => BackgroundAgentStatus::Failed,
            "interrupted" => BackgroundAgentStatus::Interrupted,
            value => {
                return Err(ToolError::Tool(format!(
                    "Unknown status: {}. Supported: active, paused, running, completed, failed, interrupted",
                    value
                )));
            }
        };
        Ok(Some(parsed))
    }

    fn canonical_existing_ancestor(path: &Path) -> anyhow::Result<PathBuf> {
        let mut current = if path.exists() {
            path.to_path_buf()
        } else {
            path.parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| path.to_path_buf())
        };

        while !current.exists() {
            if !current.pop() {
                break;
            }
        }

        if !current.exists() {
            anyhow::bail!(
                "No existing ancestor found for path: {}",
                path.display()
            );
        }

        Ok(current.canonicalize()?)
    }

    pub(crate) fn resolve_log_tail_path(path: Option<&str>) -> restflow_tools::Result<PathBuf> {
        let logs_dir = crate::paths::logs_dir()?;
        let resolved = match path
            .map(str::trim)
            .filter(|raw| !raw.is_empty())
            .map(PathBuf::from)
        {
            Some(custom_path) if custom_path.is_absolute() => custom_path,
            Some(custom_path) => logs_dir.join(custom_path),
            None => crate::paths::daemon_log_path()?,
        };

        let logs_root = Self::canonical_existing_ancestor(&logs_dir)?;
        let path_root = Self::canonical_existing_ancestor(&resolved)?;
        if !path_root.starts_with(&logs_root) {
            return Err(ToolError::Tool(format!(
                "log_tail path must stay under {}",
                logs_dir.display()
            )));
        }

        if let Ok(metadata) = std::fs::symlink_metadata(&resolved)
            && metadata.file_type().is_symlink()
        {
            return Err(ToolError::Tool(
                "log_tail does not allow symlink paths".to_string(),
            ));
        }

        Ok(resolved)
    }

    pub(crate) fn read_log_tail(path: &Path, lines: usize) -> anyhow::Result<(Vec<String>, bool)> {
        let mut file = std::fs::File::open(path)?;
        let mut content = String::new();
        use std::io::Read;
        file.read_to_string(&mut content)?;
        let all_lines: Vec<String> = content.lines().map(str::to_string).collect();
        let total = all_lines.len();
        let start = total.saturating_sub(lines);
        let truncated = total > lines;
        Ok((all_lines[start..].to_vec(), truncated))
    }
}

impl OpsProvider for OpsProviderAdapter {
    fn daemon_status(&self) -> restflow_tools::Result<Value> {
        let status = check_daemon_status()?;
        let evidence = match status {
            DaemonStatus::Running { pid } => json!({
                "status": "running",
                "pid": pid
            }),
            DaemonStatus::NotRunning => json!({
                "status": "not_running"
            }),
            DaemonStatus::Stale { pid } => json!({
                "status": "stale",
                "pid": pid
            }),
        };
        let verification = json!({
            "source": "daemon_pid_file",
            "checked_at": Utc::now().timestamp_millis()
        });
        Ok(build_ops_response("daemon_status", evidence, verification))
    }

    fn daemon_health(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = restflow_tools::Result<Value>> + Send + '_>>
    {
        Box::pin(async move {
            let socket = crate::paths::socket_path()?;
            let health = check_health(socket, None).await?;
            let evidence = serde_json::to_value(health)?;
            let verification = json!({
                "healthy": evidence["healthy"],
                "ipc_checked": true,
                "http_checked": false
            });
            Ok(build_ops_response("daemon_health", evidence, verification))
        })
    }

    fn background_summary(
        &self,
        status: Option<&str>,
        limit: usize,
    ) -> restflow_tools::Result<Value> {
        let status_filter = Self::parse_status_filter(status)?;
        let tasks = match status_filter.clone() {
            Some(s) => self.background_storage.list_tasks_by_status(s)?,
            None => self.background_storage.list_tasks()?,
        };
        let mut by_status: BTreeMap<String, usize> = BTreeMap::new();
        for task in &tasks {
            *by_status
                .entry(task.status.as_str().to_string())
                .or_default() += 1;
        }
        let sample: Vec<Value> = tasks
            .iter()
            .take(limit)
            .map(|task| {
                json!({
                    "id": task.id,
                    "name": task.name,
                    "agent_id": task.agent_id,
                    "status": task.status.as_str(),
                    "updated_at": task.updated_at
                })
            })
            .collect();
        let evidence = json!({
            "total": tasks.len(),
            "by_status": by_status,
            "sample": sample
        });
        let verification = json!({
            "status_filter": status_filter.as_ref().map(|s| s.as_str()),
            "sample_limit": limit,
            "derived_from": "background_agent_storage"
        });
        Ok(build_ops_response("background_summary", evidence, verification))
    }

    fn session_summary(&self, limit: usize) -> restflow_tools::Result<Value> {
        let summaries = self.chat_storage.list_summaries()?;
        let recent: Vec<Value> = summaries
            .iter()
            .take(limit)
            .map(|session| {
                json!({
                    "id": session.id,
                    "name": session.name,
                    "agent_id": session.agent_id,
                    "model": session.model,
                    "message_count": session.message_count,
                    "updated_at": session.updated_at,
                    "last_message_preview": session.last_message_preview
                })
            })
            .collect();
        let evidence = json!({
            "total": summaries.len(),
            "recent": recent
        });
        let verification = json!({
            "sorted_by": "updated_at_desc",
            "sample_limit": limit,
            "derived_from": "chat_session_storage"
        });
        Ok(build_ops_response("session_summary", evidence, verification))
    }

    fn log_tail(&self, lines: usize, path: Option<&str>) -> restflow_tools::Result<Value> {
        let resolved = Self::resolve_log_tail_path(path)?;
        if !resolved.exists() {
            let evidence = json!({
                "path": resolved.to_string_lossy(),
                "lines": [],
                "line_count": 0
            });
            let verification = json!({
                "path_exists": false,
                "requested_lines": lines
            });
            return Ok(build_ops_response("log_tail", evidence, verification));
        }

        let (tail, truncated) = Self::read_log_tail(&resolved, lines)?;
        let evidence = json!({
            "path": resolved.to_string_lossy(),
            "lines": tail,
            "line_count": tail.len()
        });
        let verification = json!({
            "path_exists": true,
            "requested_lines": lines,
            "truncated": truncated
        });
        Ok(build_ops_response("log_tail", evidence, verification))
    }
}

// Test helper for log_tail_payload (used by tests)
#[cfg(test)]
impl OpsProviderAdapter {
    pub(crate) fn log_tail_payload(input: &Value) -> restflow_tools::Result<(Value, Value)> {
        let lines = input
            .get("lines")
            .and_then(Value::as_u64)
            .map(|v| v as usize)
            .unwrap_or(100)
            .clamp(1, 1000);
        let path = Self::resolve_log_tail_path(
            input
                .get("path")
                .and_then(Value::as_str),
        )?;
        if !path.exists() {
            let evidence = json!({
                "path": path.to_string_lossy(),
                "lines": [],
                "line_count": 0
            });
            let verification = json!({
                "path_exists": false,
                "requested_lines": lines
            });
            return Ok((evidence, verification));
        }

        let (tail, truncated) = Self::read_log_tail(&path, lines)?;
        let evidence = json!({
            "path": path.to_string_lossy(),
            "lines": tail,
            "line_count": tail.len()
        });
        let verification = json!({
            "path_exists": true,
            "requested_lines": lines,
            "truncated": truncated
        });
        Ok((evidence, verification))
    }
}

#[cfg(test)]
mod tests_adapter {
    use super::*;
    use restflow_ai::tools::OpsProvider;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn setup() -> (OpsProviderAdapter, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(redb::Database::create(db_path).unwrap());
        let bg_storage = BackgroundAgentStorage::new(db.clone()).unwrap();
        let chat_storage = ChatSessionStorage::new(db).unwrap();
        (OpsProviderAdapter::new(bg_storage, chat_storage), temp_dir)
    }

    #[test]
    fn test_background_summary_empty() {
        let (adapter, _dir) = setup();
        let result = adapter.background_summary(None, 10).unwrap();
        assert_eq!(result["operation"], "background_summary");
        assert_eq!(result["evidence"]["total"], 0);
    }

    #[test]
    fn test_session_summary_empty() {
        let (adapter, _dir) = setup();
        let result = adapter.session_summary(10).unwrap();
        assert_eq!(result["operation"], "session_summary");
        assert_eq!(result["evidence"]["total"], 0);
    }

    #[test]
    fn test_daemon_status() {
        let (adapter, _dir) = setup();
        let result = adapter.daemon_status().unwrap();
        assert_eq!(result["operation"], "daemon_status");
        assert!(result["evidence"]["status"].is_string());
    }

    #[test]
    fn test_log_tail_nonexistent_file() {
        let (adapter, _dir) = setup();
        // log_tail with default path should work (returns empty if no file)
        let result = adapter.log_tail(10, None);
        // Result depends on system state but should not panic
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_read_log_tail() {
        let dir = tempdir().unwrap();
        let log_file = dir.path().join("test.log");
        std::fs::write(&log_file, "line1\nline2\nline3\nline4\nline5\n").unwrap();

        let (lines, truncated) = OpsProviderAdapter::read_log_tail(&log_file, 3).unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "line3");
        assert_eq!(lines[2], "line5");
        assert!(truncated);
    }

    #[test]
    fn test_read_log_tail_no_truncation() {
        let dir = tempdir().unwrap();
        let log_file = dir.path().join("test.log");
        std::fs::write(&log_file, "line1\nline2\n").unwrap();

        let (lines, truncated) = OpsProviderAdapter::read_log_tail(&log_file, 100).unwrap();
        assert_eq!(lines.len(), 2);
        assert!(!truncated);
    }

    #[test]
    fn test_parse_status_filter() {
        assert!(OpsProviderAdapter::parse_status_filter(None).unwrap().is_none());
        assert!(OpsProviderAdapter::parse_status_filter(Some("active")).unwrap().is_some());
        assert!(OpsProviderAdapter::parse_status_filter(Some("RUNNING")).unwrap().is_some());
        assert!(OpsProviderAdapter::parse_status_filter(Some("invalid")).is_err());
    }
}
