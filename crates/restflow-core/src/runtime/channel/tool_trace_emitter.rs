use crate::models::chat_session::ExecutionStepInfo;
use crate::models::{
    ExecutionTraceEvent, LifecycleTrace, MessageTrace, ToolTrace, ToolTraceEvent,
};
use crate::runtime::trace::{RestflowTrace, append_trace_event};
use crate::storage::{ExecutionTraceStorage, ToolTraceStorage};
use async_trait::async_trait;
use regex::Regex;
use restflow_ai::agent::StreamEmitter;
use restflow_trace::TraceEvent;
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

/// Build execution step info from completed tool traces.
pub fn build_execution_steps(traces: &[ToolTrace]) -> Vec<ExecutionStepInfo> {
    traces
        .iter()
        .filter(|t| t.event_type == ToolTraceEvent::ToolCallCompleted)
        .map(|t| {
            let status = if t.success.unwrap_or(true) {
                "completed"
            } else {
                "failed"
            };
            let tool_name = t
                .tool_name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .unwrap_or("unknown_tool");
            let mut step =
                ExecutionStepInfo::new("tool_call", tool_name.to_string()).with_status(status);
            if let Some(ms) = t.duration_ms {
                step = step.with_duration(ms);
            }
            step
        })
        .collect()
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
    append_turn_completed_with_ai_duration(storage, session_id, turn_id, None);
}

/// Append turn-completed event to tool trace storage with optional AI duration.
pub fn append_turn_completed_with_ai_duration(
    storage: &ToolTraceStorage,
    session_id: &str,
    turn_id: &str,
    ai_duration_ms: Option<u64>,
) {
    let event = ToolTrace::turn_completed(session_id, turn_id);
    let mut event = event;
    event.duration_ms = ai_duration_ms;
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
    append_turn_failed_with_ai_duration(storage, session_id, turn_id, error_text, None);
}

/// Append turn-failed event to tool trace storage with optional AI duration.
pub fn append_turn_failed_with_ai_duration(
    storage: &ToolTraceStorage,
    session_id: &str,
    turn_id: &str,
    error_text: &str,
    ai_duration_ms: Option<u64>,
) {
    let event = ToolTrace::turn_failed(
        session_id,
        turn_id,
        truncate_text(&sanitize_secrets(error_text), MAX_EVENT_TEXT_CHARS),
    );
    let mut event = event;
    event.duration_ms = ai_duration_ms;
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
    append_turn_cancelled_with_ai_duration(storage, session_id, turn_id, reason, None);
}

/// Append turn-cancelled event to tool trace storage with optional AI duration.
pub fn append_turn_cancelled_with_ai_duration(
    storage: &ToolTraceStorage,
    session_id: &str,
    turn_id: &str,
    reason: &str,
    ai_duration_ms: Option<u64>,
) {
    let event = ToolTrace::turn_cancelled(
        session_id,
        turn_id,
        truncate_text(&sanitize_secrets(reason), MAX_EVENT_TEXT_CHARS),
    );
    let mut event = event;
    event.duration_ms = ai_duration_ms;
    if let Err(error) = storage.append(&event) {
        warn!(
            session_id,
            turn_id,
            error = %error,
            "Failed to append turn cancelled event"
        );
    }
}

fn append_lifecycle_trace(
    storage: Option<&ExecutionTraceStorage>,
    task_id: &str,
    agent_id: &str,
    status: &str,
    message: Option<String>,
    error: Option<String>,
) {
    let Some(storage) = storage else {
        return;
    };

    let event = ExecutionTraceEvent::lifecycle(
        task_id,
        agent_id,
        LifecycleTrace {
            status: status.to_string(),
            message,
            error,
        },
    );
    if let Err(err) = storage.store(&event) {
        warn!(
            task_id,
            agent_id,
            error = %err,
            "Failed to append lifecycle execution trace"
        );
    }
}

/// Append turn-started events to both tool trace and execution trace storages.
pub fn append_turn_started_with_execution(
    tool_trace_storage: &ToolTraceStorage,
    execution_trace_storage: Option<&ExecutionTraceStorage>,
    session_id: &str,
    turn_id: &str,
    task_id: &str,
    agent_id: &str,
) {
    append_turn_started(tool_trace_storage, session_id, turn_id);
    append_lifecycle_trace(
        execution_trace_storage,
        task_id,
        agent_id,
        "turn_started",
        Some(format!("Turn started: {}", turn_id)),
        None,
    );
}

/// Append turn-completed events to both tool trace and execution trace storages.
pub fn append_turn_completed_with_execution(
    tool_trace_storage: &ToolTraceStorage,
    execution_trace_storage: Option<&ExecutionTraceStorage>,
    session_id: &str,
    turn_id: &str,
    task_id: &str,
    agent_id: &str,
) {
    append_turn_completed_with_execution_and_ai_duration(
        tool_trace_storage,
        execution_trace_storage,
        session_id,
        turn_id,
        task_id,
        agent_id,
        None,
    );
}

/// Append turn-completed events to both tool trace and execution trace storages
/// with optional AI duration.
pub fn append_turn_completed_with_execution_and_ai_duration(
    tool_trace_storage: &ToolTraceStorage,
    execution_trace_storage: Option<&ExecutionTraceStorage>,
    session_id: &str,
    turn_id: &str,
    task_id: &str,
    agent_id: &str,
    ai_duration_ms: Option<u64>,
) {
    append_turn_completed_with_ai_duration(tool_trace_storage, session_id, turn_id, ai_duration_ms);
    append_lifecycle_trace(
        execution_trace_storage,
        task_id,
        agent_id,
        "turn_completed",
        Some(format!("Turn completed: {}", turn_id)),
        None,
    );
}

/// Append turn-failed events to both tool trace and execution trace storages.
pub fn append_turn_failed_with_execution(
    tool_trace_storage: &ToolTraceStorage,
    execution_trace_storage: Option<&ExecutionTraceStorage>,
    session_id: &str,
    turn_id: &str,
    task_id: &str,
    agent_id: &str,
    error_text: &str,
) {
    append_turn_failed_with_execution_and_ai_duration(
        tool_trace_storage,
        execution_trace_storage,
        session_id,
        turn_id,
        task_id,
        agent_id,
        error_text,
        None,
    );
}

/// Append turn-failed events to both tool trace and execution trace storages with optional AI duration.
#[allow(clippy::too_many_arguments)]
pub fn append_turn_failed_with_execution_and_ai_duration(
    tool_trace_storage: &ToolTraceStorage,
    execution_trace_storage: Option<&ExecutionTraceStorage>,
    session_id: &str,
    turn_id: &str,
    task_id: &str,
    agent_id: &str,
    error_text: &str,
    ai_duration_ms: Option<u64>,
) {
    append_turn_failed_with_ai_duration(
        tool_trace_storage,
        session_id,
        turn_id,
        error_text,
        ai_duration_ms,
    );
    let sanitized_error = truncate_text(&sanitize_secrets(error_text), MAX_EVENT_TEXT_CHARS);
    append_lifecycle_trace(
        execution_trace_storage,
        task_id,
        agent_id,
        "turn_failed",
        Some(format!("Turn failed: {}", turn_id)),
        Some(sanitized_error),
    );
}

/// Append turn-cancelled events to both tool trace and execution trace storages.
pub fn append_turn_cancelled_with_execution(
    tool_trace_storage: &ToolTraceStorage,
    execution_trace_storage: Option<&ExecutionTraceStorage>,
    session_id: &str,
    turn_id: &str,
    task_id: &str,
    agent_id: &str,
    reason: &str,
) {
    append_turn_cancelled_with_execution_and_ai_duration(
        tool_trace_storage,
        execution_trace_storage,
        session_id,
        turn_id,
        task_id,
        agent_id,
        reason,
        None,
    );
}

/// Append turn-cancelled events to both tool trace and execution trace storages with optional AI duration.
#[allow(clippy::too_many_arguments)]
pub fn append_turn_cancelled_with_execution_and_ai_duration(
    tool_trace_storage: &ToolTraceStorage,
    execution_trace_storage: Option<&ExecutionTraceStorage>,
    session_id: &str,
    turn_id: &str,
    task_id: &str,
    agent_id: &str,
    reason: &str,
    ai_duration_ms: Option<u64>,
) {
    append_turn_cancelled_with_ai_duration(
        tool_trace_storage,
        session_id,
        turn_id,
        reason,
        ai_duration_ms,
    );
    let sanitized_reason = truncate_text(&sanitize_secrets(reason), MAX_EVENT_TEXT_CHARS);
    append_lifecycle_trace(
        execution_trace_storage,
        task_id,
        agent_id,
        "turn_cancelled",
        Some(format!("Turn cancelled: {}", turn_id)),
        Some(sanitized_reason),
    );
}

/// Append message trace event to execution trace storage.
pub fn append_message_trace(
    storage: &ExecutionTraceStorage,
    task_id: &str,
    agent_id: &str,
    role: &str,
    content: &str,
) {
    let content_preview = normalize_payload(content);
    let event = ExecutionTraceEvent::message(
        task_id,
        agent_id,
        MessageTrace {
            role: role.to_string(),
            content_preview,
            tool_call_count: None,
        },
    );

    if let Err(err) = storage.store(&event) {
        warn!(
            task_id,
            agent_id,
            role,
            error = %err,
            "Failed to append message execution trace"
        );
    }
}

/// Stream emitter that forwards events and persists tool call records to storage.
pub struct ToolTraceEmitter {
    inner: Box<dyn StreamEmitter>,
    trace_storage: ToolTraceStorage,
    execution_trace_storage: Option<ExecutionTraceStorage>,
    trace: RestflowTrace,
    tool_start_times: HashMap<String, Instant>,
    tool_inputs: HashMap<String, Option<String>>,
    _base_dir: Option<PathBuf>,
}

impl ToolTraceEmitter {
    /// Create a new tool-trace emitter.
    pub fn new(
        inner: Box<dyn StreamEmitter>,
        trace_storage: ToolTraceStorage,
        trace: RestflowTrace,
    ) -> Self {
        Self {
            inner,
            trace_storage,
            execution_trace_storage: None,
            trace,
            tool_start_times: HashMap::new(),
            tool_inputs: HashMap::new(),
            _base_dir: crate::paths::ensure_restflow_dir().ok(),
        }
    }

    /// Attach execution trace storage for dual-write.
    pub fn with_execution_trace_storage(
        mut self,
        execution_trace_storage: ExecutionTraceStorage,
    ) -> Self {
        self.execution_trace_storage = Some(execution_trace_storage);
        self
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
        let normalized_input = normalize_payload(arguments);
        self.tool_inputs
            .insert(id.to_string(), normalized_input.clone());
        let event = TraceEvent::tool_call_started(
            self.trace.clone(),
            id.to_string(),
            name.to_string(),
            normalized_input,
        );
        append_trace_event(
            &self.trace_storage,
            self.execution_trace_storage.as_ref(),
            &event,
        );
    }

    async fn emit_tool_call_result(&mut self, id: &str, name: &str, result: &str, success: bool) {
        self.inner
            .emit_tool_call_result(id, name, result, success)
            .await;

        let duration_ms = self
            .tool_start_times
            .remove(id)
            .map(|start| start.elapsed().as_millis() as u64);
        let input_summary = self.tool_inputs.remove(id).flatten();
        let full_output = normalize_full_payload(result);
        let output = full_output
            .as_deref()
            .map(|value| truncate_text(value, MAX_EVENT_TEXT_CHARS));
        let output_ref = full_output.as_deref().and_then(|value| {
            maybe_persist_full_output(&self.trace.session_id, &self.trace.turn_id, id, name, value)
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
        let event = TraceEvent::tool_call_completed(
            self.trace.clone(),
            id.to_string(),
            name.to_string(),
            input_summary,
            output,
            output_ref,
            success,
            duration_ms,
            error,
        );
        append_trace_event(
            &self.trace_storage,
            self.execution_trace_storage.as_ref(),
            &event,
        );
    }

    async fn emit_complete(&mut self) {
        self.inner.emit_complete().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ToolCallCompletion;
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
            RestflowTrace::new("run-1", "session-1", "task-1", "agent-1"),
        );

        emitter
            .emit_tool_call_start("call-1", "bash", "{\"cmd\":\"echo hi\"}")
            .await;
        emitter
            .emit_tool_call_result("call-1", "bash", "{\"ok\":true}", true)
            .await;

        let events = storage
            .list_by_session_turn("session-1", "run-run-1", None)
            .expect("list");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].tool_name.as_deref(), Some("bash"));
        assert_eq!(events[1].success, Some(true));
    }

    #[test]
    fn test_build_execution_steps_filters_and_maps_completed_tool_calls() {
        let traces = vec![
            ToolTrace::turn_started("session-1", "turn-1"),
            ToolTrace::tool_call_completed(
                "session-1",
                "turn-1",
                "call-1",
                "web_search",
                ToolCallCompletion {
                    output: None,
                    output_ref: None,
                    success: true,
                    duration_ms: Some(1250),
                    error: None,
                },
            ),
            ToolTrace::tool_call_completed(
                "session-1",
                "turn-1",
                "call-2",
                "transcribe",
                ToolCallCompletion {
                    output: None,
                    output_ref: None,
                    success: false,
                    duration_ms: None,
                    error: Some("tool failed".to_string()),
                },
            ),
        ];

        let steps = build_execution_steps(&traces);
        assert_eq!(steps.len(), 2);

        assert_eq!(steps[0].step_type, "tool_call");
        assert_eq!(steps[0].name, "web_search");
        assert_eq!(steps[0].status, "completed");
        assert_eq!(steps[0].duration_ms, Some(1250));

        assert_eq!(steps[1].step_type, "tool_call");
        assert_eq!(steps[1].name, "transcribe");
        assert_eq!(steps[1].status, "failed");
        assert_eq!(steps[1].duration_ms, None);
    }

    #[test]
    fn test_build_execution_steps_uses_unknown_tool_when_name_missing_or_blank() {
        let mut missing_name = ToolTrace::tool_call_completed(
            "session-1",
            "turn-1",
            "call-1",
            "placeholder",
            ToolCallCompletion {
                output: None,
                output_ref: None,
                success: true,
                duration_ms: None,
                error: None,
            },
        );
        missing_name.tool_name = None;

        let mut blank_name = ToolTrace::tool_call_completed(
            "session-1",
            "turn-1",
            "call-2",
            "placeholder",
            ToolCallCompletion {
                output: None,
                output_ref: None,
                success: false,
                duration_ms: Some(12),
                error: Some("failed".to_string()),
            },
        );
        blank_name.tool_name = Some("   ".to_string());

        let steps = build_execution_steps(&[missing_name, blank_name]);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].name, "unknown_tool");
        assert_eq!(steps[1].name, "unknown_tool");
    }
}
