//! Shared execution telemetry domain primitives for RestFlow.

use async_trait::async_trait;
use chrono::Utc;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

mod run_lifecycle;

pub use run_lifecycle::{RunAttemptTracker, RunDescriptor, RunHandle, RunKind, RunLifecycleService};

pub const DEFAULT_TELEMETRY_TEXT_LIMIT: usize = 10_000;

/// Context describing a traced run execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunTraceContext {
    pub run_id: String,
    pub actor_id: String,
    pub parent_run_id: Option<String>,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub scope_id: String,
}

/// Outcome for traced run completion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunTraceOutcome {
    pub success: bool,
    pub error: Option<String>,
}

/// Shared execution telemetry context propagated through runtime and executors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryContext {
    pub trace: RestflowTrace,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub effective_model: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub attempt: Option<u32>,
}

impl TelemetryContext {
    pub fn new(trace: RestflowTrace) -> Self {
        Self {
            trace,
            requested_model: None,
            effective_model: None,
            provider: None,
            attempt: None,
        }
    }

    pub fn with_requested_model(mut self, requested_model: impl Into<String>) -> Self {
        self.requested_model = Some(requested_model.into());
        self
    }

    pub fn with_effective_model(mut self, effective_model: impl Into<String>) -> Self {
        self.effective_model = Some(effective_model.into());
        self
    }

    pub fn with_provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = Some(provider.into());
        self
    }

    pub fn with_attempt(mut self, attempt: u32) -> Self {
        self.attempt = Some(attempt);
        self
    }
}

#[async_trait]
pub trait TelemetrySink: Send + Sync {
    async fn emit(&self, event: ExecutionEventEnvelope);
}

#[derive(Default)]
pub struct CompositeTelemetrySink {
    sinks: Vec<std::sync::Arc<dyn TelemetrySink>>,
}

impl CompositeTelemetrySink {
    pub fn new(sinks: Vec<std::sync::Arc<dyn TelemetrySink>>) -> Self {
        Self { sinks }
    }

    pub fn with_sink(mut self, sink: std::sync::Arc<dyn TelemetrySink>) -> Self {
        self.sinks.push(sink);
        self
    }
}

#[async_trait]
impl TelemetrySink for CompositeTelemetrySink {
    async fn emit(&self, event: ExecutionEventEnvelope) {
        for sink in &self.sinks {
            sink.emit(event.clone()).await;
        }
    }
}

/// Tool-call start payload carried by the canonical telemetry schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallStartedPayload {
    pub tool_call_id: String,
    pub tool_name: String,
    pub input: Option<String>,
}

/// Tool-call completion payload carried by the canonical telemetry schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallCompletedPayload {
    pub tool_call_id: String,
    pub tool_name: String,
    pub input_summary: Option<String>,
    pub output: Option<String>,
    pub output_ref: Option<String>,
    pub success: bool,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
}

/// Metric dimension payload carried by execution telemetry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionMetricDimension {
    pub key: String,
    pub value: String,
}

/// Metric sample payload carried by execution telemetry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionMetricSample {
    pub name: String,
    pub value: f64,
    pub unit: Option<String>,
    #[serde(default)]
    pub dimensions: Vec<ExecutionMetricDimension>,
}

/// Provider health payload carried by execution telemetry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderHealthChanged {
    pub provider: String,
    pub model: Option<String>,
    pub status: String,
    pub reason: Option<String>,
    pub error_kind: Option<String>,
}

/// Structured log field payload carried by execution telemetry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionLogField {
    pub key: String,
    pub value: String,
}

/// Structured log record payload carried by execution telemetry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionLogRecord {
    pub level: String,
    pub message: String,
    #[serde(default)]
    pub fields: Vec<ExecutionLogField>,
}

/// LLM-call payload carried by the canonical telemetry schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmCallPayload {
    pub model: String,
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<u64>,
    pub is_reasoning: Option<bool>,
    pub message_count: Option<u32>,
}

/// Message payload carried by the canonical telemetry schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessagePayload {
    pub role: String,
    pub content_preview: Option<String>,
    pub tool_call_count: Option<u32>,
}

/// Unified execution telemetry event payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExecutionEvent {
    RunStarted,
    RunCompleted {
        ai_duration_ms: Option<u64>,
    },
    RunFailed {
        error: String,
        ai_duration_ms: Option<u64>,
    },
    RunInterrupted {
        reason: String,
        ai_duration_ms: Option<u64>,
    },
    ModelSwitch {
        from_model: String,
        to_model: String,
        reason: Option<String>,
        success: bool,
    },
    LlmCall(LlmCallPayload),
    ToolCallStarted(ToolCallStartedPayload),
    ToolCallCompleted(ToolCallCompletedPayload),
    Message(MessagePayload),
    MetricSample(ExecutionMetricSample),
    ProviderHealthChanged(ProviderHealthChanged),
    LogRecord(ExecutionLogRecord),
}

/// Unified execution telemetry envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionEventEnvelope {
    pub event_id: String,
    pub occurred_at_ms: i64,
    pub trace: RestflowTrace,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub effective_model: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub attempt: Option<u32>,
    pub event: ExecutionEvent,
}

impl ExecutionEventEnvelope {
    pub fn new(trace: RestflowTrace, event: ExecutionEvent) -> Self {
        let occurred_at_ms = Utc::now().timestamp_millis();
        Self {
            event_id: format!("{}-{occurred_at_ms}", trace.run_id),
            occurred_at_ms,
            trace,
            requested_model: None,
            effective_model: None,
            provider: None,
            attempt: None,
            event,
        }
    }

    pub fn with_requested_model(mut self, requested_model: impl Into<String>) -> Self {
        self.requested_model = Some(requested_model.into());
        self
    }

    pub fn with_effective_model(mut self, effective_model: impl Into<String>) -> Self {
        self.effective_model = Some(effective_model.into());
        self
    }

    pub fn with_provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = Some(provider.into());
        self
    }

    pub fn with_attempt(mut self, attempt: u32) -> Self {
        self.attempt = Some(attempt);
        self
    }

    pub fn from_telemetry_context(context: &TelemetryContext, event: ExecutionEvent) -> Self {
        let mut envelope = Self::new(context.trace.clone(), event);
        envelope.requested_model = context.requested_model.clone();
        envelope.effective_model = context.effective_model.clone();
        envelope.provider = context.provider.clone();
        envelope.attempt = context.attempt;
        envelope
    }

    pub fn run_started(trace: RestflowTrace) -> Self {
        Self::new(trace, ExecutionEvent::RunStarted)
    }

    pub fn run_completed(trace: RestflowTrace, ai_duration_ms: Option<u64>) -> Self {
        Self::new(trace, ExecutionEvent::RunCompleted { ai_duration_ms })
    }

    pub fn run_failed(
        trace: RestflowTrace,
        error: impl Into<String>,
        ai_duration_ms: Option<u64>,
    ) -> Self {
        Self::new(
            trace,
            ExecutionEvent::RunFailed {
                error: error.into(),
                ai_duration_ms,
            },
        )
    }

    pub fn run_interrupted(
        trace: RestflowTrace,
        reason: impl Into<String>,
        ai_duration_ms: Option<u64>,
    ) -> Self {
        Self::new(
            trace,
            ExecutionEvent::RunInterrupted {
                reason: reason.into(),
                ai_duration_ms,
            },
        )
    }

    pub fn message(
        trace: RestflowTrace,
        role: impl Into<String>,
        content_preview: Option<String>,
        tool_call_count: Option<u32>,
    ) -> Self {
        Self::new(
            trace,
            ExecutionEvent::Message(MessagePayload {
                role: role.into(),
                content_preview,
                tool_call_count,
            }),
        )
    }
}

/// Source storage for a timeline event returned by backend trace queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceTimelineSource {
    ExecutionTrace,
}

/// Optional artifact tail preview attached to a timeline event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceArtifactPreview {
    pub path: String,
    pub total_lines: usize,
    pub lines: Vec<String>,
}

/// One persisted timeline record for a run trace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceTimelineEvent {
    pub record_id: Option<String>,
    pub timestamp_ms: i64,
    pub source: TraceTimelineSource,
    pub event: ExecutionEventEnvelope,
    #[serde(default)]
    pub artifact_preview: Option<TraceArtifactPreview>,
}

/// Aggregated summary for one traced run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunTraceSummary {
    pub trace: RestflowTrace,
    pub status: String,
    pub started_at_ms: Option<i64>,
    pub ended_at_ms: Option<i64>,
    pub last_event_at_ms: i64,
    pub event_count: usize,
    pub tool_call_count: usize,
    pub message_count: usize,
    pub llm_call_count: usize,
}

/// Full backend timeline for one traced run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunTraceTimeline {
    pub summary: RunTraceSummary,
    pub events: Vec<TraceTimelineEvent>,
}

/// Canonical RestFlow trace descriptor for one run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestflowTrace {
    pub run_id: String,
    #[serde(default)]
    pub parent_run_id: Option<String>,
    pub session_id: String,
    pub turn_id: String,
    #[serde(alias = "execution_task_id")]
    pub scope_id: String,
    pub actor_id: String,
    pub created_at_ms: i64,
}

impl RestflowTrace {
    pub fn new(
        run_id: impl Into<String>,
        session_id: impl Into<String>,
        scope_id: impl Into<String>,
        actor_id: impl Into<String>,
    ) -> Self {
        let run_id = run_id.into();
        Self {
            parent_run_id: None,
            turn_id: format!("run-{}", run_id),
            run_id,
            session_id: session_id.into(),
            scope_id: scope_id.into(),
            actor_id: actor_id.into(),
            created_at_ms: Utc::now().timestamp_millis(),
        }
    }

    pub fn with_parent_run_id(mut self, parent_run_id: Option<String>) -> Self {
        self.parent_run_id = parent_run_id;
        self
    }

    pub fn from_run(
        run_id: impl Into<String>,
        actor_id: impl Into<String>,
        parent_run_id: Option<String>,
        session_id: Option<String>,
        scope_id: Option<String>,
    ) -> Self {
        let run_id = run_id.into();
        let session_id = session_id
            .filter(|value| !value.trim().is_empty())
            .or(scope_id.clone().filter(|value| !value.trim().is_empty()))
            .or(parent_run_id.clone())
            .unwrap_or_else(|| run_id.clone());
        let scope_id = scope_id
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                let session_id = session_id.trim();
                if session_id.is_empty() {
                    None
                } else {
                    Some(session_id.to_string())
                }
            })
            .or(parent_run_id.clone())
            .unwrap_or_else(|| run_id.clone());
        Self::new(run_id, session_id, scope_id, actor_id).with_parent_run_id(parent_run_id)
    }

    pub fn from_context(context: &RunTraceContext) -> Self {
        Self::from_run(
            context.run_id.clone(),
            context.actor_id.clone(),
            context.parent_run_id.clone(),
            Some(context.session_id.clone()),
            Some(context.scope_id.clone()),
        )
    }
}

pub fn truncate_telemetry_text(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars {
        return value.to_string();
    }
    let mut truncated = value.chars().take(max_chars).collect::<String>();
    truncated.push_str("...");
    truncated
}

pub fn sanitize_telemetry_secrets(input: &str) -> String {
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

pub fn normalize_telemetry_preview(value: &str, max_chars: usize) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let sanitized = sanitize_telemetry_secrets(trimmed);
    let normalized = match serde_json::from_str::<serde_json::Value>(&sanitized) {
        Ok(json) => json.to_string(),
        Err(_) => sanitized,
    };
    Some(truncate_telemetry_text(&normalized, max_chars))
}

#[cfg(test)]
mod tests {
    use super::{
        ExecutionEvent, ExecutionEventEnvelope, LlmCallPayload, MessagePayload, RestflowTrace,
        RunTraceContext, RunTraceOutcome, RunTraceSummary, RunTraceTimeline,
        ToolCallCompletedPayload, TraceArtifactPreview, TraceTimelineEvent, TraceTimelineSource,
        normalize_telemetry_preview, sanitize_telemetry_secrets,
    };

    #[test]
    fn new_uses_run_prefixed_turn_id() {
        let trace = RestflowTrace::new("run-1", "session-1", "task-1", "agent-1");
        assert_eq!(trace.run_id, "run-1");
        assert_eq!(trace.parent_run_id, None);
        assert_eq!(trace.turn_id, "run-run-1");
        assert_eq!(trace.session_id, "session-1");
        assert_eq!(trace.scope_id, "task-1");
        assert_eq!(trace.actor_id, "agent-1");
    }

    #[test]
    fn from_run_defaults_to_parent_when_present() {
        let trace = RestflowTrace::from_run(
            "child-run",
            "worker",
            Some("parent-run".to_string()),
            None,
            None,
        );
        assert_eq!(trace.parent_run_id.as_deref(), Some("parent-run"));
        assert_eq!(trace.session_id, "parent-run");
        assert_eq!(trace.scope_id, "parent-run");
        assert_eq!(trace.turn_id, "run-child-run");
    }

    #[test]
    fn from_context_maps_context_fields() {
        let context = RunTraceContext {
            run_id: "child-run".to_string(),
            actor_id: "worker".to_string(),
            parent_run_id: Some("parent-run".to_string()),
            session_id: "session-1".to_string(),
            scope_id: "scope-1".to_string(),
        };

        let trace = RestflowTrace::from_context(&context);
        assert_eq!(trace.run_id, "child-run");
        assert_eq!(trace.parent_run_id.as_deref(), Some("parent-run"));
        assert_eq!(trace.session_id, "session-1");
        assert_eq!(trace.scope_id, "scope-1");
        assert_eq!(trace.actor_id, "worker");
    }

    #[test]
    fn run_trace_context_roundtrips_through_json() {
        let context = RunTraceContext {
            run_id: "run-1".to_string(),
            actor_id: "worker".to_string(),
            parent_run_id: Some("parent-1".to_string()),
            session_id: "session-1".to_string(),
            scope_id: "scope-1".to_string(),
        };

        let json = serde_json::to_string(&context).expect("serialize context");
        let restored: RunTraceContext = serde_json::from_str(&json).expect("deserialize context");

        assert_eq!(restored, context);
    }

    #[test]
    fn run_trace_outcome_roundtrips_through_json() {
        let outcome = RunTraceOutcome {
            success: false,
            error: Some("boom".to_string()),
        };

        let json = serde_json::to_string(&outcome).expect("serialize outcome");
        let restored: RunTraceOutcome = serde_json::from_str(&json).expect("deserialize outcome");

        assert_eq!(restored, outcome);
    }

    #[test]
    fn execution_event_envelope_roundtrips_through_json() {
        let event = ExecutionEventEnvelope::run_failed(
            RestflowTrace::new("run-1", "session-1", "task-1", "agent-1"),
            "boom",
            Some(123),
        );

        let json = serde_json::to_string(&event).expect("serialize event");
        let restored: ExecutionEventEnvelope =
            serde_json::from_str(&json).expect("deserialize event");

        assert_eq!(restored, event);
    }

    #[test]
    fn trace_roundtrips_legacy_execution_task_id_as_scope_id() {
        let json = r#"{
            "run_id":"run-1",
            "session_id":"session-1",
            "turn_id":"run-run-1",
            "execution_task_id":"task-legacy",
            "actor_id":"agent-1",
            "created_at_ms":123
        }"#;

        let restored: RestflowTrace = serde_json::from_str(json).expect("deserialize legacy trace");
        assert_eq!(restored.scope_id, "task-legacy");
        assert_eq!(restored.parent_run_id, None);
    }

    #[test]
    fn run_completed_preserves_duration() {
        let event = ExecutionEventEnvelope::run_completed(
            RestflowTrace::new("run-1", "session-1", "task-1", "agent-1"),
            Some(321),
        );

        assert_eq!(
            event.event,
            ExecutionEvent::RunCompleted {
                ai_duration_ms: Some(321)
            }
        );
    }

    #[test]
    fn tool_call_completed_roundtrips_payload() {
        let event = ExecutionEventEnvelope::new(
            RestflowTrace::new("run-1", "session-1", "task-1", "agent-1"),
            ExecutionEvent::ToolCallCompleted(ToolCallCompletedPayload {
                tool_call_id: "call-1".to_string(),
                tool_name: "bash".to_string(),
                input_summary: Some("{\"cmd\":\"echo hi\"}".to_string()),
                output: Some("{\"ok\":true}".to_string()),
                output_ref: Some("/tmp/output.txt".to_string()),
                success: true,
                duration_ms: Some(42),
                error: None,
            }),
        );

        let json = serde_json::to_string(&event).expect("serialize event");
        let restored: ExecutionEventEnvelope =
            serde_json::from_str(&json).expect("deserialize event");
        assert_eq!(restored, event);
    }

    #[test]
    fn message_roundtrips_payload() {
        let event = ExecutionEventEnvelope::message(
            RestflowTrace::new("run-1", "session-1", "task-1", "agent-1"),
            "assistant",
            Some("hello".to_string()),
            Some(2),
        );

        assert_eq!(
            event.event,
            ExecutionEvent::Message(MessagePayload {
                role: "assistant".to_string(),
                content_preview: Some("hello".to_string()),
                tool_call_count: Some(2),
            })
        );
    }

    #[test]
    fn llm_call_roundtrips_payload() {
        let event = ExecutionEventEnvelope::new(
            RestflowTrace::new("run-1", "session-1", "task-1", "agent-1"),
            ExecutionEvent::LlmCall(LlmCallPayload {
                model: "claude-sonnet".to_string(),
                input_tokens: Some(100),
                output_tokens: Some(50),
                total_tokens: Some(150),
                cost_usd: Some(0.25),
                duration_ms: Some(321),
                is_reasoning: Some(false),
                message_count: Some(4),
            }),
        );

        let json = serde_json::to_string(&event).expect("serialize event");
        let restored: ExecutionEventEnvelope =
            serde_json::from_str(&json).expect("deserialize event");
        assert_eq!(restored, event);
    }

    #[test]
    fn run_trace_timeline_roundtrips_through_json() {
        let trace = RestflowTrace::new("run-1", "session-1", "scope-1", "agent-1");
        let timeline = RunTraceTimeline {
            summary: RunTraceSummary {
                trace: trace.clone(),
                status: "completed".to_string(),
                started_at_ms: Some(100),
                ended_at_ms: Some(200),
                last_event_at_ms: 200,
                event_count: 2,
                tool_call_count: 1,
                message_count: 0,
                llm_call_count: 1,
            },
            events: vec![TraceTimelineEvent {
                record_id: Some("event-1".to_string()),
                timestamp_ms: 200,
                source: TraceTimelineSource::ExecutionTrace,
                event: ExecutionEventEnvelope::new(
                    trace,
                    ExecutionEvent::LlmCall(LlmCallPayload {
                        model: "gpt-5".to_string(),
                        input_tokens: Some(10),
                        output_tokens: Some(20),
                        total_tokens: Some(30),
                        cost_usd: None,
                        duration_ms: Some(123),
                        is_reasoning: None,
                        message_count: Some(4),
                    }),
                ),
                artifact_preview: Some(TraceArtifactPreview {
                    path: "/tmp/output.txt".to_string(),
                    total_lines: 2,
                    lines: vec!["a".to_string(), "b".to_string()],
                }),
            }],
        };

        let json = serde_json::to_string(&timeline).expect("serialize timeline");
        let restored: RunTraceTimeline = serde_json::from_str(&json).expect("deserialize timeline");
        assert_eq!(restored, timeline);
    }

    #[test]
    fn sanitize_telemetry_secrets_redacts_known_patterns() {
        let sanitized = sanitize_telemetry_secrets("Authorization: Bearer sk-secret-secret-secret");
        assert!(sanitized.contains("[REDACTED]"));
    }

    #[test]
    fn normalize_telemetry_preview_trims_and_truncates() {
        let preview = normalize_telemetry_preview("  hello world  ", 5).expect("preview");
        assert_eq!(preview, "hello...");
    }
}
