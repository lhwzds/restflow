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
        let logs_dir =
            crate::paths::logs_dir().map_err(|e| ToolError::Tool(e.to_string()))?;
        let resolved = match path
            .map(str::trim)
            .filter(|raw| !raw.is_empty())
            .map(PathBuf::from)
        {
            Some(custom_path) if custom_path.is_absolute() => custom_path,
            Some(custom_path) => logs_dir.join(custom_path),
            None => crate::paths::daemon_log_path()
                .map_err(|e| ToolError::Tool(e.to_string()))?,
        };

        let logs_root = Self::canonical_existing_ancestor(&logs_dir)
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        let path_root = Self::canonical_existing_ancestor(&resolved)
            .map_err(|e| ToolError::Tool(e.to_string()))?;
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
        let status =
            check_daemon_status().map_err(|e| ToolError::Tool(e.to_string()))?;
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
        Ok(crate::ops::build_response(
            crate::ops::ManageOpsOperation::DaemonStatus,
            evidence,
            verification,
        ))
    }

    fn daemon_health(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = restflow_tools::Result<Value>> + Send + '_>>
    {
        Box::pin(async move {
            let socket = crate::paths::socket_path()
                .map_err(|e| ToolError::Tool(e.to_string()))?;
            let health = check_health(socket, None)
                .await
                .map_err(|e| ToolError::Tool(e.to_string()))?;
            let evidence = serde_json::to_value(health).map_err(ToolError::from)?;
            let verification = json!({
                "healthy": evidence["healthy"],
                "ipc_checked": true,
                "http_checked": false
            });
            Ok(crate::ops::build_response(
                crate::ops::ManageOpsOperation::DaemonHealth,
                evidence,
                verification,
            ))
        })
    }

    fn background_summary(
        &self,
        status: Option<&str>,
        limit: usize,
    ) -> restflow_tools::Result<Value> {
        let status_filter = Self::parse_status_filter(status)?;
        let tasks = match status_filter.clone() {
            Some(s) => self
                .background_storage
                .list_tasks_by_status(s)
                .map_err(|e| ToolError::Tool(e.to_string()))?,
            None => self
                .background_storage
                .list_tasks()
                .map_err(|e| ToolError::Tool(e.to_string()))?,
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
        Ok(crate::ops::build_response(
            crate::ops::ManageOpsOperation::BackgroundSummary,
            evidence,
            verification,
        ))
    }

    fn session_summary(&self, limit: usize) -> restflow_tools::Result<Value> {
        let summaries = self
            .chat_storage
            .list_summaries()
            .map_err(|e| ToolError::Tool(e.to_string()))?;
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
        Ok(crate::ops::build_response(
            crate::ops::ManageOpsOperation::SessionSummary,
            evidence,
            verification,
        ))
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
            return Ok(crate::ops::build_response(
                crate::ops::ManageOpsOperation::LogTail,
                evidence,
                verification,
            ));
        }

        let (tail, truncated) = Self::read_log_tail(&resolved, lines)
            .map_err(|e| ToolError::Tool(e.to_string()))?;
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
        Ok(crate::ops::build_response(
            crate::ops::ManageOpsOperation::LogTail,
            evidence,
            verification,
        ))
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

        let (tail, truncated) =
            Self::read_log_tail(&path, lines).map_err(|e| ToolError::Tool(e.to_string()))?;
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
