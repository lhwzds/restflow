//! Execution telemetry models for runtime tracking and querying.
//!
//! App-facing execution DTOs are contract-owned and re-exported here so daemon
//! and runtime code can share a single transport schema while keeping runtime
//! builder helpers inside `restflow-core`.

pub use restflow_contracts::request::{
    ExecutionLogField, ExecutionLogQuery, ExecutionLogResponse, ExecutionMetricQuery,
    ExecutionMetricsResponse, ExecutionTimeline, ExecutionTraceCategory, ExecutionTraceEvent,
    ExecutionTraceQuery, ExecutionTraceSource, ExecutionTraceStats, ExecutionTraceTimeRange,
    LifecycleTrace, LlmCallTrace, LogRecordTrace, MessageTrace, MetricDimension,
    MetricSampleTrace, ModelSwitchTrace, ProviderHealthQuery, ProviderHealthResponse,
    ProviderHealthTrace, ToolCallPhase, ToolCallTrace,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use ts_rs::TS;

/// Tool completion payload used by runtime helpers that consume tool-call outputs.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq, Eq)]
#[specta(skip_attr = "ts")]
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

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_contracts::request as contract;

    #[test]
    fn reexported_query_types_round_trip_with_contracts() {
        let trace_query = ExecutionTraceQuery {
            run_id: Some("run-1".to_string()),
            session_id: Some("session-1".to_string()),
            category: Some(ExecutionTraceCategory::ToolCall),
            source: Some(ExecutionTraceSource::Runtime),
            limit: Some(25),
            offset: Some(5),
            ..ExecutionTraceQuery::default()
        };
        let trace_value = serde_json::to_value(&trace_query).expect("serialize trace query");
        let trace_round_trip: contract::ExecutionTraceQuery =
            serde_json::from_value(trace_value).expect("deserialize trace query");
        assert_eq!(trace_round_trip, trace_query);

        let metric_query = ExecutionMetricQuery {
            run_id: Some("run-1".to_string()),
            metric_name: Some("llm_latency_ms".to_string()),
            limit: Some(10),
            ..ExecutionMetricQuery::default()
        };
        let metric_value = serde_json::to_value(&metric_query).expect("serialize metric query");
        let metric_round_trip: contract::ExecutionMetricQuery =
            serde_json::from_value(metric_value).expect("deserialize metric query");
        assert_eq!(metric_round_trip, metric_query);

        let provider_query = ProviderHealthQuery {
            provider: Some("openai".to_string()),
            model: Some("gpt-5".to_string()),
            limit: Some(3),
        };
        let provider_value =
            serde_json::to_value(&provider_query).expect("serialize provider health query");
        let provider_round_trip: contract::ProviderHealthQuery =
            serde_json::from_value(provider_value).expect("deserialize provider health query");
        assert_eq!(provider_round_trip, provider_query);

        let log_query = ExecutionLogQuery {
            run_id: Some("run-1".to_string()),
            level: Some("warn".to_string()),
            limit: Some(50),
            ..ExecutionLogQuery::default()
        };
        let log_value = serde_json::to_value(&log_query).expect("serialize log query");
        let log_round_trip: contract::ExecutionLogQuery =
            serde_json::from_value(log_value).expect("deserialize log query");
        assert_eq!(log_round_trip, log_query);
    }

    #[test]
    fn export_bindings_tool_call_completion() {
        ToolCallCompletion::export_to_string(&ts_rs::Config::default()).expect("ts export");
    }
}
