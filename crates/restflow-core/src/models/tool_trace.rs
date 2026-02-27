//! Tool trace models.
//!
//! Provides append-only structured events for execution tracing and visualization.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Event type for a persisted tool trace record.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ToolTraceEvent {
    /// A turn started execution.
    TurnStarted,
    /// A tool call started.
    ToolCallStarted,
    /// A tool call completed.
    ToolCallCompleted,
    /// A turn completed successfully.
    TurnCompleted,
    /// A turn failed.
    TurnFailed,
    /// A turn was cancelled.
    TurnCancelled,
}

/// Append-only execution event for chat turns.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct ToolTrace {
    /// Event ID.
    pub id: String,
    /// Session ID this event belongs to.
    pub session_id: String,
    /// Turn ID within the session.
    pub turn_id: String,
    /// Optional assistant message ID (when known).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    /// Event type.
    pub event_type: ToolTraceEvent,
    /// Optional tool call ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Optional tool name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Optional tool input payload (JSON string or raw text).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    /// Optional tool output payload (JSON string or raw text).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// Optional file reference for full output payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_ref: Option<String>,
    /// Optional success flag (typically for tool completion).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,
    /// Optional duration in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// Optional error text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Event timestamp (Unix milliseconds).
    pub created_at: i64,
}

impl ToolTrace {
    fn base(
        session_id: impl Into<String>,
        turn_id: impl Into<String>,
        event_type: ToolTraceEvent,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.into(),
            turn_id: turn_id.into(),
            message_id: None,
            event_type,
            tool_call_id: None,
            tool_name: None,
            input: None,
            output: None,
            output_ref: None,
            success: None,
            duration_ms: None,
            error: None,
            created_at: Utc::now().timestamp_millis(),
        }
    }

    /// Create a turn started event.
    pub fn turn_started(session_id: impl Into<String>, turn_id: impl Into<String>) -> Self {
        Self::base(session_id, turn_id, ToolTraceEvent::TurnStarted)
    }

    /// Create a turn completed event.
    pub fn turn_completed(session_id: impl Into<String>, turn_id: impl Into<String>) -> Self {
        Self::base(session_id, turn_id, ToolTraceEvent::TurnCompleted)
    }

    /// Create a turn failed event.
    pub fn turn_failed(
        session_id: impl Into<String>,
        turn_id: impl Into<String>,
        error: impl Into<String>,
    ) -> Self {
        let mut event = Self::base(session_id, turn_id, ToolTraceEvent::TurnFailed);
        event.error = Some(error.into());
        event
    }

    /// Create a turn cancelled event.
    pub fn turn_cancelled(
        session_id: impl Into<String>,
        turn_id: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        let mut event = Self::base(session_id, turn_id, ToolTraceEvent::TurnCancelled);
        event.error = Some(reason.into());
        event
    }

    /// Create a tool call start event.
    pub fn tool_call_started(
        session_id: impl Into<String>,
        turn_id: impl Into<String>,
        tool_call_id: impl Into<String>,
        tool_name: impl Into<String>,
        input: Option<String>,
    ) -> Self {
        let mut event = Self::base(session_id, turn_id, ToolTraceEvent::ToolCallStarted);
        event.tool_call_id = Some(tool_call_id.into());
        event.tool_name = Some(tool_name.into());
        event.input = input;
        event
    }

    /// Create a tool call completion event.
    pub fn tool_call_completed(
        session_id: impl Into<String>,
        turn_id: impl Into<String>,
        tool_call_id: impl Into<String>,
        tool_name: impl Into<String>,
        completion: ToolCallCompletion,
    ) -> Self {
        let mut event = Self::base(session_id, turn_id, ToolTraceEvent::ToolCallCompleted);
        event.tool_call_id = Some(tool_call_id.into());
        event.tool_name = Some(tool_name.into());
        event.output = completion.output;
        event.output_ref = completion.output_ref;
        event.success = Some(completion.success);
        event.duration_ms = completion.duration_ms;
        event.error = completion.error;
        event
    }
}

/// Tool completion payload used for ToolCallCompleted events.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
pub struct ToolCallCompletion {
    /// Optional tool output payload (JSON string or raw text).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// Optional file reference for full output payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_ref: Option<String>,
    /// Whether the tool call succeeded.
    pub success: bool,
    /// Optional duration in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// Optional error text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_turn_started_constructor() {
        let event = ToolTrace::turn_started("session-1", "turn-1");
        assert_eq!(event.session_id, "session-1");
        assert_eq!(event.turn_id, "turn-1");
        assert_eq!(event.event_type, ToolTraceEvent::TurnStarted);
    }

    #[test]
    fn test_tool_call_completed_constructor() {
        let event = ToolTrace::tool_call_completed(
            "session-1",
            "turn-1",
            "call-1",
            "bash",
            ToolCallCompletion {
                output: Some("{\"ok\":true}".to_string()),
                output_ref: None,
                success: true,
                duration_ms: Some(120),
                error: None,
            },
        );
        assert_eq!(event.event_type, ToolTraceEvent::ToolCallCompleted);
        assert_eq!(event.tool_call_id.as_deref(), Some("call-1"));
        assert_eq!(event.tool_name.as_deref(), Some("bash"));
        assert_eq!(event.success, Some(true));
        assert_eq!(event.duration_ms, Some(120));
    }

    #[test]
    fn export_bindings_tool_trace_event_type() {
        ToolTraceEvent::export_to_string(&ts_rs::Config::default()).expect("ts export");
    }

    #[test]
    fn export_bindings_tool_trace() {
        ToolTrace::export_to_string(&ts_rs::Config::default()).expect("ts export");
    }

    #[test]
    fn export_bindings_tool_call_completion() {
        ToolCallCompletion::export_to_string(&ts_rs::Config::default()).expect("ts export");
    }
}
