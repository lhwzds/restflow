//! Execution telemetry models for runtime tracking and querying.
//!
//! This module provides the canonical event schema used across:
//! - Execution traces and timelines
//! - Tool-call reconstruction
//! - Model-switch visibility
//! - Metrics, provider health, and structured execution logs

use chrono::Utc;
use restflow_telemetry::RestflowTrace;
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
    /// Metric sample event.
    MetricSample,
    /// Provider health projection event.
    ProviderHealth,
    /// Structured execution log event.
    LogRecord,
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
    /// Event from telemetry projector.
    Telemetry,
}

/// Tool call phase represented in the telemetry stream.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallPhase {
    Started,
    Completed,
}

/// Metric dimension attached to a metric sample.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[ts(export)]
pub struct MetricDimension {
    pub key: String,
    pub value: String,
}

/// LLM call trace data.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq)]
#[ts(export)]
pub struct LlmCallTrace {
    /// Canonical model used for the call.
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
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq)]
#[ts(export)]
pub struct ToolCallTrace {
    /// Phase of the tool event.
    pub phase: ToolCallPhase,
    /// Stable tool call ID.
    pub tool_call_id: String,
    /// Name of the tool invoked.
    pub tool_name: String,
    /// Full tool input payload when captured.
    pub input: Option<String>,
    /// Optional summarized tool input.
    pub input_summary: Option<String>,
    /// Full tool output payload when captured.
    pub output: Option<String>,
    /// File reference for full output payload.
    pub output_ref: Option<String>,
    /// Whether the tool succeeded.
    pub success: Option<bool>,
    /// Error message if failed.
    pub error: Option<String>,
    /// Duration of the tool execution in milliseconds.
    pub duration_ms: Option<i64>,
}

/// Tool completion payload used by runtime helpers that consume tool-call outputs.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
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
    #[ts(type = "number | null")]
    pub duration_ms: Option<u64>,
    /// Optional error text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Model switch trace data.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
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
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[ts(export)]
pub struct LifecycleTrace {
    /// Current status after the lifecycle event.
    pub status: String,
    /// Detailed message.
    pub message: Option<String>,
    /// Error details if applicable.
    pub error: Option<String>,
    /// AI execution duration in milliseconds when available.
    pub ai_duration_ms: Option<i64>,
}

/// Message event trace data.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[ts(export)]
pub struct MessageTrace {
    /// Role of the message sender.
    pub role: String,
    /// Message content preview.
    pub content_preview: Option<String>,
    /// Number of tool calls in this message.
    pub tool_call_count: Option<u32>,
}

/// Metric sample trace data.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq)]
#[ts(export)]
pub struct MetricSampleTrace {
    pub name: String,
    pub value: f64,
    pub unit: Option<String>,
    pub dimensions: Vec<MetricDimension>,
}

/// Provider health trace data.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[ts(export)]
pub struct ProviderHealthTrace {
    pub provider: String,
    pub model: Option<String>,
    pub status: String,
    pub reason: Option<String>,
    pub error_kind: Option<String>,
}

/// Structured execution log field.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[ts(export)]
pub struct ExecutionLogField {
    pub key: String,
    pub value: String,
}

/// Structured execution log record.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[ts(export)]
pub struct LogRecordTrace {
    pub level: String,
    pub message: String,
    pub fields: Vec<ExecutionLogField>,
}

/// Unified execution trace event structure.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq)]
#[ts(export)]
pub struct ExecutionTraceEvent {
    /// Unique event ID.
    pub id: String,
    /// Task or execution scope ID this event belongs to.
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
    /// Run ID for trace grouping.
    pub run_id: Option<String>,
    /// Parent run ID for nested traces.
    pub parent_run_id: Option<String>,
    /// Session ID when applicable.
    pub session_id: Option<String>,
    /// Turn ID when applicable.
    pub turn_id: Option<String>,
    /// Requested model for this execution scope.
    pub requested_model: Option<String>,
    /// Effective canonical model for this event or attempt.
    pub effective_model: Option<String>,
    /// Provider for the requested/effective model.
    pub provider: Option<String>,
    /// Attempt number for retries or failover.
    pub attempt: Option<u32>,
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
    /// Metric sample details (if category is MetricSample).
    #[serde(default)]
    pub metric_sample: Option<MetricSampleTrace>,
    /// Provider health details (if category is ProviderHealth).
    #[serde(default)]
    pub provider_health: Option<ProviderHealthTrace>,
    /// Structured log record details (if category is LogRecord).
    #[serde(default)]
    pub log_record: Option<LogRecordTrace>,
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
            run_id: None,
            parent_run_id: None,
            session_id: None,
            turn_id: None,
            requested_model: None,
            effective_model: None,
            provider: None,
            attempt: None,
            llm_call: None,
            tool_call: None,
            model_switch: None,
            lifecycle: None,
            message: None,
            metric_sample: None,
            provider_health: None,
            log_record: None,
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

    /// Create a metric sample trace event.
    pub fn metric_sample(
        task_id: impl Into<String>,
        agent_id: impl Into<String>,
        trace: MetricSampleTrace,
    ) -> Self {
        Self::new(
            task_id,
            agent_id,
            ExecutionTraceCategory::MetricSample,
            ExecutionTraceSource::Telemetry,
        )
        .with_metric_sample(trace)
    }

    /// Create a provider health trace event.
    pub fn provider_health(
        task_id: impl Into<String>,
        agent_id: impl Into<String>,
        trace: ProviderHealthTrace,
    ) -> Self {
        Self::new(
            task_id,
            agent_id,
            ExecutionTraceCategory::ProviderHealth,
            ExecutionTraceSource::Telemetry,
        )
        .with_provider_health(trace)
    }

    /// Create a structured log event.
    pub fn log_record(
        task_id: impl Into<String>,
        agent_id: impl Into<String>,
        trace: LogRecordTrace,
    ) -> Self {
        Self::new(
            task_id,
            agent_id,
            ExecutionTraceCategory::LogRecord,
            ExecutionTraceSource::Telemetry,
        )
        .with_log_record(trace)
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

    /// Set metric sample details.
    pub fn with_metric_sample(mut self, trace: MetricSampleTrace) -> Self {
        self.metric_sample = Some(trace);
        self
    }

    /// Set provider health details.
    pub fn with_provider_health(mut self, trace: ProviderHealthTrace) -> Self {
        self.provider_health = Some(trace);
        self
    }

    /// Set structured log details.
    pub fn with_log_record(mut self, trace: LogRecordTrace) -> Self {
        self.log_record = Some(trace);
        self
    }

    /// Set subflow path.
    pub fn with_subflow_path(mut self, path: Vec<String>) -> Self {
        self.subflow_path = path;
        self
    }

    /// Attach trace context derived from a RestFlow trace descriptor.
    pub fn with_trace_context(mut self, trace: &RestflowTrace) -> Self {
        self.run_id = Some(trace.run_id.clone());
        self.parent_run_id = trace.parent_run_id.clone();
        self.session_id = Some(trace.session_id.clone());
        self.turn_id = Some(trace.turn_id.clone());
        if self.subflow_path.is_empty() {
            let mut path = Vec::new();
            if let Some(parent_run_id) = trace.parent_run_id.as_ref()
                && !parent_run_id.trim().is_empty()
            {
                path.push(parent_run_id.clone());
            }
            path.push(trace.run_id.clone());
            self.subflow_path = path;
        }
        self
    }

    /// Set requested model.
    pub fn with_requested_model(mut self, requested_model: impl Into<String>) -> Self {
        self.requested_model = Some(requested_model.into());
        self
    }

    /// Set effective model.
    pub fn with_effective_model(mut self, effective_model: impl Into<String>) -> Self {
        self.effective_model = Some(effective_model.into());
        self
    }

    /// Set provider.
    pub fn with_provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = Some(provider.into());
        self
    }

    /// Set attempt number.
    pub fn with_attempt(mut self, attempt: u32) -> Self {
        self.attempt = Some(attempt);
        self
    }
}

/// Query filters for retrieving execution trace events.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, Type, PartialEq)]
#[ts(export)]
pub struct ExecutionTraceQuery {
    /// Filter by task ID.
    pub task_id: Option<String>,
    /// Filter by run ID.
    pub run_id: Option<String>,
    /// Filter by session ID.
    pub session_id: Option<String>,
    /// Filter by turn ID.
    pub turn_id: Option<String>,
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
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, Type, PartialEq)]
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
    /// Number of metric sample events.
    pub metric_sample_count: u64,
    /// Number of provider health events.
    pub provider_health_count: u64,
    /// Number of structured log events.
    pub log_record_count: u64,
    /// Total tokens used.
    pub total_tokens: u64,
    /// Total cost in USD.
    pub total_cost_usd: f64,
    /// Time range of events.
    pub time_range: Option<ExecutionTraceTimeRange>,
}

/// Time range of execution trace events.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[ts(export)]
pub struct ExecutionTraceTimeRange {
    /// Earliest timestamp.
    #[ts(type = "number")]
    pub earliest: i64,
    /// Latest timestamp.
    #[ts(type = "number")]
    pub latest: i64,
}

/// Timeline payload for a trace or execution scope.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq)]
#[ts(export)]
pub struct ExecutionTimeline {
    pub events: Vec<ExecutionTraceEvent>,
    pub stats: ExecutionTraceStats,
}

/// Query for metric samples.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, Type, PartialEq)]
#[ts(export)]
pub struct ExecutionMetricQuery {
    pub task_id: Option<String>,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub metric_name: Option<String>,
    pub limit: Option<usize>,
}

/// Aggregated execution metrics response.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, Type, PartialEq)]
#[ts(export)]
pub struct ExecutionMetricsResponse {
    pub samples: Vec<ExecutionTraceEvent>,
}

/// Query for provider health snapshots.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[ts(export)]
pub struct ProviderHealthQuery {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub limit: Option<usize>,
}

/// Provider health query response.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, Type, PartialEq)]
#[ts(export)]
pub struct ProviderHealthResponse {
    pub events: Vec<ExecutionTraceEvent>,
}

/// Query for structured execution logs.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[ts(export)]
pub struct ExecutionLogQuery {
    pub task_id: Option<String>,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub level: Option<String>,
    pub limit: Option<usize>,
}

/// Structured execution logs response.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, Type, PartialEq)]
#[ts(export)]
pub struct ExecutionLogResponse {
    pub events: Vec<ExecutionTraceEvent>,
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
        )
        .with_requested_model("claude-sonnet-4-20250514")
        .with_effective_model("claude-sonnet-4-20250514");

        assert_eq!(event.task_id, "task-123");
        assert_eq!(event.agent_id, "agent-456");
        assert_eq!(event.category, ExecutionTraceCategory::LlmCall);
        assert!(event.llm_call.is_some());
        assert_eq!(
            event.requested_model.as_deref(),
            Some("claude-sonnet-4-20250514")
        );
    }

    #[test]
    fn test_execution_trace_query_default() {
        let query = ExecutionTraceQuery::default();
        assert!(query.task_id.is_none());
        assert!(query.agent_id.is_none());
        assert!(query.limit.is_none());
    }

    #[test]
    fn test_trace_context_assignment() {
        let trace = RestflowTrace::new("run-1", "session-1", "task-1", "agent-1");
        let event = ExecutionTraceEvent::lifecycle(
            "task-1",
            "agent-1",
            LifecycleTrace {
                status: "running".to_string(),
                message: None,
                error: None,
                ai_duration_ms: None,
            },
        )
        .with_trace_context(&trace);
        assert_eq!(event.run_id.as_deref(), Some("run-1"));
        assert_eq!(event.session_id.as_deref(), Some("session-1"));
        assert_eq!(event.turn_id.as_deref(), Some("run-run-1"));
    }

    #[test]
    fn export_bindings_tool_call_completion() {
        ToolCallCompletion::export_to_string(&ts_rs::Config::default()).expect("ts export");
    }
}
