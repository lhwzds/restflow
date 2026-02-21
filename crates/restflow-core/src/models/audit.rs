//! Audit event models for background agent execution tracking.
//!
//! This module provides structured audit events for tracking:
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
use ts_rs::TS;

/// Audit event category
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventCategory {
    /// LLM API call event
    LlmCall,
    /// Tool invocation event
    ToolCall,
    /// Model switch event
    ModelSwitch,
    /// Lifecycle event (start/end/error)
    Lifecycle,
    /// Agent message event
    Message,
}

/// Source of the audit event
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventSource {
    /// Event from the agent executor
    AgentExecutor,
    /// Event from the runtime
    Runtime,
    /// Event from MCP server
    McpServer,
    /// Event from CLI
    Cli,
}

/// LLM call audit data
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LlmCallAudit {
    /// Model used for the call
    pub model: String,
    /// Number of input tokens
    pub input_tokens: Option<u32>,
    /// Number of output tokens
    pub output_tokens: Option<u32>,
    /// Total tokens used
    pub total_tokens: Option<u32>,
    /// Cost in USD
    pub cost_usd: Option<f64>,
    /// Duration of the call in milliseconds
    pub duration_ms: Option<i64>,
    /// Whether this was a reasoning call
    pub is_reasoning: Option<bool>,
    /// Number of messages in the request
    pub message_count: Option<u32>,
}

/// Tool call audit data
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolCallAudit {
    /// Name of the tool invoked
    pub tool_name: String,
    /// Tool input (truncated for storage efficiency)
    pub input_summary: Option<String>,
    /// Whether the tool succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Duration of the tool execution in milliseconds
    pub duration_ms: Option<i64>,
}

/// Model switch audit data
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ModelSwitchAudit {
    /// Previous model
    pub from_model: String,
    /// Target model
    pub to_model: String,
    /// Reason for switching
    pub reason: Option<String>,
    /// Whether the switch was successful
    pub success: bool,
}

/// Lifecycle event audit data
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LifecycleAudit {
    /// Current status after the lifecycle event
    pub status: String,
    /// Detailed message
    pub message: Option<String>,
    /// Error details if applicable
    pub error: Option<String>,
}

/// Message event audit data
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MessageAudit {
    /// Role of the message sender
    pub role: String,
    /// Message content preview
    pub content_preview: Option<String>,
    /// Number of tool calls in this message
    pub tool_call_count: Option<u32>,
}

/// Unified audit event structure
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AuditEvent {
    /// Unique event ID
    pub id: String,
    /// Task ID this event belongs to
    pub task_id: String,
    /// Agent ID that generated this event
    pub agent_id: String,
    /// Event category
    pub category: AuditEventCategory,
    /// Event source
    pub source: AuditEventSource,
    /// Timestamp (milliseconds since epoch)
    #[ts(type = "number")]
    pub timestamp: i64,
    /// Subflow path for nested agent calls
    #[serde(default)]
    #[ts(type = "string[]")]
    pub subflow_path: Vec<String>,
    /// LLM call details (if category is LlmCall)
    #[serde(default)]
    pub llm_call: Option<LlmCallAudit>,
    /// Tool call details (if category is ToolCall)
    #[serde(default)]
    pub tool_call: Option<ToolCallAudit>,
    /// Model switch details (if category is ModelSwitch)
    #[serde(default)]
    pub model_switch: Option<ModelSwitchAudit>,
    /// Lifecycle details (if category is Lifecycle)
    #[serde(default)]
    pub lifecycle: Option<LifecycleAudit>,
    /// Message details (if category is Message)
    #[serde(default)]
    pub message: Option<MessageAudit>,
}

impl AuditEvent {
    /// Create a new audit event with a generated ID
    pub fn new(
        task_id: impl Into<String>,
        agent_id: impl Into<String>,
        category: AuditEventCategory,
        source: AuditEventSource,
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

    /// Create an LLM call audit event
    pub fn llm_call(
        task_id: impl Into<String>,
        agent_id: impl Into<String>,
        audit: LlmCallAudit,
    ) -> Self {
        Self::new(task_id, agent_id, AuditEventCategory::LlmCall, AuditEventSource::AgentExecutor)
            .with_llm_call(audit)
    }

    /// Create a tool call audit event
    pub fn tool_call(
        task_id: impl Into<String>,
        agent_id: impl Into<String>,
        audit: ToolCallAudit,
    ) -> Self {
        Self::new(task_id, agent_id, AuditEventCategory::ToolCall, AuditEventSource::AgentExecutor)
            .with_tool_call(audit)
    }

    /// Create a model switch audit event
    pub fn model_switch(
        task_id: impl Into<String>,
        agent_id: impl Into<String>,
        audit: ModelSwitchAudit,
    ) -> Self {
        Self::new(task_id, agent_id, AuditEventCategory::ModelSwitch, AuditEventSource::AgentExecutor)
            .with_model_switch(audit)
    }

    /// Create a lifecycle audit event
    pub fn lifecycle(
        task_id: impl Into<String>,
        agent_id: impl Into<String>,
        audit: LifecycleAudit,
    ) -> Self {
        Self::new(task_id, agent_id, AuditEventCategory::Lifecycle, AuditEventSource::Runtime)
            .with_lifecycle(audit)
    }

    /// Create a message audit event
    pub fn message(
        task_id: impl Into<String>,
        agent_id: impl Into<String>,
        audit: MessageAudit,
    ) -> Self {
        Self::new(task_id, agent_id, AuditEventCategory::Message, AuditEventSource::AgentExecutor)
            .with_message(audit)
    }

    /// Set LLM call details
    pub fn with_llm_call(mut self, audit: LlmCallAudit) -> Self {
        self.llm_call = Some(audit);
        self
    }

    /// Set tool call details
    pub fn with_tool_call(mut self, audit: ToolCallAudit) -> Self {
        self.tool_call = Some(audit);
        self
    }

    /// Set model switch details
    pub fn with_model_switch(mut self, audit: ModelSwitchAudit) -> Self {
        self.model_switch = Some(audit);
        self
    }

    /// Set lifecycle details
    pub fn with_lifecycle(mut self, audit: LifecycleAudit) -> Self {
        self.lifecycle = Some(audit);
        self
    }

    /// Set message details
    pub fn with_message(mut self, audit: MessageAudit) -> Self {
        self.message = Some(audit);
        self
    }

    /// Set subflow path
    pub fn with_subflow_path(mut self, path: Vec<String>) -> Self {
        self.subflow_path = path;
        self
    }
}

/// Query filters for retrieving audit events
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AuditQuery {
    /// Filter by task ID
    pub task_id: Option<String>,
    /// Filter by agent ID
    pub agent_id: Option<String>,
    /// Filter by event category
    pub category: Option<AuditEventCategory>,
    /// Filter by event source
    pub source: Option<AuditEventSource>,
    /// Start timestamp (inclusive)
    pub from_timestamp: Option<i64>,
    /// End timestamp (inclusive)
    pub to_timestamp: Option<i64>,
    /// Limit number of results
    pub limit: Option<usize>,
    /// Offset for pagination
    pub offset: Option<usize>,
}

/// Statistics about audit events
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AuditStats {
    /// Total number of events
    pub total_events: u64,
    /// Number of LLM call events
    pub llm_call_count: u64,
    /// Number of tool call events
    pub tool_call_count: u64,
    /// Number of model switch events
    pub model_switch_count: u64,
    /// Number of lifecycle events
    pub lifecycle_count: u64,
    /// Number of message events
    pub message_count: u64,
    /// Total tokens used
    pub total_tokens: u64,
    /// Total cost in USD
    pub total_cost_usd: f64,
    /// Time range of events
    pub time_range: Option<AuditTimeRange>,
}

/// Time range of audit events
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AuditTimeRange {
    /// earliest timestamp
    #[ts(type = "number")]
    pub earliest: i64,
    /// Latest timestamp
    #[ts(type = "number")]
    pub latest: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_event_creation() {
        let event = AuditEvent::llm_call(
            "task-123",
            "agent-456",
            LlmCallAudit {
                model: "claude-3-5-sonnet-20241022".to_string(),
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
        assert_eq!(event.category, AuditEventCategory::LlmCall);
        assert!(event.llm_call.is_some());
    }

    #[test]
    fn test_audit_query_default() {
        let query = AuditQuery::default();
        assert!(query.task_id.is_none());
        assert!(query.agent_id.is_none());
        assert!(query.limit.is_none());
    }
}
