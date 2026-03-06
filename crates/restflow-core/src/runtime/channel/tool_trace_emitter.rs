use crate::models::chat_session::ExecutionStepInfo;
use crate::models::{
    ExecutionTraceEvent, LifecycleTrace, MessageTrace, ToolCallCompletion, ToolCallTrace,
    ToolTrace, ToolTraceEvent,
};
use crate::storage::{ExecutionTraceStorage, ToolTraceStorage};
use async_trait::async_trait;
use regex::Regex;
use restflow_ai::agent::{
    NullEmitter, StreamEmitter, SubagentResult, SubagentTraceContext, SubagentTraceSink,
};
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
    append_turn_completed(tool_trace_storage, session_id, turn_id);
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
    append_turn_failed(tool_trace_storage, session_id, turn_id, error_text);
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
    append_turn_cancelled(tool_trace_storage, session_id, turn_id, reason);
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

fn subagent_trace_session_id(context: &SubagentTraceContext) -> String {
    context
        .parent_execution_id
        .clone()
        .unwrap_or_else(|| context.task_id.clone())
}

fn subagent_trace_turn_id(context: &SubagentTraceContext) -> String {
    format!("subagent-{}", context.task_id)
}

/// Sub-agent trace sink that persists lifecycle and tool-call events using
/// the existing tool/execution trace storages.
#[derive(Clone)]
pub struct ToolTraceSubagentSink {
    tool_trace_storage: ToolTraceStorage,
    execution_trace_storage: Option<ExecutionTraceStorage>,
}

impl ToolTraceSubagentSink {
    pub fn new(
        tool_trace_storage: ToolTraceStorage,
        execution_trace_storage: Option<ExecutionTraceStorage>,
    ) -> Self {
        Self {
            tool_trace_storage,
            execution_trace_storage,
        }
    }
}

impl SubagentTraceSink for ToolTraceSubagentSink {
    fn on_subagent_started(&self, context: &SubagentTraceContext) {
        let session_id = subagent_trace_session_id(context);
        let turn_id = subagent_trace_turn_id(context);
        append_turn_started_with_execution(
            &self.tool_trace_storage,
            self.execution_trace_storage.as_ref(),
            &session_id,
            &turn_id,
            &context.task_id,
            &context.agent_name,
        );
    }

    fn build_subagent_emitter(&self, context: &SubagentTraceContext) -> Box<dyn StreamEmitter> {
        let session_id = subagent_trace_session_id(context);
        let turn_id = subagent_trace_turn_id(context);
        let emitter = ToolTraceEmitter::new(
            Box::new(NullEmitter),
            self.tool_trace_storage.clone(),
            session_id,
            turn_id,
        );
        if let Some(storage) = self.execution_trace_storage.as_ref() {
            Box::new(emitter.with_execution_trace_context(
                storage.clone(),
                context.task_id.clone(),
                context.agent_name.clone(),
            ))
        } else {
            Box::new(emitter)
        }
    }

    fn on_subagent_finished(&self, context: &SubagentTraceContext, result: &SubagentResult) {
        let session_id = subagent_trace_session_id(context);
        let turn_id = subagent_trace_turn_id(context);
        if result.success {
            append_turn_completed_with_execution(
                &self.tool_trace_storage,
                self.execution_trace_storage.as_ref(),
                &session_id,
                &turn_id,
                &context.task_id,
                &context.agent_name,
            );
            return;
        }

        let error_text = result
            .error
            .as_deref()
            .unwrap_or("Sub-agent execution failed");
        append_turn_failed_with_execution(
            &self.tool_trace_storage,
            self.execution_trace_storage.as_ref(),
            &session_id,
            &turn_id,
            &context.task_id,
            &context.agent_name,
            error_text,
        );
    }
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
    session_id: String,
    turn_id: String,
    tool_start_times: HashMap<String, Instant>,
    tool_inputs: HashMap<String, Option<String>>,
    execution_task_id: Option<String>,
    execution_agent_id: Option<String>,
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
            execution_trace_storage: None,
            session_id: session_id.into(),
            turn_id: turn_id.into(),
            tool_start_times: HashMap::new(),
            tool_inputs: HashMap::new(),
            execution_task_id: None,
            execution_agent_id: None,
            _base_dir: crate::paths::ensure_restflow_dir().ok(),
        }
    }

    /// Attach execution trace context for dual-write into execution trace storage.
    pub fn with_execution_trace_context(
        mut self,
        execution_trace_storage: ExecutionTraceStorage,
        task_id: impl Into<String>,
        agent_id: impl Into<String>,
    ) -> Self {
        self.execution_trace_storage = Some(execution_trace_storage);
        self.execution_task_id = Some(task_id.into());
        self.execution_agent_id = Some(agent_id.into());
        self
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
        self.tool_inputs
            .insert(id.to_string(), normalize_payload(arguments));
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
        let input_summary = self.tool_inputs.remove(id).flatten();
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
                error: error.clone(),
            },
        );
        self.append_event(event);

        if let (Some(execution_storage), Some(task_id), Some(agent_id)) = (
            self.execution_trace_storage.as_ref(),
            self.execution_task_id.as_deref(),
            self.execution_agent_id.as_deref(),
        ) {
            let trace = ToolCallTrace {
                tool_name: name.to_string(),
                input_summary,
                success,
                error,
                duration_ms: duration_ms.map(|value| value as i64),
            };
            let trace_event = ExecutionTraceEvent::tool_call(task_id, agent_id, trace);
            if let Err(err) = execution_storage.store(&trace_event) {
                warn!(
                    task_id,
                    agent_id,
                    tool_call_id = id,
                    tool_name = name,
                    error = %err,
                    "Failed to append tool-call execution trace"
                );
            }
        }
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

    #[tokio::test]
    async fn test_tool_trace_subagent_sink_writes_lifecycle_and_tool_events() {
        let storage = setup_storage();
        let sink = ToolTraceSubagentSink::new(storage.clone(), None);
        let context = SubagentTraceContext {
            task_id: "task-1".to_string(),
            agent_name: "worker".to_string(),
            parent_execution_id: Some("parent-1".to_string()),
        };

        sink.on_subagent_started(&context);

        let mut emitter = sink.build_subagent_emitter(&context);
        emitter
            .emit_tool_call_start("call-1", "bash", "{\"cmd\":\"echo hi\"}")
            .await;
        emitter
            .emit_tool_call_result("call-1", "bash", "{\"ok\":true}", true)
            .await;

        sink.on_subagent_finished(
            &context,
            &SubagentResult {
                success: true,
                output: "ok".to_string(),
                summary: None,
                duration_ms: 10,
                tokens_used: Some(1),
                cost_usd: None,
                error: None,
            },
        );

        let events = storage
            .list_by_session_turn("parent-1", "subagent-task-1", None)
            .expect("list");
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].event_type, ToolTraceEvent::TurnStarted);
        assert_eq!(events[1].event_type, ToolTraceEvent::ToolCallStarted);
        assert_eq!(events[2].event_type, ToolTraceEvent::ToolCallCompleted);
        assert_eq!(events[3].event_type, ToolTraceEvent::TurnCompleted);
    }
}
