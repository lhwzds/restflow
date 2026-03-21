use anyhow::Result;

use crate::models::{
    ExecutionLogQuery, ExecutionLogResponse, ExecutionMetricQuery, ExecutionMetricsResponse,
    ExecutionTimeline, ExecutionTraceCategory, ExecutionTraceEvent, ExecutionTraceQuery,
    ExecutionTraceStats, ProviderHealthQuery, ProviderHealthResponse,
};
use crate::storage::{
    ExecutionTraceStorage, ProviderHealthSnapshotStorage, StructuredExecutionLogStorage,
    TelemetryMetricSampleStorage,
};

pub fn get_execution_timeline(
    execution_traces: &ExecutionTraceStorage,
    query: &ExecutionTraceQuery,
) -> Result<ExecutionTimeline> {
    let mut events = execution_traces.query(query)?;
    events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp).then_with(|| a.id.cmp(&b.id)));
    let stats = execution_trace_stats_for_events(&events);
    Ok(ExecutionTimeline { events, stats })
}

pub fn get_execution_metrics(
    storage: &TelemetryMetricSampleStorage,
    query: &ExecutionMetricQuery,
) -> Result<ExecutionMetricsResponse> {
    let mut samples = storage.list_all()?;
    samples.retain(|event| {
        event.category == ExecutionTraceCategory::MetricSample
            && query
                .task_id
                .as_ref()
                .is_none_or(|value| event.task_id == *value)
            && query
                .session_id
                .as_ref()
                .is_none_or(|value| event.session_id.as_ref() == Some(value))
            && query
                .agent_id
                .as_ref()
                .is_none_or(|value| event.agent_id == *value)
            && query.metric_name.as_ref().is_none_or(|value| {
                event.metric_sample.as_ref().map(|sample| &sample.name) == Some(value)
            })
    });
    samples.sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then_with(|| b.id.cmp(&a.id)));
    if let Some(limit) = query.limit {
        samples.truncate(limit);
    }
    Ok(ExecutionMetricsResponse { samples })
}

pub fn get_provider_health(
    storage: &ProviderHealthSnapshotStorage,
    query: &ProviderHealthQuery,
) -> Result<ProviderHealthResponse> {
    let mut events = storage.list_all()?;
    events.retain(|event| {
        event.category == ExecutionTraceCategory::ProviderHealth
            && query.provider.as_ref().is_none_or(|value| {
                event
                    .provider_health
                    .as_ref()
                    .map(|health| &health.provider)
                    == Some(value)
            })
            && query.model.as_ref().is_none_or(|value| {
                event
                    .provider_health
                    .as_ref()
                    .and_then(|health| health.model.as_ref())
                    == Some(value)
            })
    });
    events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then_with(|| b.id.cmp(&a.id)));
    if let Some(limit) = query.limit {
        events.truncate(limit);
    }
    Ok(ProviderHealthResponse { events })
}

pub fn query_execution_logs(
    storage: &StructuredExecutionLogStorage,
    query: &ExecutionLogQuery,
) -> Result<ExecutionLogResponse> {
    let mut events = storage.list_all()?;
    events.retain(|event| {
        event.category == ExecutionTraceCategory::LogRecord
            && query
                .task_id
                .as_ref()
                .is_none_or(|value| event.task_id == *value)
            && query
                .session_id
                .as_ref()
                .is_none_or(|value| event.session_id.as_ref() == Some(value))
            && query
                .agent_id
                .as_ref()
                .is_none_or(|value| event.agent_id == *value)
            && query
                .level
                .as_ref()
                .is_none_or(|value| event.log_record.as_ref().map(|log| &log.level) == Some(value))
    });
    events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then_with(|| b.id.cmp(&a.id)));
    if let Some(limit) = query.limit {
        events.truncate(limit);
    }
    Ok(ExecutionLogResponse { events })
}

pub fn execution_trace_stats_for_events(events: &[ExecutionTraceEvent]) -> ExecutionTraceStats {
    let mut stats = ExecutionTraceStats {
        total_events: events.len() as u64,
        ..ExecutionTraceStats::default()
    };
    for event in events {
        match event.category {
            ExecutionTraceCategory::LlmCall => {
                stats.llm_call_count += 1;
                if let Some(llm_call) = event.llm_call.as_ref() {
                    stats.total_tokens += llm_call.total_tokens.unwrap_or(0) as u64;
                    stats.total_cost_usd += llm_call.cost_usd.unwrap_or(0.0);
                }
            }
            ExecutionTraceCategory::ToolCall => stats.tool_call_count += 1,
            ExecutionTraceCategory::ModelSwitch => stats.model_switch_count += 1,
            ExecutionTraceCategory::Lifecycle => stats.lifecycle_count += 1,
            ExecutionTraceCategory::Message => stats.message_count += 1,
            ExecutionTraceCategory::MetricSample => stats.metric_sample_count += 1,
            ExecutionTraceCategory::ProviderHealth => stats.provider_health_count += 1,
            ExecutionTraceCategory::LogRecord => stats.log_record_count += 1,
        }
    }
    stats
}
