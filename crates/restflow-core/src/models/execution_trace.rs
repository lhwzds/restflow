//! Execution trace event models for background agent execution tracking.
//!
//! This module provides structured execution trace events for tracking:
//! - LLM calls (model, tokens, cost)
//! - Tool invocations (name, input, output, duration)
//! - Model switches (from, to, reason)
//! - Lifecycle events (start, end, errors)
//!
//! # Design Goals
//!
//! - Additive schema for backward compatibility
//! - Efficient storage with indexed lookups
//! - Rich context for debugging and analytics

use chrono::Utc;
use serde::{Deserialize, Serialize};
use specta::Type;
use ts_rs::TS;

/// Execution trace event category.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionTraceCategory {
    /// LLM API call event.
    LlmCall,
    /// Tool invocation event.
    ToolCall,
    /// Model switch event.
    ModelSwitch,
    /// Lifecycle event (start/end/error).
    Lifecycle,
    /// Agent message event.
    Message,
}

/// Source of the execution trace event.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionTraceSource {
    /// Event from the agent executor.
    AgentExecutor,
    /// Event from the runtime.
    Runtime,
    /// Event from MCP server.
    McpServer,
    /// Event from CLI.
    Cli,
}

/// LLM call trace data.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type)]
#[ts(export)]
pub struct LlmCallTrace {
    /// Model used for the call.
    pub model: String,
    /// Number of input tokens.
    pub input_tokens: Option<u32>,
    /// Number of output tokens.
    pub output_tokens: Option<u32>,
    /// Total tokens used.
    pub total_tokens: Option<u32>,
    /// Cost in USD.
    pub cost_usd: Option<f64>,
    /// Duration of the call in milliseconds.
    pub duration_ms: Option<i64>,
    /// Whether this was a reasoning call.
    pub is_reasoning: Option<bool>,
    /// Number of messages in the request.
    pub message_count: Option<u32>,
}

/// Tool call trace data.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type)]
#[ts(export)]
pub struct ToolCallTrace {
    /// Name of the tool invoked.
    pub tool_name: String,
    /// Tool input (truncated for storage efficiency).
    pub input_summary: Option<String>,
    /// Whether the tool succeeded.
    pub success: bool,
    /// Error message if failed.
    pub error: Option<String>,
    /// Duration of the tool execution in milliseconds.
    pub duration_ms: Option<i64>,
}

/// Model switch trace data.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type)]
#[ts(export)]
pub struct ModelSwitchTrace {
    /// Previous model.
    pub from_model: String,
    /// Target model.
    pub to_model: String,
    /// Reason for switching.
    pub reason: Option<String>,
    /// Whether the switch was successful.
    pub success: bool,
}

/// Lifecycle event trace data.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type)]
#[ts(export)]
pub struct LifecycleTrace {
    /// Current status after the lifecycle event.
    pub status: String,
    /// Detailed message.
    pub message: Option<String>,
    /// Error details if applicable.
    pub error: Option<String>,
}

/// Message event trace data.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type)]
#[ts(export)]
pub struct MessageTrace {
    /// Role of the message sender.
    pub role: String,
    /// Message content preview.
    pub content_preview: Option<String>,
    /// Number of tool calls in this message.
    pub tool_call_count: Option<u32>,
}

/// Unified execution trace event structure.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type)]
#[ts(export)]
pub struct ExecutionTraceEvent {
    /// Unique event ID.
    pub id: String,
    /// Task ID this event belongs to.
    pub task_id: String,
    /// Agent ID that generated this event.
    pub agent_id: String,
    /// Event category.
    pub category: ExecutionTraceCategory,
    /// Event source.
    pub source: ExecutionTraceSource,
    /// Timestamp (milliseconds since epoch).
    #[ts(type = "number")]
    pub timestamp: i64,
    /// Subflow path for nested agent calls.
    #[serde(default)]
    #[ts(type = "string[]")]
    pub subflow_path: Vec<String>,
    /// LLM call details (if category is LlmCall).
    #[serde(default)]
    pub llm_call: Option<LlmCallTrace>,
    /// Tool call details (if category is ToolCall).
    #[serde(default)]
    pub tool_call: Option<ToolCallTrace>,
    /// Model switch details (if category is ModelSwitch).
    #[serde(default)]
    pub model_switch: Option<ModelSwitchTrace>,
    /// Lifecycle details (if category is Lifecycle).
    #[serde(default)]
    pub lifecycle: Option<LifecycleTrace>,
    /// Message details (if category is Message).
    #[serde(default)]
    pub message: Option<MessageTrace>,
}

impl ExecutionTraceEvent {
    /// Create a new execution trace event with a generated ID.
    pub fn new(
        task_id: impl Into<String>,
        agent_id: impl Into<String>,
        category: ExecutionTraceCategory,
        source: ExecutionTraceSource,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            task_id: task_id.into(),
            agent_id: agent_id.into(),
            category,
            source,
            timestamp: Utc::now().timestamp_millis(),
            subflow_path: Vec::new(),
            llm_call: None,
            tool_call: None,
            model_switch: None,
            lifecycle: None,
            message: None,
        }
    }

    /// Create an LLM call trace event.
    pub fn llm_call(
        task_id: impl Into<String>,
        agent_id: impl Into<String>,
        trace: LlmCallTrace,
    ) -> Self {
        Self::new(
            task_id,
            agent_id,
            ExecutionTraceCategory::LlmCall,
            ExecutionTraceSource::AgentExecutor,
        )
        .with_llm_call(trace)
    }

    /// Create a tool call trace event.
    pub fn tool_call(
        task_id: impl Into<String>,
        agent_id: impl Into<String>,
        trace: ToolCallTrace,
    ) -> Self {
        Self::new(
            task_id,
            agent_id,
            ExecutionTraceCategory::ToolCall,
            ExecutionTraceSource::AgentExecutor,
        )
        .with_tool_call(trace)
    }

    /// Create a model switch trace event.
    pub fn model_switch(
        task_id: impl Into<String>,
        agent_id: impl Into<String>,
        trace: ModelSwitchTrace,
    ) -> Self {
        Self::new(
            task_id,
            agent_id,
            ExecutionTraceCategory::ModelSwitch,
            ExecutionTraceSource::AgentExecutor,
        )
        .with_model_switch(trace)
    }

    /// Create a lifecycle trace event.
    pub fn lifecycle(
        task_id: impl Into<String>,
        agent_id: impl Into<String>,
        trace: LifecycleTrace,
    ) -> Self {
        Self::new(
            task_id,
            agent_id,
            ExecutionTraceCategory::Lifecycle,
            ExecutionTraceSource::Runtime,
        )
        .with_lifecycle(trace)
    }

    /// Create a message trace event.
    pub fn message(
        task_id: impl Into<String>,
        agent_id: impl Into<String>,
        trace: MessageTrace,
    ) -> Self {
        Self::new(
            task_id,
            agent_id,
            ExecutionTraceCategory::Message,
            ExecutionTraceSource::AgentExecutor,
        )
        .with_message(trace)
    }

    /// Set LLM call details.
    pub fn with_llm_call(mut self, trace: LlmCallTrace) -> Self {
        self.llm_call = Some(trace);
        self
    }

    /// Set tool call details.
    pub fn with_tool_call(mut self, trace: ToolCallTrace) -> Self {
        self.tool_call = Some(trace);
        self
    }

    /// Set model switch details.
    pub fn with_model_switch(mut self, trace: ModelSwitchTrace) -> Self {
        self.model_switch = Some(trace);
        self
    }

    /// Set lifecycle details.
    pub fn with_lifecycle(mut self, trace: LifecycleTrace) -> Self {
        self.lifecycle = Some(trace);
        self
    }

    /// Set message details.
    pub fn with_message(mut self, trace: MessageTrace) -> Self {
        self.message = Some(trace);
        self
    }

    /// Set subflow path.
    pub fn with_subflow_path(mut self, path: Vec<String>) -> Self {
        self.subflow_path = path;
        self
    }
}

/// Query filters for retrieving execution trace events.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, Type)]
#[ts(export)]
pub struct ExecutionTraceQuery {
    /// Filter by task ID.
    pub task_id: Option<String>,
    /// Filter by agent ID.
    pub agent_id: Option<String>,
    /// Filter by event category.
    pub category: Option<ExecutionTraceCategory>,
    /// Filter by event source.
    pub source: Option<ExecutionTraceSource>,
    /// Start timestamp (inclusive).
    pub from_timestamp: Option<i64>,
    /// End timestamp (inclusive).
    pub to_timestamp: Option<i64>,
    /// Limit number of results.
    pub limit: Option<usize>,
    /// Offset for pagination.
    pub offset: Option<usize>,
}

/// Statistics about execution trace events.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, Type)]
#[ts(export)]
pub struct ExecutionTraceStats {
    /// Total number of events.
    pub total_events: u64,
    /// Number of LLM call events.
    pub llm_call_count: u64,
    /// Number of tool call events.
    pub tool_call_count: u64,
    /// Number of model switch events.
    pub model_switch_count: u64,
    /// Number of lifecycle events.
    pub lifecycle_count: u64,
    /// Number of message events.
    pub message_count: u64,
    /// Total tokens used.
    pub total_tokens: u64,
    /// Total cost in USD.
    pub total_cost_usd: f64,
    /// Time range of events.
    pub time_range: Option<ExecutionTraceTimeRange>,
}

/// Time range of execution trace events.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type)]
#[ts(export)]
pub struct ExecutionTraceTimeRange {
    /// Earliest timestamp.
    #[ts(type = "number")]
    pub earliest: i64,
    /// Latest timestamp.
    #[ts(type = "number")]
    pub latest: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_trace_event_creation() {
        let event = ExecutionTraceEvent::llm_call(
            "task-123",
            "agent-456",
            LlmCallTrace {
                model: "claude-sonnet-4-20250514".to_string(),
                input_tokens: Some(1000),
                output_tokens: Some(500),
                total_tokens: Some(1500),
                cost_usd: Some(0.01),
                duration_ms: Some(1500),
                is_reasoning: Some(false),
                message_count: Some(10),
            },
        );

        assert_eq!(event.task_id, "task-123");
        assert_eq!(event.agent_id, "agent-456");
        assert_eq!(event.category, ExecutionTraceCategory::LlmCall);
        assert!(event.llm_call.is_some());
    }

    #[test]
    fn test_execution_trace_query_default() {
        let query = ExecutionTraceQuery::default();
        assert!(query.task_id.is_none());
        assert!(query.agent_id.is_none());
        assert!(query.limit.is_none());
    }
}
