use crate::models::{ToolCallCompletion, ToolTrace};
use crate::storage::ToolTraceStorage;
use async_trait::async_trait;
use regex::Regex;
use restflow_ai::agent::StreamEmitter;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Instant;
use tracing::warn;

const MAX_EVENT_TEXT_CHARS: usize = 10_000;

fn truncate_text(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars {
        return value.to_string();
    }
    let mut truncated = value.chars().take(max_chars).collect::<String>();
    truncated.push_str("...");
    truncated
}

fn sanitize_secrets(input: &str) -> String {
    static SECRET_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(concat!(
            r"(?i)(?:",
            r"sk-[a-zA-Z0-9_-]{20,}",
            r"|xoxb-[a-zA-Z0-9_-]{20,}",
            r"|xoxp-[a-zA-Z0-9_-]{20,}",
            r"|Bearer\s+[a-zA-Z0-9._\-/+=]{20,}",
            r"|AKIA[0-9A-Z]{16}",
            r"|ghp_[a-zA-Z0-9]{36,}",
            r"|gho_[a-zA-Z0-9]{36,}",
            r"|glpat-[a-zA-Z0-9_-]{20,}",
            r#"|(?:api[_\-]?key|apikey|secret[_\-]?key|access[_\-]?token|auth[_\-]?token)\s*[=:]\s*["']?[a-zA-Z0-9._\-/+=]{8,}"#,
            r")",
        ))
        .expect("invalid secret pattern regex")
    });
    SECRET_PATTERN.replace_all(input, "[REDACTED]").into_owned()
}

fn normalize_payload(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let sanitized = sanitize_secrets(trimmed);
    let normalized = match serde_json::from_str::<serde_json::Value>(&sanitized) {
        Ok(json) => json.to_string(),
        Err(_) => sanitized,
    };
    Some(truncate_text(&normalized, MAX_EVENT_TEXT_CHARS))
}

fn normalize_full_payload(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let sanitized = sanitize_secrets(trimmed);
    match serde_json::from_str::<serde_json::Value>(&sanitized) {
        Ok(json) => Some(json.to_string()),
        Err(_) => Some(sanitized),
    }
}

fn sanitize_path_component(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn maybe_persist_full_output(
    session_id: &str,
    turn_id: &str,
    tool_call_id: &str,
    tool_name: &str,
    full_output: &str,
) -> Option<String> {
    if full_output.chars().count() <= MAX_EVENT_TEXT_CHARS {
        return None;
    }

    let traces_dir = crate::paths::ensure_restflow_dir()
        .ok()?
        .join("traces")
        .join(sanitize_path_component(session_id))
        .join(sanitize_path_component(turn_id));
    if std::fs::create_dir_all(&traces_dir).is_err() {
        return None;
    }

    let file_name = format!(
        "{}-{}.txt",
        sanitize_path_component(tool_name),
        sanitize_path_component(tool_call_id)
    );
    let path = traces_dir.join(file_name);
    if std::fs::write(&path, full_output).is_err() {
        return None;
    }

    Some(path.to_string_lossy().to_string())
}

/// Append turn-start event to tool trace storage.
pub fn append_turn_started(storage: &ToolTraceStorage, session_id: &str, turn_id: &str) {
    let event = ToolTrace::turn_started(session_id, turn_id);
    if let Err(error) = storage.append(&event) {
        warn!(
            session_id,
            turn_id,
            error = %error,
            "Failed to append turn started event"
        );
    }
}

/// Append turn-completed event to tool trace storage.
pub fn append_turn_completed(storage: &ToolTraceStorage, session_id: &str, turn_id: &str) {
    let event = ToolTrace::turn_completed(session_id, turn_id);
    if let Err(error) = storage.append(&event) {
        warn!(
            session_id,
            turn_id,
            error = %error,
            "Failed to append turn completed event"
        );
    }
}

/// Append turn-failed event to tool trace storage.
pub fn append_turn_failed(
    storage: &ToolTraceStorage,
    session_id: &str,
    turn_id: &str,
    error_text: &str,
) {
    let event = ToolTrace::turn_failed(
        session_id,
        turn_id,
        truncate_text(&sanitize_secrets(error_text), MAX_EVENT_TEXT_CHARS),
    );
    if let Err(error) = storage.append(&event) {
        warn!(
            session_id,
            turn_id,
            error = %error,
            "Failed to append turn failed event"
        );
    }
}

/// Append turn-cancelled event to tool trace storage.
pub fn append_turn_cancelled(
    storage: &ToolTraceStorage,
    session_id: &str,
    turn_id: &str,
    reason: &str,
) {
    let event = ToolTrace::turn_cancelled(
        session_id,
        turn_id,
        truncate_text(&sanitize_secrets(reason), MAX_EVENT_TEXT_CHARS),
    );
    if let Err(error) = storage.append(&event) {
        warn!(
            session_id,
            turn_id,
            error = %error,
            "Failed to append turn cancelled event"
        );
    }
}

/// Stream emitter that forwards events and persists tool call records to storage.
pub struct ToolTraceEmitter {
    inner: Box<dyn StreamEmitter>,
    trace_storage: ToolTraceStorage,
    session_id: String,
    turn_id: String,
    tool_start_times: HashMap<String, Instant>,
    _base_dir: Option<PathBuf>,
}

impl ToolTraceEmitter {
    /// Create a new tool-trace emitter.
    pub fn new(
        inner: Box<dyn StreamEmitter>,
        trace_storage: ToolTraceStorage,
        session_id: impl Into<String>,
        turn_id: impl Into<String>,
    ) -> Self {
        Self {
            inner,
            trace_storage,
            session_id: session_id.into(),
            turn_id: turn_id.into(),
            tool_start_times: HashMap::new(),
            _base_dir: crate::paths::ensure_restflow_dir().ok(),
        }
    }

    fn append_event(&self, event: ToolTrace) {
        if let Err(error) = self.trace_storage.append(&event) {
            warn!(
                session_id = %self.session_id,
                turn_id = %self.turn_id,
                error = %error,
                "Failed to append tool trace event"
            );
        }
    }
}

#[async_trait]
impl StreamEmitter for ToolTraceEmitter {
    async fn emit_text_delta(&mut self, text: &str) {
        self.inner.emit_text_delta(text).await;
    }

    async fn emit_thinking_delta(&mut self, text: &str) {
        self.inner.emit_thinking_delta(text).await;
    }

    async fn emit_tool_call_start(&mut self, id: &str, name: &str, arguments: &str) {
        self.inner.emit_tool_call_start(id, name, arguments).await;
        self.tool_start_times.insert(id.to_string(), Instant::now());
        let event = ToolTrace::tool_call_started(
            self.session_id.clone(),
            self.turn_id.clone(),
            id.to_string(),
            name.to_string(),
            normalize_payload(arguments),
        );
        self.append_event(event);
    }

    async fn emit_tool_call_result(&mut self, id: &str, name: &str, result: &str, success: bool) {
        self.inner
            .emit_tool_call_result(id, name, result, success)
            .await;

        let duration_ms = self
            .tool_start_times
            .remove(id)
            .map(|start| start.elapsed().as_millis() as u64);
        let full_output = normalize_full_payload(result);
        let output = full_output
            .as_deref()
            .map(|value| truncate_text(value, MAX_EVENT_TEXT_CHARS));
        let output_ref = full_output.as_deref().and_then(|value| {
            maybe_persist_full_output(&self.session_id, &self.turn_id, id, name, value)
        });
        let error = if success {
            None
        } else {
            Some(
                output
                    .clone()
                    .unwrap_or_else(|| "tool call failed".to_string()),
            )
        };
        let event = ToolTrace::tool_call_completed(
            self.session_id.clone(),
            self.turn_id.clone(),
            id.to_string(),
            name.to_string(),
            ToolCallCompletion {
                output,
                output_ref,
                success,
                duration_ms,
                error,
            },
        );
        self.append_event(event);
    }

    async fn emit_complete(&mut self) {
        self.inner.emit_complete().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::Database;
    use restflow_ai::agent::NullEmitter;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn setup_storage() -> ToolTraceStorage {
        let temp_dir = tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).expect("db"));
        ToolTraceStorage::new(db).expect("storage")
    }

    #[tokio::test]
    async fn test_tool_trace_emitter_writes_tool_events() {
        let storage = setup_storage();
        let mut emitter = ToolTraceEmitter::new(
            Box::new(NullEmitter),
            storage.clone(),
            "session-1",
            "turn-1",
        );

        emitter
            .emit_tool_call_start("call-1", "bash", "{\"cmd\":\"echo hi\"}")
            .await;
        emitter
            .emit_tool_call_result("call-1", "bash", "{\"ok\":true}", true)
            .await;

        let events = storage
            .list_by_session_turn("session-1", "turn-1", None)
            .expect("list");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].tool_name.as_deref(), Some("bash"));
        assert_eq!(events[1].success, Some(true));
    }
}
