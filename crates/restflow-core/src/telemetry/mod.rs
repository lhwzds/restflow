//! RestFlow execution telemetry implementation layer.
//!
//! This module owns product-specific projection, persistence, and query logic.
//! The shared event domain lives in the `restflow-telemetry` crate.

mod derive;
mod mapping;
mod projector;
mod query;
mod resolution;
mod sink;

pub use derive::{
    derive_log_events, derive_metric_events, derive_projection_events,
    derive_provider_health_events,
};
pub use mapping::{
    build_execution_steps, build_log_record_event, build_metric_sample_event,
    build_provider_health_event, execution_event_to_trace_event,
};
pub use projector::{
    ExecutionTraceProjector, MetricsProjector, ProviderHealthProjector,
    SessionProjectionProjector, StructuredLogProjector, TelemetryProjector,
};
pub use query::{
    execution_trace_stats_for_events, get_execution_metrics, get_execution_timeline,
    get_provider_health, query_execution_logs,
};
pub use resolution::{ExecutionResolution, ModelSwitchRecord};
pub use sink::{
    CoreTelemetrySink, ExecutionTraceSink, build_core_telemetry_sink,
    build_execution_trace_sink, emit_event, emit_message, emit_run_completed,
    emit_run_failed, emit_run_interrupted, emit_run_started,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        ExecutionLogQuery, ExecutionMetricQuery, ExecutionTraceCategory, ExecutionTraceQuery,
        ExecutionTraceSource, Provider, ProviderHealthQuery,
    };
    use crate::storage::Storage;
    use restflow_telemetry::TelemetrySink;
    use tempfile::tempdir;

    #[tokio::test]
    async fn sink_projects_trace_session_metrics_and_health() {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("telemetry.db");
        let storage = Storage::new(db_path.to_str().expect("db path")).expect("storage");
        let mut session = crate::models::ChatSession::new(
            "agent-1".to_string(),
            "minimax-coding-plan-m2-5-highspeed".to_string(),
        );
        session.id = "session-1".to_string();
        storage
            .chat_sessions
            .create(&session)
            .expect("create session");

        let sink = CoreTelemetrySink::new(
            storage.execution_traces.clone(),
            storage.chat_sessions.clone(),
            storage.telemetry_metric_samples.clone(),
            storage.provider_health_snapshots.clone(),
            storage.structured_execution_logs.clone(),
        );
        let trace =
            restflow_telemetry::RestflowTrace::new("run-1", "session-1", "session-1", "agent-1");
        sink.emit(
            restflow_telemetry::ExecutionEventEnvelope::new(
                trace,
                restflow_telemetry::ExecutionEvent::ModelSwitch {
                    from_model: "minimax-coding-plan-m2-5-highspeed".to_string(),
                    to_model: "minimax-coding-plan-m2-5".to_string(),
                    reason: Some("failover".to_string()),
                    success: true,
                },
            )
            .with_requested_model("minimax-coding-plan-m2-5-highspeed")
            .with_effective_model("minimax-coding-plan-m2-5")
            .with_provider(Provider::MiniMaxCodingPlan.as_canonical_str()),
        )
        .await;

        let llm_trace =
            restflow_telemetry::RestflowTrace::new("run-1", "session-1", "session-1", "agent-1");
        sink.emit(
            restflow_telemetry::ExecutionEventEnvelope::new(
                llm_trace,
                restflow_telemetry::ExecutionEvent::LlmCall(restflow_telemetry::LlmCallPayload {
                    model: "minimax-coding-plan-m2-5".to_string(),
                    input_tokens: Some(120),
                    output_tokens: Some(30),
                    total_tokens: Some(150),
                    cost_usd: Some(0.42),
                    duration_ms: Some(900),
                    is_reasoning: Some(false),
                    message_count: Some(4),
                }),
            )
            .with_requested_model("minimax-coding-plan-m2-5-highspeed")
            .with_effective_model("minimax-coding-plan-m2-5")
            .with_provider(Provider::MiniMaxCodingPlan.as_canonical_str()),
        )
        .await;

        let events = storage
            .execution_traces
            .query(&ExecutionTraceQuery {
                task_id: Some("session-1".to_string()),
                ..ExecutionTraceQuery::default()
            })
            .expect("query");
        assert_eq!(
            events
                .iter()
                .find(|event| event.category == ExecutionTraceCategory::ModelSwitch)
                .and_then(|event| event.effective_model.as_deref()),
            Some("minimax-coding-plan-m2-5")
        );

        let persisted = storage
            .chat_sessions
            .get("session-1")
            .expect("load session")
            .expect("session");
        assert_eq!(
            persisted.metadata.last_model.as_deref(),
            Some("minimax-coding-plan-m2-5")
        );
        assert_eq!(persisted.prompt_tokens, 120);
        assert_eq!(persisted.completion_tokens, 30);
        assert_eq!(persisted.cost, 0.42);

        let metrics = get_execution_metrics(
            &storage.telemetry_metric_samples,
            &ExecutionMetricQuery {
                task_id: Some("session-1".to_string()),
                ..ExecutionMetricQuery::default()
            },
        )
        .expect("metrics");
        assert!(metrics.samples.iter().any(|event| {
            event.metric_sample.as_ref().map(|s| s.name.as_str()) == Some("llm_total_tokens")
        }));
        assert!(metrics.samples.iter().any(|event| {
            event.metric_sample.as_ref().map(|s| s.name.as_str()) == Some("model_failover_count")
        }));

        let provider_health = get_provider_health(
            &storage.provider_health_snapshots,
            &ProviderHealthQuery {
                provider: Some("minimax-coding-plan".to_string()),
                model: Some("minimax-coding-plan-m2-5-highspeed".to_string()),
                limit: Some(10),
            },
        )
        .expect("provider health");
        assert_eq!(provider_health.events.len(), 1);
        assert_eq!(
            provider_health.events[0]
                .provider_health
                .as_ref()
                .map(|value| value.status.as_str()),
            Some("degraded")
        );

        let logs = query_execution_logs(
            &storage.structured_execution_logs,
            &ExecutionLogQuery {
                task_id: Some("session-1".to_string()),
                level: Some("warn".to_string()),
                ..ExecutionLogQuery::default()
            },
        )
        .expect("logs");
        assert_eq!(logs.events.len(), 1);
        assert_eq!(
            logs.events[0]
                .log_record
                .as_ref()
                .map(|value| value.message.as_str()),
            Some(
                "Model failover from minimax-coding-plan-m2-5-highspeed to minimax-coding-plan-m2-5"
            )
        );

        let timeline = get_execution_timeline(
            &storage.execution_traces,
            &ExecutionTraceQuery {
                task_id: Some("session-1".to_string()),
                ..ExecutionTraceQuery::default()
            },
        )
        .expect("timeline");
        assert!(
            timeline
                .events
                .iter()
                .any(|event| event.category == ExecutionTraceCategory::MetricSample)
        );
        assert!(
            timeline
                .events
                .iter()
                .any(|event| event.category == ExecutionTraceCategory::ProviderHealth)
        );
        assert!(
            timeline
                .events
                .iter()
                .any(|event| event.category == ExecutionTraceCategory::LogRecord)
        );
    }

    #[test]
    fn projector_assigns_telemetry_source_categories() {
        let trace =
            restflow_telemetry::RestflowTrace::new("run-1", "session-1", "scope-1", "agent-1");
        let event = build_metric_sample_event(trace, "latency_ms", 42.0, None, Vec::new());
        let projected = execution_event_to_trace_event(&event);
        assert_eq!(projected.category, ExecutionTraceCategory::MetricSample);
        assert_eq!(projected.source, ExecutionTraceSource::Telemetry);
    }
}
