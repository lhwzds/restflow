//! Lightweight trace events for debugging

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Lightweight trace event for debugging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEvent {
    pub timestamp: DateTime<Utc>,
    pub event_type: String,
    pub data: Value,
}

impl TraceEvent {
    /// Create a new trace event
    pub fn new(event_type: impl Into<String>, data: Value) -> Self {
        Self {
            timestamp: Utc::now(),
            event_type: event_type.into(),
            data,
        }
    }

    /// Create a tool call event
    pub fn tool_call(tool: &str, input: &Value) -> Self {
        Self::new(
            "tool_call",
            serde_json::json!({
                "tool": tool,
                "input": input
            }),
        )
    }

    /// Create a tool result event
    pub fn tool_result(tool: &str, success: bool) -> Self {
        Self::new(
            "tool_result",
            serde_json::json!({
                "tool": tool,
                "success": success
            }),
        )
    }

    /// Create an LLM call event
    pub fn llm_call(message_count: usize) -> Self {
        Self::new(
            "llm_call",
            serde_json::json!({
                "messages": message_count
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_event_new() {
        let event = TraceEvent::new("test", serde_json::json!({"key": "value"}));
        assert_eq!(event.event_type, "test");
        assert_eq!(event.data["key"], "value");
    }

    #[test]
    fn test_trace_event_tool_call() {
        let event = TraceEvent::tool_call(
            "http_request",
            &serde_json::json!({"url": "http://example.com"}),
        );
        assert_eq!(event.event_type, "tool_call");
        assert_eq!(event.data["tool"], "http_request");
    }
}
