//! Shared run-trace primitives for RestFlow.

use chrono::Utc;
use serde::{Deserialize, Serialize};

/// Context describing a traced run execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunTraceContext {
    pub run_id: String,
    pub actor_id: String,
    pub parent_run_id: Option<String>,
}

/// Outcome for traced run completion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunTraceOutcome {
    pub success: bool,
    pub error: Option<String>,
}

/// Canonical event payload for one traced run lifecycle transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceEvent {
    pub trace: RestflowTrace,
    pub kind: TraceEventKind,
}

/// Tool-call start payload carried by the canonical trace event schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceToolCallStart {
    pub tool_call_id: String,
    pub tool_name: String,
    pub input: Option<String>,
}

/// Tool-call completion payload carried by the canonical trace event schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceToolCallCompleted {
    pub tool_call_id: String,
    pub tool_name: String,
    pub input_summary: Option<String>,
    pub output: Option<String>,
    pub output_ref: Option<String>,
    pub success: bool,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
}

/// Lifecycle event kinds emitted for a traced run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TraceEventKind {
    RunStarted,
    RunCompleted {
        ai_duration_ms: Option<u64>,
    },
    RunFailed {
        error: String,
        ai_duration_ms: Option<u64>,
    },
    RunCancelled {
        reason: String,
        ai_duration_ms: Option<u64>,
    },
    ToolCallStarted(TraceToolCallStart),
    ToolCallCompleted(TraceToolCallCompleted),
}

/// Canonical RestFlow trace descriptor for one run.
///
/// `created_at_ms` captures trace metadata creation time.
/// Callers should record AI execution duration independently and attach it to
/// terminal events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestflowTrace {
    pub run_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub execution_task_id: String,
    pub actor_id: String,
    pub created_at_ms: i64,
}

impl RestflowTrace {
    /// Build a trace descriptor from explicit fields.
    pub fn new(
        run_id: impl Into<String>,
        session_id: impl Into<String>,
        execution_task_id: impl Into<String>,
        actor_id: impl Into<String>,
    ) -> Self {
        let run_id = run_id.into();
        Self {
            turn_id: format!("run-{}", run_id),
            run_id,
            session_id: session_id.into(),
            execution_task_id: execution_task_id.into(),
            actor_id: actor_id.into(),
            created_at_ms: Utc::now().timestamp_millis(),
        }
    }

    /// Build from run metadata with sane defaults for missing parent/scope.
    pub fn from_run(
        run_id: impl Into<String>,
        actor_id: impl Into<String>,
        parent_run_id: Option<String>,
        execution_scope_id: Option<String>,
    ) -> Self {
        let run_id = run_id.into();
        let session_id = parent_run_id.clone().unwrap_or_else(|| run_id.clone());
        let execution_task_id = execution_scope_id
            .or(parent_run_id)
            .unwrap_or_else(|| run_id.clone());
        Self::new(run_id, session_id, execution_task_id, actor_id)
    }
}

impl TraceEvent {
    pub fn run_started(trace: RestflowTrace) -> Self {
        Self {
            trace,
            kind: TraceEventKind::RunStarted,
        }
    }

    pub fn run_completed(trace: RestflowTrace, ai_duration_ms: Option<u64>) -> Self {
        Self {
            trace,
            kind: TraceEventKind::RunCompleted { ai_duration_ms },
        }
    }

    pub fn run_failed(
        trace: RestflowTrace,
        error: impl Into<String>,
        ai_duration_ms: Option<u64>,
    ) -> Self {
        Self {
            trace,
            kind: TraceEventKind::RunFailed {
                error: error.into(),
                ai_duration_ms,
            },
        }
    }

    pub fn run_cancelled(
        trace: RestflowTrace,
        reason: impl Into<String>,
        ai_duration_ms: Option<u64>,
    ) -> Self {
        Self {
            trace,
            kind: TraceEventKind::RunCancelled {
                reason: reason.into(),
                ai_duration_ms,
            },
        }
    }

    pub fn tool_call_started(
        trace: RestflowTrace,
        tool_call_id: impl Into<String>,
        tool_name: impl Into<String>,
        input: Option<String>,
    ) -> Self {
        Self {
            trace,
            kind: TraceEventKind::ToolCallStarted(TraceToolCallStart {
                tool_call_id: tool_call_id.into(),
                tool_name: tool_name.into(),
                input,
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn tool_call_completed(
        trace: RestflowTrace,
        tool_call_id: impl Into<String>,
        tool_name: impl Into<String>,
        input_summary: Option<String>,
        output: Option<String>,
        output_ref: Option<String>,
        success: bool,
        duration_ms: Option<u64>,
        error: Option<String>,
    ) -> Self {
        Self {
            trace,
            kind: TraceEventKind::ToolCallCompleted(TraceToolCallCompleted {
                tool_call_id: tool_call_id.into(),
                tool_name: tool_name.into(),
                input_summary,
                output,
                output_ref,
                success,
                duration_ms,
                error,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RestflowTrace, RunTraceContext, RunTraceOutcome, TraceEvent, TraceEventKind,
        TraceToolCallCompleted,
    };

    #[test]
    fn new_uses_run_prefixed_turn_id() {
        let trace = RestflowTrace::new("run-1", "session-1", "task-1", "agent-1");
        assert_eq!(trace.run_id, "run-1");
        assert_eq!(trace.turn_id, "run-run-1");
        assert_eq!(trace.session_id, "session-1");
        assert_eq!(trace.execution_task_id, "task-1");
        assert_eq!(trace.actor_id, "agent-1");
    }

    #[test]
    fn from_run_defaults_to_parent_when_present() {
        let trace =
            RestflowTrace::from_run("child-run", "worker", Some("parent-run".to_string()), None);
        assert_eq!(trace.session_id, "parent-run");
        assert_eq!(trace.execution_task_id, "parent-run");
        assert_eq!(trace.turn_id, "run-child-run");
    }

    #[test]
    fn run_trace_context_roundtrips_through_json() {
        let context = RunTraceContext {
            run_id: "run-1".to_string(),
            actor_id: "worker".to_string(),
            parent_run_id: Some("parent-1".to_string()),
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
    fn trace_event_roundtrips_through_json() {
        let event = TraceEvent::run_failed(
            RestflowTrace::new("run-1", "session-1", "task-1", "agent-1"),
            "boom",
            Some(123),
        );

        let json = serde_json::to_string(&event).expect("serialize event");
        let restored: TraceEvent = serde_json::from_str(&json).expect("deserialize event");

        assert_eq!(restored, event);
    }

    #[test]
    fn trace_event_run_completed_preserves_duration() {
        let event = TraceEvent::run_completed(
            RestflowTrace::new("run-1", "session-1", "task-1", "agent-1"),
            Some(321),
        );

        assert_eq!(
            event.kind,
            TraceEventKind::RunCompleted {
                ai_duration_ms: Some(321)
            }
        );
    }

    #[test]
    fn trace_event_tool_call_completed_roundtrips_payload() {
        let event = TraceEvent::tool_call_completed(
            RestflowTrace::new("run-1", "session-1", "task-1", "agent-1"),
            "call-1",
            "bash",
            Some("{\"cmd\":\"echo hi\"}".to_string()),
            Some("{\"ok\":true}".to_string()),
            Some("/tmp/output.txt".to_string()),
            true,
            Some(42),
            None,
        );

        assert_eq!(
            event.kind,
            TraceEventKind::ToolCallCompleted(TraceToolCallCompleted {
                tool_call_id: "call-1".to_string(),
                tool_name: "bash".to_string(),
                input_summary: Some("{\"cmd\":\"echo hi\"}".to_string()),
                output: Some("{\"ok\":true}".to_string()),
                output_ref: Some("/tmp/output.txt".to_string()),
                success: true,
                duration_ms: Some(42),
                error: None,
            })
        );
    }
}
