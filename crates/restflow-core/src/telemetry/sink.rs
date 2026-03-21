use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use restflow_telemetry::{
    DEFAULT_TELEMETRY_TEXT_LIMIT, ExecutionEventEnvelope, RestflowTrace, TelemetrySink,
    normalize_telemetry_preview, sanitize_telemetry_secrets, truncate_telemetry_text,
};

use crate::models::ExecutionTraceCategory;
use crate::storage::{
    ExecutionTraceStorage, ProviderHealthSnapshotStorage, StructuredExecutionLogStorage,
    TelemetryMetricSampleStorage,
};

use super::derive::derive_projection_events;
use super::mapping::execution_event_to_trace_event;
use super::projector::{
    ExecutionTraceProjector, MetricsProjector, ProviderHealthProjector,
    SessionProjectionProjector, StructuredLogProjector, TelemetryProjector,
};

#[derive(Clone)]
pub struct CoreTelemetrySink {
    trace_projector: ExecutionTraceProjector,
    session_projector: SessionProjectionProjector,
    metrics_projector: MetricsProjector,
    provider_health_projector: ProviderHealthProjector,
    structured_log_projector: StructuredLogProjector,
}

impl CoreTelemetrySink {
    pub fn new(
        execution_traces: ExecutionTraceStorage,
        chat_sessions: crate::storage::ChatSessionStorage,
        telemetry_metric_samples: TelemetryMetricSampleStorage,
        provider_health_snapshots: ProviderHealthSnapshotStorage,
        structured_execution_logs: StructuredExecutionLogStorage,
    ) -> Self {
        Self {
            trace_projector: ExecutionTraceProjector::new(execution_traces),
            session_projector: SessionProjectionProjector::new(chat_sessions),
            metrics_projector: MetricsProjector::new(telemetry_metric_samples),
            provider_health_projector: ProviderHealthProjector::new(provider_health_snapshots),
            structured_log_projector: StructuredLogProjector::new(structured_execution_logs),
        }
    }

    fn project_primary_event(&self, event: &crate::models::ExecutionTraceEvent) -> Result<()> {
        self.trace_projector.project(event)?;
        self.session_projector.project(event)?;
        self.metrics_projector.project(event)?;
        self.provider_health_projector.project(event)?;
        self.structured_log_projector.project(event)?;
        Ok(())
    }

    fn project_derived_event(&self, event: &crate::models::ExecutionTraceEvent) -> Result<()> {
        self.trace_projector.project(event)?;
        match event.category {
            ExecutionTraceCategory::MetricSample => self.metrics_projector.project(event)?,
            ExecutionTraceCategory::ProviderHealth => {
                self.provider_health_projector.project(event)?
            }
            ExecutionTraceCategory::LogRecord => self.structured_log_projector.project(event)?,
            _ => {}
        }
        Ok(())
    }
}

#[async_trait]
impl TelemetrySink for CoreTelemetrySink {
    async fn emit(&self, event: ExecutionEventEnvelope) {
        let projected = execution_event_to_trace_event(&event);
        if let Err(error) = self.project_primary_event(&projected) {
            tracing::warn!(
                event_id = %projected.id,
                category = ?projected.category,
                error = %error,
                "Failed to project telemetry event"
            );
            return;
        }

        for derived in derive_projection_events(&projected) {
            if let Err(error) = self.project_derived_event(&derived) {
                tracing::warn!(
                    event_id = %derived.id,
                    category = ?derived.category,
                    error = %error,
                    "Failed to project derived telemetry event"
                );
            }
        }
    }
}

#[derive(Clone)]
pub struct ExecutionTraceSink {
    projector: ExecutionTraceProjector,
}

impl ExecutionTraceSink {
    pub fn new(execution_traces: ExecutionTraceStorage) -> Self {
        Self {
            projector: ExecutionTraceProjector::new(execution_traces),
        }
    }
}

#[async_trait]
impl TelemetrySink for ExecutionTraceSink {
    async fn emit(&self, event: ExecutionEventEnvelope) {
        let projected = execution_event_to_trace_event(&event);
        if let Err(error) = self.projector.project(&projected) {
            tracing::warn!(
                event_id = %projected.id,
                category = ?projected.category,
                error = %error,
                "Failed to append execution trace event"
            );
            return;
        }

        for derived in derive_projection_events(&projected) {
            if let Err(error) = self.projector.project(&derived) {
                tracing::warn!(
                    event_id = %derived.id,
                    category = ?derived.category,
                    error = %error,
                    "Failed to append derived execution trace event"
                );
            }
        }
    }
}

pub fn build_core_telemetry_sink(storage: &crate::storage::Storage) -> Arc<dyn TelemetrySink> {
    Arc::new(CoreTelemetrySink::new(
        storage.execution_traces.clone(),
        storage.chat_sessions.clone(),
        storage.telemetry_metric_samples.clone(),
        storage.provider_health_snapshots.clone(),
        storage.structured_execution_logs.clone(),
    ))
}

pub fn build_execution_trace_sink(
    execution_traces: &ExecutionTraceStorage,
) -> Arc<dyn TelemetrySink> {
    Arc::new(ExecutionTraceSink::new(execution_traces.clone()))
}

pub async fn emit_event(sink: &Arc<dyn TelemetrySink>, event: ExecutionEventEnvelope) {
    sink.emit(event).await;
}

pub async fn emit_run_started(sink: &Arc<dyn TelemetrySink>, trace: RestflowTrace) {
    emit_event(sink, ExecutionEventEnvelope::run_started(trace)).await;
}

pub async fn emit_run_completed(
    sink: &Arc<dyn TelemetrySink>,
    trace: RestflowTrace,
    ai_duration_ms: Option<u64>,
) {
    emit_event(
        sink,
        ExecutionEventEnvelope::run_completed(trace, ai_duration_ms),
    )
    .await;
}

pub async fn emit_run_failed(
    sink: &Arc<dyn TelemetrySink>,
    trace: RestflowTrace,
    error_text: &str,
    ai_duration_ms: Option<u64>,
) {
    let sanitized_error = truncate_telemetry_text(
        &sanitize_telemetry_secrets(error_text),
        DEFAULT_TELEMETRY_TEXT_LIMIT,
    );
    emit_event(
        sink,
        ExecutionEventEnvelope::run_failed(trace, sanitized_error, ai_duration_ms),
    )
    .await;
}

pub async fn emit_run_interrupted(
    sink: &Arc<dyn TelemetrySink>,
    trace: RestflowTrace,
    reason: &str,
    ai_duration_ms: Option<u64>,
) {
    let sanitized_reason = truncate_telemetry_text(
        &sanitize_telemetry_secrets(reason),
        DEFAULT_TELEMETRY_TEXT_LIMIT,
    );
    emit_event(
        sink,
        ExecutionEventEnvelope::run_interrupted(trace, sanitized_reason, ai_duration_ms),
    )
    .await;
}

pub async fn emit_message(
    sink: &Arc<dyn TelemetrySink>,
    trace: RestflowTrace,
    role: &str,
    content: &str,
) {
    emit_event(
        sink,
        ExecutionEventEnvelope::message(
            trace,
            role.to_string(),
            normalize_telemetry_preview(content, DEFAULT_TELEMETRY_TEXT_LIMIT),
            None,
        ),
    )
    .await;
}
