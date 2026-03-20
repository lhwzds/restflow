//! Shared runtime helpers for RestFlow run traces.

use crate::models::{
    ExecutionTraceEvent, LifecycleTrace, LlmCallTrace, MessageTrace, ModelSwitchTrace,
    ToolCallCompletion, ToolCallTrace, ToolTrace,
};
use crate::runtime::channel::tool_trace_emitter::ToolTraceEmitter;
use crate::storage::{ExecutionTraceStorage, ToolTraceStorage};
use anyhow::Result;
use regex::Regex;
use restflow_ai::agent::{NullEmitter, RunTraceEmitterFactory, StreamEmitter};
pub use restflow_trace::{
    RestflowTrace, RunTraceContext, RunTraceLifecycleSink, RunTraceOutcome, RunTraceSummary,
    RunTraceTimeline, TraceArtifactPreview, TraceEvent, TraceEventKind, TraceEventSink,
    TraceLlmCall, TraceMessage, TraceTimelineEvent, TraceTimelineSource, TraceToolCallCompleted,
    TraceToolCallStart,
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

fn execution_trace_path(trace: &RestflowTrace) -> Vec<String> {
    let mut path = Vec::new();
    if let Some(parent_run_id) = trace.parent_run_id.as_ref()
        && !parent_run_id.trim().is_empty()
    {
        path.push(parent_run_id.clone());
    }
    path.push(trace.run_id.clone());
    path
}

fn with_execution_trace_path(
    event: ExecutionTraceEvent,
    trace: &RestflowTrace,
) -> ExecutionTraceEvent {
    event.with_subflow_path(execution_trace_path(trace))
}

fn merge_trace_identity(existing: &mut RestflowTrace, candidate: &RestflowTrace) {
    if existing.parent_run_id.is_none() && candidate.parent_run_id.is_some() {
        existing.parent_run_id = candidate.parent_run_id.clone();
    }
    if existing.actor_id == "unknown" && candidate.actor_id != "unknown" {
        existing.actor_id = candidate.actor_id.clone();
    }
    if existing.created_at_ms > candidate.created_at_ms {
        existing.created_at_ms = candidate.created_at_ms;
    }
}

fn trace_from_legacy_turn(
    session_id: &str,
    turn_id: &str,
    task_id: &str,
    agent_id: &str,
) -> RestflowTrace {
    let run_id = turn_id
        .strip_prefix("run-")
        .map(str::to_string)
        .unwrap_or_else(|| turn_id.to_string());
    RestflowTrace::from_run(
        run_id,
        agent_id.to_string(),
        None,
        Some(session_id.to_string()),
        Some(task_id.to_string()),
    )
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
        TraceEventKind::RunInterrupted {
            reason,
            ai_duration_ms,
        } => {
            append_restflow_trace_interrupted(
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
        TraceEventKind::LlmCall(llm_call) => {
            append_restflow_llm_call(execution_trace_storage, &event.trace, llm_call);
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
    trace: &RestflowTrace,
    status: &str,
    message: Option<String>,
    error: Option<String>,
) {
    let Some(storage) = storage else {
        return;
    };

    let event = with_execution_trace_path(
        ExecutionTraceEvent::lifecycle(
            trace.scope_id.clone(),
            trace.actor_id.clone(),
            LifecycleTrace {
                status: status.to_string(),
                message,
                error,
            },
        ),
        trace,
    );
    if let Err(err) = storage.store(&event) {
        warn!(
            scope_id = %trace.scope_id,
            agent_id = %trace.actor_id,
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

/// Append turn-interrupted event to tool trace storage.
pub fn append_turn_interrupted(
    storage: &ToolTraceStorage,
    session_id: &str,
    turn_id: &str,
    reason: &str,
) {
    append_turn_interrupted_with_ai_duration(storage, session_id, turn_id, reason, None);
}

/// Append turn-interrupted event to tool trace storage with optional AI duration.
pub fn append_turn_interrupted_with_ai_duration(
    storage: &ToolTraceStorage,
    session_id: &str,
    turn_id: &str,
    reason: &str,
    ai_duration_ms: Option<u64>,
) {
    let mut event = ToolTrace::turn_interrupted(
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
    let trace = trace_from_legacy_turn(session_id, turn_id, task_id, agent_id);
    append_lifecycle_trace(
        execution_trace_storage,
        &trace,
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
    let trace = trace_from_legacy_turn(session_id, turn_id, task_id, agent_id);
    append_lifecycle_trace(
        execution_trace_storage,
        &trace,
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
    let trace = trace_from_legacy_turn(session_id, turn_id, task_id, agent_id);
    append_lifecycle_trace(
        execution_trace_storage,
        &trace,
        "turn_failed",
        Some(format!("Turn failed: {}", turn_id)),
        Some(sanitized_error),
    );
}

/// Append turn-interrupted events to both tool trace and execution trace storages.
pub fn append_turn_interrupted_with_execution(
    tool_trace_storage: &ToolTraceStorage,
    execution_trace_storage: Option<&ExecutionTraceStorage>,
    session_id: &str,
    turn_id: &str,
    task_id: &str,
    agent_id: &str,
    reason: &str,
) {
    append_turn_interrupted_with_execution_and_ai_duration(
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

/// Append turn-interrupted events to both tool trace and execution trace storages with optional AI duration.
#[allow(clippy::too_many_arguments)]
pub fn append_turn_interrupted_with_execution_and_ai_duration(
    tool_trace_storage: &ToolTraceStorage,
    execution_trace_storage: Option<&ExecutionTraceStorage>,
    session_id: &str,
    turn_id: &str,
    task_id: &str,
    agent_id: &str,
    reason: &str,
    ai_duration_ms: Option<u64>,
) {
    append_turn_interrupted_with_ai_duration(
        tool_trace_storage,
        session_id,
        turn_id,
        reason,
        ai_duration_ms,
    );
    let sanitized_reason =
        truncate_trace_text(&sanitize_trace_secrets(reason), MAX_TRACE_EVENT_TEXT_CHARS);
    let trace = trace_from_legacy_turn(session_id, turn_id, task_id, agent_id);
    append_lifecycle_trace(
        execution_trace_storage,
        &trace,
        "turn_interrupted",
        Some(format!("Turn interrupted: {}", turn_id)),
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
        &trace.scope_id,
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
        &trace.scope_id,
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
        &trace.scope_id,
        &trace.actor_id,
        error_text,
        ai_duration_ms,
    );
}

/// Append run-interrupted events for a canonical RestFlow trace.
pub fn append_restflow_trace_interrupted(
    tool_trace_storage: &ToolTraceStorage,
    execution_trace_storage: Option<&ExecutionTraceStorage>,
    trace: &RestflowTrace,
    reason: &str,
    ai_duration_ms: Option<u64>,
) {
    append_turn_interrupted_with_execution_and_ai_duration(
        tool_trace_storage,
        execution_trace_storage,
        &trace.session_id,
        &trace.turn_id,
        &trace.scope_id,
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

    let trace_event = with_execution_trace_path(
        ExecutionTraceEvent::tool_call(
            trace.scope_id.clone(),
            trace.actor_id.clone(),
            ToolCallTrace {
                tool_name: tool_call.tool_name.clone(),
                input_summary: tool_call.input_summary.clone(),
                success: tool_call.success,
                error: tool_call.error.clone(),
                duration_ms: tool_call.duration_ms.map(|value| value as i64),
            },
        ),
        trace,
    );
    if let Err(error) = storage.store(&trace_event) {
        warn!(
            scope_id = %trace.scope_id,
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

    let trace_event = with_execution_trace_path(
        ExecutionTraceEvent::message(
            trace.scope_id.clone(),
            trace.actor_id.clone(),
            MessageTrace {
                role: message.role.clone(),
                content_preview: message.content_preview.clone(),
                tool_call_count: message.tool_call_count,
            },
        ),
        trace,
    );
    if let Err(error) = storage.store(&trace_event) {
        warn!(
            scope_id = %trace.scope_id,
            agent_id = %trace.actor_id,
            role = %message.role,
            error = %error,
            "Failed to append message trace event"
        );
    }
}

/// Append LLM-call events for a canonical RestFlow trace.
pub fn append_restflow_llm_call(
    execution_trace_storage: Option<&ExecutionTraceStorage>,
    trace: &RestflowTrace,
    llm_call: &TraceLlmCall,
) {
    let Some(storage) = execution_trace_storage else {
        return;
    };

    let trace_event = with_execution_trace_path(
        ExecutionTraceEvent::llm_call(
            trace.scope_id.clone(),
            trace.actor_id.clone(),
            LlmCallTrace {
                model: llm_call.model.clone(),
                input_tokens: llm_call.input_tokens,
                output_tokens: llm_call.output_tokens,
                total_tokens: llm_call.total_tokens,
                cost_usd: llm_call.cost_usd,
                duration_ms: llm_call.duration_ms.map(|value| value as i64),
                is_reasoning: llm_call.is_reasoning,
                message_count: llm_call.message_count,
            },
        ),
        trace,
    );
    if let Err(error) = storage.store(&trace_event) {
        warn!(
            scope_id = %trace.scope_id,
            agent_id = %trace.actor_id,
            model = %llm_call.model,
            error = %error,
            "Failed to append llm trace event"
        );
    }
}

/// Append model-switch events for a canonical RestFlow trace.
pub fn append_restflow_model_switch(
    execution_trace_storage: Option<&ExecutionTraceStorage>,
    trace: &RestflowTrace,
    from_model: &str,
    to_model: &str,
    reason: Option<&str>,
    success: bool,
) {
    let Some(storage) = execution_trace_storage else {
        return;
    };

    let trace_event = with_execution_trace_path(
        ExecutionTraceEvent::model_switch(
            trace.scope_id.clone(),
            trace.actor_id.clone(),
            ModelSwitchTrace {
                from_model: from_model.to_string(),
                to_model: to_model.to_string(),
                reason: reason.map(str::to_string),
                success,
            },
        ),
        trace,
    );
    if let Err(error) = storage.store(&trace_event) {
        warn!(
            scope_id = %trace.scope_id,
            agent_id = %trace.actor_id,
            from_model = %from_model,
            to_model = %to_model,
            error = %error,
            "Failed to append model switch trace event"
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

fn run_id_from_turn_id(turn_id: &str) -> Option<&str> {
    turn_id.strip_prefix("run-")
}

fn run_id_from_execution_event(event: &ExecutionTraceEvent) -> Option<&str> {
    event.subflow_path.last().map(String::as_str)
}

fn parent_run_id_from_execution_event(event: &ExecutionTraceEvent) -> Option<&str> {
    if event.subflow_path.len() >= 2 {
        event
            .subflow_path
            .get(event.subflow_path.len().saturating_sub(2))
            .map(String::as_str)
    } else {
        None
    }
}

fn timeline_trace_from_execution_event(
    event: &ExecutionTraceEvent,
    session_id: &str,
) -> Option<RestflowTrace> {
    let run_id = run_id_from_execution_event(event)?.to_string();
    Some(RestflowTrace {
        turn_id: format!("run-{}", run_id),
        run_id,
        parent_run_id: parent_run_id_from_execution_event(event).map(str::to_string),
        session_id: session_id.to_string(),
        scope_id: event.task_id.clone(),
        actor_id: event.agent_id.clone(),
        created_at_ms: event.timestamp,
    })
}

fn timeline_trace_from_tool_event(
    event: &ToolTrace,
    scope_id: &str,
    fallback_actor_id: Option<&str>,
) -> Option<RestflowTrace> {
    let run_id = run_id_from_turn_id(&event.turn_id)?.to_string();
    Some(RestflowTrace {
        run_id,
        parent_run_id: None,
        session_id: event.session_id.clone(),
        turn_id: event.turn_id.clone(),
        scope_id: scope_id.to_string(),
        actor_id: fallback_actor_id.unwrap_or("unknown").to_string(),
        created_at_ms: event.created_at,
    })
}

fn read_artifact_preview(path: &str, line_limit: usize) -> Option<TraceArtifactPreview> {
    let content = std::fs::read_to_string(path).ok()?;
    let lines = content.lines().collect::<Vec<_>>();
    let total_lines = lines.len();
    let start = total_lines.saturating_sub(line_limit.max(1));
    Some(TraceArtifactPreview {
        path: path.to_string(),
        total_lines,
        lines: lines[start..]
            .iter()
            .map(|line| (*line).to_string())
            .collect::<Vec<_>>(),
    })
}

fn tool_event_to_timeline_record(
    trace: &RestflowTrace,
    tool_event: ToolTrace,
    artifact_line_limit: Option<usize>,
) -> Option<TraceTimelineEvent> {
    let event = match tool_event.event_type {
        crate::models::ToolTraceEvent::TurnStarted => TraceEvent::run_started(trace.clone()),
        crate::models::ToolTraceEvent::TurnCompleted => {
            TraceEvent::run_completed(trace.clone(), tool_event.duration_ms)
        }
        crate::models::ToolTraceEvent::TurnFailed => TraceEvent::run_failed(
            trace.clone(),
            tool_event
                .error
                .clone()
                .unwrap_or_else(|| "run failed".to_string()),
            tool_event.duration_ms,
        ),
        crate::models::ToolTraceEvent::TurnInterrupted => TraceEvent::run_interrupted(
            trace.clone(),
            tool_event
                .error
                .clone()
                .unwrap_or_else(|| "run interrupted".to_string()),
            tool_event.duration_ms,
        ),
        crate::models::ToolTraceEvent::ToolCallStarted => TraceEvent::tool_call_started(
            trace.clone(),
            tool_event.tool_call_id.clone()?,
            tool_event.tool_name.clone()?,
            tool_event.input.clone(),
        ),
        crate::models::ToolTraceEvent::ToolCallCompleted => TraceEvent::tool_call_completed(
            trace.clone(),
            tool_event.tool_call_id.clone()?,
            tool_event.tool_name.clone()?,
            None,
            tool_event.output.clone(),
            tool_event.output_ref.clone(),
            tool_event.success.unwrap_or(false),
            tool_event.duration_ms,
            tool_event.error.clone(),
        ),
    };

    let artifact_preview = artifact_line_limit.and_then(|limit| {
        tool_event
            .output_ref
            .as_deref()
            .and_then(|path| read_artifact_preview(path, limit))
    });

    Some(TraceTimelineEvent {
        record_id: Some(tool_event.id),
        timestamp_ms: tool_event.created_at,
        source: TraceTimelineSource::ToolTrace,
        event,
        artifact_preview,
    })
}

fn execution_event_to_timeline_record(
    trace: &RestflowTrace,
    execution_event: ExecutionTraceEvent,
) -> Option<TraceTimelineEvent> {
    let event = match execution_event.category {
        crate::models::ExecutionTraceCategory::LlmCall => {
            let llm = execution_event.llm_call.as_ref()?;
            TraceEvent::llm_call(
                trace.clone(),
                llm.model.clone(),
                llm.input_tokens,
                llm.output_tokens,
                llm.total_tokens,
                llm.cost_usd,
                llm.duration_ms.map(|value| value as u64),
                llm.is_reasoning,
                llm.message_count,
            )
        }
        crate::models::ExecutionTraceCategory::Message => {
            let message = execution_event.message.as_ref()?;
            TraceEvent::message(
                trace.clone(),
                message.role.clone(),
                message.content_preview.clone(),
                message.tool_call_count,
            )
        }
        _ => return None,
    };

    Some(TraceTimelineEvent {
        record_id: Some(execution_event.id),
        timestamp_ms: execution_event.timestamp,
        source: TraceTimelineSource::ExecutionTrace,
        event,
        artifact_preview: None,
    })
}

fn build_run_trace_summary(
    trace: RestflowTrace,
    events: &[TraceTimelineEvent],
) -> Option<RunTraceSummary> {
    events.first()?;
    let last_event = events.last()?;
    let mut status = "running".to_string();
    let mut started_at_ms = None;
    let mut ended_at_ms = None;
    let mut tool_call_count = 0usize;
    let mut message_count = 0usize;
    let mut llm_call_count = 0usize;

    for event in events {
        match &event.event.kind {
            TraceEventKind::RunStarted => {
                started_at_ms.get_or_insert(event.timestamp_ms);
            }
            TraceEventKind::RunCompleted { .. } => {
                status = "completed".to_string();
                ended_at_ms = Some(event.timestamp_ms);
            }
            TraceEventKind::RunFailed { .. } => {
                status = "failed".to_string();
                ended_at_ms = Some(event.timestamp_ms);
            }
            TraceEventKind::RunInterrupted { .. } => {
                status = "interrupted".to_string();
                ended_at_ms = Some(event.timestamp_ms);
            }
            TraceEventKind::ToolCallCompleted(_) => {
                tool_call_count += 1;
            }
            TraceEventKind::Message(_) => {
                message_count += 1;
            }
            TraceEventKind::LlmCall(_) => {
                llm_call_count += 1;
            }
            TraceEventKind::ToolCallStarted(_) => {}
        }
    }

    Some(RunTraceSummary {
        trace,
        status,
        started_at_ms,
        ended_at_ms,
        last_event_at_ms: last_event.timestamp_ms,
        event_count: events.len(),
        tool_call_count,
        message_count,
        llm_call_count,
    })
}

fn load_scope_run_timelines(
    tool_trace_storage: &ToolTraceStorage,
    execution_trace_storage: &ExecutionTraceStorage,
    session_id: &str,
    scope_id: &str,
    artifact_line_limit: Option<usize>,
) -> Result<Vec<RunTraceTimeline>> {
    use std::collections::{BTreeMap, HashMap};

    let tool_events = tool_trace_storage.list_by_session(session_id, None)?;
    let execution_events = execution_trace_storage.query(&crate::models::ExecutionTraceQuery {
        task_id: Some(scope_id.to_string()),
        limit: Some(usize::MAX),
        ..Default::default()
    })?;

    let mut run_traces = HashMap::<String, RestflowTrace>::new();
    for execution_event in &execution_events {
        if let Some(trace) = timeline_trace_from_execution_event(execution_event, session_id) {
            run_traces
                .entry(trace.run_id.clone())
                .and_modify(|existing| merge_trace_identity(existing, &trace))
                .or_insert(trace);
        }
    }

    for tool_event in &tool_events {
        let run_id = match run_id_from_turn_id(&tool_event.turn_id) {
            Some(run_id) => run_id.to_string(),
            None => continue,
        };
        let fallback_actor_id = run_traces.get(&run_id).map(|trace| trace.actor_id.as_str());
        if let Some(trace) = timeline_trace_from_tool_event(tool_event, scope_id, fallback_actor_id)
        {
            run_traces
                .entry(run_id)
                .and_modify(|existing| merge_trace_identity(existing, &trace))
                .or_insert(trace);
        }
    }

    let mut grouped = BTreeMap::<String, Vec<TraceTimelineEvent>>::new();
    for tool_event in tool_events {
        let Some(run_id) = run_id_from_turn_id(&tool_event.turn_id).map(str::to_string) else {
            continue;
        };
        let Some(trace) = run_traces.get(&run_id).cloned() else {
            continue;
        };
        if let Some(record) = tool_event_to_timeline_record(&trace, tool_event, artifact_line_limit)
        {
            grouped.entry(run_id).or_default().push(record);
        }
    }

    for execution_event in execution_events {
        let Some(run_id) = run_id_from_execution_event(&execution_event).map(str::to_string) else {
            continue;
        };
        let Some(trace) = run_traces.get(&run_id).cloned() else {
            continue;
        };
        if let Some(record) = execution_event_to_timeline_record(&trace, execution_event) {
            grouped.entry(run_id).or_default().push(record);
        }
    }

    let mut timelines = Vec::new();
    for (run_id, mut events) in grouped {
        events.sort_by(|a, b| {
            a.timestamp_ms
                .cmp(&b.timestamp_ms)
                .then_with(|| a.record_id.cmp(&b.record_id))
        });
        let Some(trace) = run_traces.get(&run_id).cloned() else {
            continue;
        };
        let Some(summary) = build_run_trace_summary(trace, &events) else {
            continue;
        };
        timelines.push(RunTraceTimeline { summary, events });
    }

    timelines.sort_by(|a, b| {
        b.summary
            .last_event_at_ms
            .cmp(&a.summary.last_event_at_ms)
            .then_with(|| b.summary.trace.run_id.cmp(&a.summary.trace.run_id))
    });
    Ok(timelines)
}

/// List run-trace summaries for one session/scope pair.
pub fn list_run_trace_summaries(
    tool_trace_storage: &ToolTraceStorage,
    execution_trace_storage: &ExecutionTraceStorage,
    session_id: &str,
    scope_id: &str,
    limit: usize,
) -> Result<Vec<RunTraceSummary>> {
    let mut timelines = load_scope_run_timelines(
        tool_trace_storage,
        execution_trace_storage,
        session_id,
        scope_id,
        None,
    )?;
    timelines.truncate(limit.max(1));
    Ok(timelines
        .into_iter()
        .map(|timeline| timeline.summary)
        .collect())
}

/// Read one run-trace timeline for one session/scope pair.
pub fn read_run_trace(
    tool_trace_storage: &ToolTraceStorage,
    execution_trace_storage: &ExecutionTraceStorage,
    session_id: &str,
    scope_id: &str,
    run_id: &str,
    artifact_line_limit: usize,
) -> Result<Option<RunTraceTimeline>> {
    let timelines = load_scope_run_timelines(
        tool_trace_storage,
        execution_trace_storage,
        session_id,
        scope_id,
        Some(artifact_line_limit.max(1)),
    )?;
    Ok(timelines
        .into_iter()
        .find(|timeline| timeline.summary.trace.run_id == run_id))
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
            event,
        );
    }
}

impl RunTraceEmitterFactory for ToolTraceRunSink {
    fn build_run_emitter(&self, context: &RunTraceContext) -> Box<dyn StreamEmitter> {
        let trace = RestflowTrace::from_context(context);
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
    use crate::storage::{ExecutionTraceStorage, ToolTraceStorage};
    use redb::Database;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn setup_storage() -> ToolTraceStorage {
        let temp_dir = tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).expect("db"));
        ToolTraceStorage::new(db).expect("storage")
    }

    fn setup_execution_storage() -> ExecutionTraceStorage {
        let temp_dir = tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("execution.db");
        let db = Arc::new(Database::create(db_path).expect("db"));
        ExecutionTraceStorage::new(db).expect("execution storage")
    }

    #[tokio::test]
    async fn test_tool_trace_run_sink_writes_lifecycle_and_tool_events() {
        let storage = setup_storage();
        let sink = ToolTraceRunSink::new(storage.clone(), None);
        let context = RunTraceContext {
            run_id: "run-1".to_string(),
            actor_id: "worker".to_string(),
            parent_run_id: Some("parent-1".to_string()),
            session_id: "session-1".to_string(),
            scope_id: "scope-1".to_string(),
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
            .list_by_session_turn("session-1", "run-run-1", None)
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
    fn test_append_trace_event_persists_interrupted_lifecycle() {
        let storage = setup_storage();
        let event = TraceEvent::run_interrupted(
            RestflowTrace::new("run-c", "session-c", "task-c", "agent-c"),
            "interrupted",
            Some(77),
        );

        append_trace_event(&storage, None, &event);

        let events = storage
            .list_by_session_turn("session-c", "run-run-c", None)
            .expect("list");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, ToolTraceEvent::TurnInterrupted);
        assert_eq!(events[0].duration_ms, Some(77));
        assert_eq!(events[0].error.as_deref(), Some("interrupted"));
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
        let execution_storage = setup_execution_storage();
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
        assert_eq!(execution_events[0].subflow_path, vec!["run-m".to_string()]);
    }

    #[test]
    fn test_append_trace_event_persists_llm_call() {
        let storage = setup_storage();
        let execution_storage = setup_execution_storage();
        let event = TraceEvent::llm_call(
            RestflowTrace::new("run-llm", "session-llm", "task-llm", "agent-llm"),
            "claude-sonnet",
            Some(10),
            Some(5),
            Some(15),
            Some(0.02),
            Some(250),
            Some(false),
            Some(3),
        );

        append_trace_event(&storage, Some(&execution_storage), &event);

        let execution_events = execution_storage
            .query(&crate::models::ExecutionTraceQuery {
                task_id: Some("task-llm".to_string()),
                ..Default::default()
            })
            .expect("query");
        assert_eq!(execution_events.len(), 1);
        assert_eq!(
            execution_events[0].category,
            ExecutionTraceCategory::LlmCall
        );
        assert_eq!(
            execution_events[0].llm_call.as_ref().map(|trace| (
                trace.model.as_str(),
                trace.total_tokens,
                trace.duration_ms
            )),
            Some(("claude-sonnet", Some(15), Some(250)))
        );
        assert_eq!(
            execution_events[0].subflow_path,
            vec!["run-llm".to_string()]
        );
    }

    #[test]
    fn test_list_and_read_run_trace_merge_tool_and_execution_events() {
        let storage = setup_storage();
        let execution_storage = setup_execution_storage();
        let trace = RestflowTrace::from_run(
            "child-run",
            "worker",
            Some("parent-run".to_string()),
            Some("session-1".to_string()),
            Some("scope-1".to_string()),
        );

        append_trace_event(
            &storage,
            Some(&execution_storage),
            &TraceEvent::run_started(trace.clone()),
        );
        append_trace_event(
            &storage,
            Some(&execution_storage),
            &TraceEvent::llm_call(
                trace.clone(),
                "gpt-5",
                Some(11),
                Some(7),
                Some(18),
                None,
                Some(120),
                None,
                Some(2),
            ),
        );
        append_trace_event(
            &storage,
            Some(&execution_storage),
            &TraceEvent::message(trace.clone(), "assistant", Some("hello".to_string()), None),
        );
        append_trace_event(
            &storage,
            Some(&execution_storage),
            &TraceEvent::tool_call_started(
                trace.clone(),
                "call-1",
                "bash",
                Some("{\"cmd\":\"echo hi\"}".to_string()),
            ),
        );
        append_trace_event(
            &storage,
            Some(&execution_storage),
            &TraceEvent::tool_call_completed(
                trace.clone(),
                "call-1",
                "bash",
                Some("{\"cmd\":\"echo hi\"}".to_string()),
                Some("{\"ok\":true}".to_string()),
                None,
                true,
                Some(5),
                None,
            ),
        );
        append_trace_event(
            &storage,
            Some(&execution_storage),
            &TraceEvent::run_completed(trace.clone(), Some(180)),
        );

        let summaries =
            list_run_trace_summaries(&storage, &execution_storage, "session-1", "scope-1", 10)
                .expect("summaries");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].trace.run_id, "child-run");
        assert_eq!(
            summaries[0].trace.parent_run_id.as_deref(),
            Some("parent-run")
        );
        assert_eq!(summaries[0].status, "completed");
        assert_eq!(summaries[0].tool_call_count, 1);
        assert_eq!(summaries[0].message_count, 1);
        assert_eq!(summaries[0].llm_call_count, 1);

        let timeline = read_run_trace(
            &storage,
            &execution_storage,
            "session-1",
            "scope-1",
            "child-run",
            20,
        )
        .expect("timeline")
        .expect("timeline exists");
        assert_eq!(timeline.summary.trace.run_id, "child-run");
        assert_eq!(timeline.events.len(), 6);
        assert!(matches!(
            timeline.events[1].event.kind,
            TraceEventKind::LlmCall(_)
        ));
        assert!(matches!(
            timeline.events[2].event.kind,
            TraceEventKind::Message(_)
        ));
    }
}
