//! Shared runtime helpers for RestFlow execution telemetry.

use crate::models::ExecutionTraceEvent;
use crate::storage::ExecutionTraceStorage;
use crate::telemetry::execution_event_to_trace_event;
use anyhow::Result;
use regex::Regex;
use restflow_ai::agent::StreamEmitter;
pub use restflow_telemetry::{
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

/// Persist one canonical trace lifecycle/message event into execution trace storage.
pub fn append_trace_event(execution_trace_storage: &ExecutionTraceStorage, event: &TraceEvent) {
    let projected = execution_event_to_trace_event(&event.to_execution_event_envelope());
    if let Err(error) = execution_trace_storage.store(&projected) {
        warn!(
            scope_id = %projected.task_id,
            agent_id = %projected.agent_id,
            event_id = %projected.id,
            error = %error,
            "Failed to append execution trace event"
        );
    }
}

pub fn append_restflow_telemetry_started(
    execution_trace_storage: &ExecutionTraceStorage,
    trace: &RestflowTrace,
) {
    append_trace_event(
        execution_trace_storage,
        &TraceEvent::run_started(trace.clone()),
    );
}

pub fn append_restflow_telemetry_completed(
    execution_trace_storage: &ExecutionTraceStorage,
    trace: &RestflowTrace,
    ai_duration_ms: Option<u64>,
) {
    append_trace_event(
        execution_trace_storage,
        &TraceEvent::run_completed(trace.clone(), ai_duration_ms),
    );
}

pub fn append_restflow_telemetry_failed(
    execution_trace_storage: &ExecutionTraceStorage,
    trace: &RestflowTrace,
    error_text: &str,
    ai_duration_ms: Option<u64>,
) {
    let sanitized_error = truncate_trace_text(
        &sanitize_trace_secrets(error_text),
        MAX_TRACE_EVENT_TEXT_CHARS,
    );
    append_trace_event(
        execution_trace_storage,
        &TraceEvent::run_failed(trace.clone(), sanitized_error, ai_duration_ms),
    );
}

pub fn append_restflow_telemetry_interrupted(
    execution_trace_storage: &ExecutionTraceStorage,
    trace: &RestflowTrace,
    reason: &str,
    ai_duration_ms: Option<u64>,
) {
    let sanitized_reason =
        truncate_trace_text(&sanitize_trace_secrets(reason), MAX_TRACE_EVENT_TEXT_CHARS);
    append_trace_event(
        execution_trace_storage,
        &TraceEvent::run_interrupted(trace.clone(), sanitized_reason, ai_duration_ms),
    );
}

/// Append a canonical message trace event through execution trace storage.
pub fn append_message_trace(
    execution_trace_storage: &ExecutionTraceStorage,
    trace: &RestflowTrace,
    role: &str,
    content: &str,
) {
    let content_preview = normalize_trace_payload(content);
    append_trace_event(
        execution_trace_storage,
        &TraceEvent::message(trace.clone(), role.to_string(), content_preview, None),
    );
}

/// Build a stream emitter bound to a canonical RestFlow trace.
///
/// Tool-call execution telemetry is emitted directly by `AgentExecutor` through
/// `TelemetrySink`, so this adapter only preserves user-visible streaming.
pub fn build_restflow_telemetry_emitter(
    inner: Box<dyn StreamEmitter>,
    _execution_trace_storage: Option<ExecutionTraceStorage>,
    _trace: &RestflowTrace,
) -> Box<dyn StreamEmitter> {
    inner
}

#[allow(dead_code)]
fn _ensure_projected_event(_event: &ExecutionTraceEvent) -> Result<()> {
    Ok(())
}
