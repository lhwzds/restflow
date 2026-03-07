//! Shared runtime helpers for RestFlow run traces.

use crate::models::{
    ExecutionTraceEvent, LifecycleTrace, MessageTrace, ToolCallCompletion, ToolCallTrace, ToolTrace,
};
use crate::runtime::channel::tool_trace_emitter::ToolTraceEmitter;
use crate::storage::{ExecutionTraceStorage, ToolTraceStorage};
use regex::Regex;
use restflow_ai::agent::{NullEmitter, RunTraceEmitterFactory, StreamEmitter};
pub use restflow_trace::{
    RestflowTrace, RunTraceContext, RunTraceLifecycleSink, RunTraceOutcome, TraceEvent,
    TraceEventKind, TraceEventSink, TraceMessage, TraceToolCallCompleted, TraceToolCallStart,
};
use std::sync::LazyLock;
use tracing::warn;

pub(crate) const MAX_TRACE_EVENT_TEXT_CHARS: usize = 10_000;

pub(crate) fn truncate_trace_text(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars {
        return value.to_string();
    }
    let mut truncated = value.chars().take(max_chars).collect::<String>();
    truncated.push_str("...");
    truncated
}

pub(crate) fn sanitize_trace_secrets(input: &str) -> String {
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

pub(crate) fn normalize_trace_payload(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let sanitized = sanitize_trace_secrets(trimmed);
    let normalized = match serde_json::from_str::<serde_json::Value>(&sanitized) {
        Ok(json) => json.to_string(),
        Err(_) => sanitized,
    };
    Some(truncate_trace_text(&normalized, MAX_TRACE_EVENT_TEXT_CHARS))
}

/// Persist one canonical trace lifecycle event into the existing storages.
pub fn append_trace_event(
    tool_trace_storage: &ToolTraceStorage,
    execution_trace_storage: Option<&ExecutionTraceStorage>,
    event: &TraceEvent,
) {
    match &event.kind {
        TraceEventKind::RunStarted => {
            append_restflow_trace_started(
                tool_trace_storage,
                execution_trace_storage,
                &event.trace,
            );
        }
        TraceEventKind::RunCompleted { ai_duration_ms } => {
            append_restflow_trace_completed(
                tool_trace_storage,
                execution_trace_storage,
                &event.trace,
                *ai_duration_ms,
            );
        }
        TraceEventKind::RunFailed {
            error,
            ai_duration_ms,
        } => {
            append_restflow_trace_failed(
                tool_trace_storage,
                execution_trace_storage,
                &event.trace,
                error,
                *ai_duration_ms,
            );
        }
        TraceEventKind::RunCancelled {
            reason,
            ai_duration_ms,
        } => {
            append_restflow_trace_cancelled(
                tool_trace_storage,
                execution_trace_storage,
                &event.trace,
                reason,
                *ai_duration_ms,
            );
        }
        TraceEventKind::ToolCallStarted(tool_call) => {
            append_restflow_tool_call_started(tool_trace_storage, &event.trace, tool_call);
        }
        TraceEventKind::ToolCallCompleted(tool_call) => {
            append_restflow_tool_call_completed(
                tool_trace_storage,
                execution_trace_storage,
                &event.trace,
                tool_call,
            );
        }
        TraceEventKind::Message(message) => {
            append_restflow_message(execution_trace_storage, &event.trace, message);
        }
    }
}

fn append_tool_trace_event(storage: &ToolTraceStorage, event: ToolTrace) {
    if let Err(error) = storage.append(&event) {
        warn!(
            session_id = %event.session_id,
            turn_id = %event.turn_id,
            error = %error,
            "Failed to append trace event"
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

/// Append turn-start event to tool trace storage.
pub fn append_turn_started(storage: &ToolTraceStorage, session_id: &str, turn_id: &str) {
    append_tool_trace_event(storage, ToolTrace::turn_started(session_id, turn_id));
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
    let mut event = ToolTrace::turn_completed(session_id, turn_id);
    event.duration_ms = ai_duration_ms;
    append_tool_trace_event(storage, event);
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
    let mut event = ToolTrace::turn_failed(
        session_id,
        turn_id,
        truncate_trace_text(
            &sanitize_trace_secrets(error_text),
            MAX_TRACE_EVENT_TEXT_CHARS,
        ),
    );
    event.duration_ms = ai_duration_ms;
    append_tool_trace_event(storage, event);
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
    let mut event = ToolTrace::turn_cancelled(
        session_id,
        turn_id,
        truncate_trace_text(&sanitize_trace_secrets(reason), MAX_TRACE_EVENT_TEXT_CHARS),
    );
    event.duration_ms = ai_duration_ms;
    append_tool_trace_event(storage, event);
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
    let sanitized_error = truncate_trace_text(
        &sanitize_trace_secrets(error_text),
        MAX_TRACE_EVENT_TEXT_CHARS,
    );
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
    let sanitized_reason =
        truncate_trace_text(&sanitize_trace_secrets(reason), MAX_TRACE_EVENT_TEXT_CHARS);
    append_lifecycle_trace(
        execution_trace_storage,
        task_id,
        agent_id,
        "turn_cancelled",
        Some(format!("Turn cancelled: {}", turn_id)),
        Some(sanitized_reason),
    );
}

/// Append run-started events for a canonical RestFlow trace.
pub fn append_restflow_trace_started(
    tool_trace_storage: &ToolTraceStorage,
    execution_trace_storage: Option<&ExecutionTraceStorage>,
    trace: &RestflowTrace,
) {
    append_turn_started_with_execution(
        tool_trace_storage,
        execution_trace_storage,
        &trace.session_id,
        &trace.turn_id,
        &trace.execution_task_id,
        &trace.actor_id,
    );
}

/// Append run-completed events for a canonical RestFlow trace.
pub fn append_restflow_trace_completed(
    tool_trace_storage: &ToolTraceStorage,
    execution_trace_storage: Option<&ExecutionTraceStorage>,
    trace: &RestflowTrace,
    ai_duration_ms: Option<u64>,
) {
    append_turn_completed_with_execution_and_ai_duration(
        tool_trace_storage,
        execution_trace_storage,
        &trace.session_id,
        &trace.turn_id,
        &trace.execution_task_id,
        &trace.actor_id,
        ai_duration_ms,
    );
}

/// Append run-failed events for a canonical RestFlow trace.
pub fn append_restflow_trace_failed(
    tool_trace_storage: &ToolTraceStorage,
    execution_trace_storage: Option<&ExecutionTraceStorage>,
    trace: &RestflowTrace,
    error_text: &str,
    ai_duration_ms: Option<u64>,
) {
    append_turn_failed_with_execution_and_ai_duration(
        tool_trace_storage,
        execution_trace_storage,
        &trace.session_id,
        &trace.turn_id,
        &trace.execution_task_id,
        &trace.actor_id,
        error_text,
        ai_duration_ms,
    );
}

/// Append run-cancelled events for a canonical RestFlow trace.
pub fn append_restflow_trace_cancelled(
    tool_trace_storage: &ToolTraceStorage,
    execution_trace_storage: Option<&ExecutionTraceStorage>,
    trace: &RestflowTrace,
    reason: &str,
    ai_duration_ms: Option<u64>,
) {
    append_turn_cancelled_with_execution_and_ai_duration(
        tool_trace_storage,
        execution_trace_storage,
        &trace.session_id,
        &trace.turn_id,
        &trace.execution_task_id,
        &trace.actor_id,
        reason,
        ai_duration_ms,
    );
}

/// Append tool-call start events for a canonical RestFlow trace.
pub fn append_restflow_tool_call_started(
    tool_trace_storage: &ToolTraceStorage,
    trace: &RestflowTrace,
    tool_call: &TraceToolCallStart,
) {
    append_tool_trace_event(
        tool_trace_storage,
        ToolTrace::tool_call_started(
            trace.session_id.clone(),
            trace.turn_id.clone(),
            tool_call.tool_call_id.clone(),
            tool_call.tool_name.clone(),
            tool_call.input.clone(),
        ),
    );
}

/// Append tool-call completion events for a canonical RestFlow trace.
pub fn append_restflow_tool_call_completed(
    tool_trace_storage: &ToolTraceStorage,
    execution_trace_storage: Option<&ExecutionTraceStorage>,
    trace: &RestflowTrace,
    tool_call: &TraceToolCallCompleted,
) {
    append_tool_trace_event(
        tool_trace_storage,
        ToolTrace::tool_call_completed(
            trace.session_id.clone(),
            trace.turn_id.clone(),
            tool_call.tool_call_id.clone(),
            tool_call.tool_name.clone(),
            ToolCallCompletion {
                output: tool_call.output.clone(),
                output_ref: tool_call.output_ref.clone(),
                success: tool_call.success,
                duration_ms: tool_call.duration_ms,
                error: tool_call.error.clone(),
            },
        ),
    );

    let Some(storage) = execution_trace_storage else {
        return;
    };

    let trace_event = ExecutionTraceEvent::tool_call(
        trace.execution_task_id.clone(),
        trace.actor_id.clone(),
        ToolCallTrace {
            tool_name: tool_call.tool_name.clone(),
            input_summary: tool_call.input_summary.clone(),
            success: tool_call.success,
            error: tool_call.error.clone(),
            duration_ms: tool_call.duration_ms.map(|value| value as i64),
        },
    );
    if let Err(error) = storage.store(&trace_event) {
        warn!(
            task_id = %trace.execution_task_id,
            agent_id = %trace.actor_id,
            tool_call_id = %tool_call.tool_call_id,
            tool_name = %tool_call.tool_name,
            error = %error,
            "Failed to append execution trace event"
        );
    }
}

/// Append message events for a canonical RestFlow trace.
pub fn append_restflow_message(
    execution_trace_storage: Option<&ExecutionTraceStorage>,
    trace: &RestflowTrace,
    message: &TraceMessage,
) {
    let Some(storage) = execution_trace_storage else {
        return;
    };

    let trace_event = ExecutionTraceEvent::message(
        trace.execution_task_id.clone(),
        trace.actor_id.clone(),
        MessageTrace {
            role: message.role.clone(),
            content_preview: message.content_preview.clone(),
            tool_call_count: message.tool_call_count,
        },
    );
    if let Err(error) = storage.store(&trace_event) {
        warn!(
            task_id = %trace.execution_task_id,
            agent_id = %trace.actor_id,
            role = %message.role,
            error = %error,
            "Failed to append message trace event"
        );
    }
}

/// Append a canonical message trace event through the shared trace adapter.
pub fn append_message_trace(
    tool_trace_storage: &ToolTraceStorage,
    execution_trace_storage: &ExecutionTraceStorage,
    trace: &RestflowTrace,
    role: &str,
    content: &str,
) {
    let content_preview = normalize_trace_payload(content);
    let event = TraceEvent::message(trace.clone(), role.to_string(), content_preview, None);
    append_trace_event(tool_trace_storage, Some(execution_trace_storage), &event);
}

/// Build a tool trace emitter bound to a canonical RestFlow trace.
pub fn build_restflow_trace_emitter(
    inner: Box<dyn StreamEmitter>,
    tool_trace_storage: ToolTraceStorage,
    execution_trace_storage: Option<ExecutionTraceStorage>,
    trace: &RestflowTrace,
) -> Box<dyn StreamEmitter> {
    let emitter = ToolTraceEmitter::new(inner, tool_trace_storage, trace.clone());
    if let Some(storage) = execution_trace_storage {
        Box::new(emitter.with_execution_trace_storage(storage))
    } else {
        Box::new(emitter)
    }
}

/// Run trace sink that persists lifecycle and tool-call events using
/// the existing tool/execution trace storages.
#[derive(Clone)]
pub struct ToolTraceRunSink {
    tool_trace_storage: ToolTraceStorage,
    execution_trace_storage: Option<ExecutionTraceStorage>,
}

impl ToolTraceRunSink {
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

impl TraceEventSink for ToolTraceRunSink {
    fn record_trace_event(&self, event: &TraceEvent) {
        append_trace_event(
            &self.tool_trace_storage,
            self.execution_trace_storage.as_ref(),
            &event,
        );
    }
}

impl RunTraceEmitterFactory for ToolTraceRunSink {
    fn build_run_emitter(&self, context: &RunTraceContext) -> Box<dyn StreamEmitter> {
        let trace = RestflowTrace::from_context(context, None);
        build_restflow_trace_emitter(
            Box::new(NullEmitter),
            self.tool_trace_storage.clone(),
            self.execution_trace_storage.clone(),
            &trace,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ExecutionTraceCategory, ToolTraceEvent};
    use crate::storage::ToolTraceStorage;
    use redb::Database;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn setup_storage() -> ToolTraceStorage {
        let temp_dir = tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).expect("db"));
        ToolTraceStorage::new(db).expect("storage")
    }

    #[tokio::test]
    async fn test_tool_trace_run_sink_writes_lifecycle_and_tool_events() {
        let storage = setup_storage();
        let sink = ToolTraceRunSink::new(storage.clone(), None);
        let context = RunTraceContext {
            run_id: "run-1".to_string(),
            actor_id: "worker".to_string(),
            parent_run_id: Some("parent-1".to_string()),
        };

        sink.on_run_started(&context);

        let mut emitter = sink.build_run_emitter(&context);
        emitter
            .emit_tool_call_start("call-1", "bash", "{\"cmd\":\"echo hi\"}")
            .await;
        emitter
            .emit_tool_call_result("call-1", "bash", "{\"ok\":true}", true)
            .await;

        sink.on_run_finished(
            &context,
            &RunTraceOutcome {
                success: true,
                error: None,
            },
        );

        let events = storage
            .list_by_session_turn("parent-1", "run-run-1", None)
            .expect("list");
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].event_type, ToolTraceEvent::TurnStarted);
        assert_eq!(events[1].event_type, ToolTraceEvent::ToolCallStarted);
        assert_eq!(events[2].event_type, ToolTraceEvent::ToolCallCompleted);
        assert_eq!(events[3].event_type, ToolTraceEvent::TurnCompleted);
    }

    #[test]
    fn test_restflow_trace_records_ai_duration_separately_from_event_time() {
        let storage = setup_storage();
        let trace = RestflowTrace::new("run-a", "session-a", "task-a", "agent-a");
        append_restflow_trace_started(&storage, None, &trace);
        append_restflow_trace_completed(&storage, None, &trace, Some(321));

        let events = storage
            .list_by_session_turn("session-a", "run-run-a", None)
            .expect("list");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, ToolTraceEvent::TurnStarted);
        assert_eq!(events[1].event_type, ToolTraceEvent::TurnCompleted);
        assert_eq!(events[1].duration_ms, Some(321));
        assert!(events[1].created_at >= trace.created_at_ms);
    }

    #[test]
    fn test_append_trace_event_persists_cancelled_lifecycle() {
        let storage = setup_storage();
        let event = TraceEvent::run_cancelled(
            RestflowTrace::new("run-c", "session-c", "task-c", "agent-c"),
            "cancelled",
            Some(77),
        );

        append_trace_event(&storage, None, &event);

        let events = storage
            .list_by_session_turn("session-c", "run-run-c", None)
            .expect("list");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, ToolTraceEvent::TurnCancelled);
        assert_eq!(events[0].duration_ms, Some(77));
        assert_eq!(events[0].error.as_deref(), Some("cancelled"));
    }

    #[test]
    fn test_append_trace_event_persists_tool_call_completion() {
        let storage = setup_storage();
        let temp_dir = tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("execution.db");
        let db = Arc::new(Database::create(db_path).expect("db"));
        let execution_storage = ExecutionTraceStorage::new(db).expect("execution storage");
        let event = TraceEvent::tool_call_completed(
            RestflowTrace::new("run-t", "session-t", "task-t", "agent-t"),
            "call-1",
            "bash",
            Some("{\"cmd\":\"echo hi\"}".to_string()),
            Some("{\"ok\":true}".to_string()),
            None,
            true,
            Some(11),
            None,
        );

        append_trace_event(&storage, Some(&execution_storage), &event);

        let tool_events = storage
            .list_by_session_turn("session-t", "run-run-t", None)
            .expect("list");
        assert_eq!(tool_events.len(), 1);
        assert_eq!(tool_events[0].event_type, ToolTraceEvent::ToolCallCompleted);
        assert_eq!(tool_events[0].tool_name.as_deref(), Some("bash"));
        assert_eq!(tool_events[0].duration_ms, Some(11));

        let execution_events = execution_storage
            .query(&crate::models::ExecutionTraceQuery {
                task_id: Some("task-t".to_string()),
                ..Default::default()
            })
            .expect("query");
        assert_eq!(execution_events.len(), 1);
        assert_eq!(
            execution_events[0].category,
            ExecutionTraceCategory::ToolCall
        );
        assert_eq!(
            execution_events[0]
                .tool_call
                .as_ref()
                .and_then(|trace| trace.input_summary.as_deref()),
            Some("{\"cmd\":\"echo hi\"}")
        );
    }

    #[test]
    fn test_append_trace_event_persists_message() {
        let storage = setup_storage();
        let temp_dir = tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("execution.db");
        let db = Arc::new(Database::create(db_path).expect("db"));
        let execution_storage = ExecutionTraceStorage::new(db).expect("execution storage");
        let event = TraceEvent::message(
            RestflowTrace::new("run-m", "session-m", "task-m", "agent-m"),
            "assistant",
            Some("hello".to_string()),
            Some(1),
        );

        append_trace_event(&storage, Some(&execution_storage), &event);

        let execution_events = execution_storage
            .query(&crate::models::ExecutionTraceQuery {
                task_id: Some("task-m".to_string()),
                ..Default::default()
            })
            .expect("query");
        assert_eq!(execution_events.len(), 1);
        assert_eq!(
            execution_events[0].category,
            ExecutionTraceCategory::Message
        );
        assert_eq!(
            execution_events[0].message.as_ref().map(|trace| (
                trace.role.as_str(),
                trace.content_preview.as_deref(),
                trace.tool_call_count
            )),
            Some(("assistant", Some("hello"), Some(1)))
        );
    }
}
