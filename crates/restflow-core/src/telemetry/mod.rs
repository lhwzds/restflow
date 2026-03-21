mod query;

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use restflow_trace::{
    ExecutionEvent, ExecutionEventEnvelope, ExecutionLogRecord, ExecutionMetricSample,
    ProviderHealthChanged, TelemetrySink,
};

use crate::models::{
    ExecutionStepInfo, ExecutionTraceCategory, ExecutionTraceEvent, LifecycleTrace, LogRecordTrace,
    MetricDimension, MetricSampleTrace, ModelId, ModelSwitchTrace, ProviderHealthTrace,
    ToolCallPhase, ToolCallTrace,
};
use crate::storage::{
    ChatSessionStorage, ExecutionTraceStorage, ProviderHealthSnapshotStorage,
    StructuredExecutionLogStorage, TelemetryMetricSampleStorage,
};

pub use query::{
    get_execution_metrics, get_execution_timeline, get_provider_health, query_execution_logs,
};

/// Build persisted execution steps from unified execution-trace events.
pub fn build_execution_steps(events: &[ExecutionTraceEvent]) -> Vec<ExecutionStepInfo> {
    let mut tool_events = events
        .iter()
        .filter_map(|event| {
            if event.category != ExecutionTraceCategory::ToolCall {
                return None;
            }
            let tool_call = event.tool_call.as_ref()?;
            if tool_call.phase != ToolCallPhase::Completed {
                return None;
            }
            Some((event.timestamp, &event.id, tool_call))
        })
        .collect::<Vec<_>>();
    tool_events.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(right.1)));

    tool_events
        .into_iter()
        .map(|(_, _, tool_call)| {
            let status = if tool_call.success == Some(false) {
                "failed"
            } else {
                "completed"
            };
            let tool_name = if tool_call.tool_name.trim().is_empty() {
                "unknown_tool"
            } else {
                tool_call.tool_name.as_str()
            };
            let mut step =
                ExecutionStepInfo::new("tool_call", tool_name.to_string()).with_status(status);
            if let Some(duration_ms) = tool_call
                .duration_ms
                .and_then(|value| u64::try_from(value).ok())
            {
                step = step.with_duration(duration_ms);
            }
            step
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSwitchRecord {
    pub from_model: ModelId,
    pub to_model: ModelId,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionResolution {
    pub requested_model: ModelId,
    pub effective_model: Option<ModelId>,
    pub switch_chain: Vec<ModelSwitchRecord>,
    pub provider: crate::models::Provider,
    pub attempt_count: u32,
}

impl ExecutionResolution {
    pub fn new(requested_model: ModelId) -> Self {
        Self {
            provider: requested_model.provider(),
            requested_model,
            effective_model: None,
            switch_chain: Vec::new(),
            attempt_count: 0,
        }
    }

    pub fn with_effective_model(mut self, model: ModelId) -> Self {
        self.effective_model = Some(model);
        self
    }

    pub fn with_attempt_count(mut self, attempt_count: u32) -> Self {
        self.attempt_count = attempt_count;
        self
    }

    pub fn push_switch(&mut self, from_model: ModelId, to_model: ModelId, reason: Option<String>) {
        self.switch_chain.push(ModelSwitchRecord {
            from_model,
            to_model,
            reason,
        });
    }
}

pub trait TelemetryProjector: Send + Sync {
    fn project(&self, event: &ExecutionTraceEvent) -> Result<()>;
}

#[derive(Clone)]
pub struct ExecutionTraceProjector {
    storage: ExecutionTraceStorage,
}

impl ExecutionTraceProjector {
    pub fn new(storage: ExecutionTraceStorage) -> Self {
        Self { storage }
    }
}

impl TelemetryProjector for ExecutionTraceProjector {
    fn project(&self, event: &ExecutionTraceEvent) -> Result<()> {
        self.storage.store(event)
    }
}

#[derive(Clone)]
pub struct SessionProjectionProjector {
    storage: ChatSessionStorage,
}

impl SessionProjectionProjector {
    pub fn new(storage: ChatSessionStorage) -> Self {
        Self { storage }
    }
}

impl TelemetryProjector for SessionProjectionProjector {
    fn project(&self, event: &ExecutionTraceEvent) -> Result<()> {
        let Some(session_id) = event.session_id.as_deref() else {
            return Ok(());
        };
        let Some(mut session) = self.storage.get(session_id)? else {
            return Ok(());
        };

        let mut changed = false;

        if let Some(effective_model) = event.effective_model.as_deref()
            && session.metadata.last_model.as_deref() != Some(effective_model)
        {
            session.metadata.last_model = Some(effective_model.to_string());
            changed = true;
        }

        if let Some(llm_call) = event.llm_call.as_ref() {
            if let Some(prompt_tokens) = llm_call.input_tokens {
                session.prompt_tokens += i64::from(prompt_tokens);
                changed = true;
            }
            if let Some(completion_tokens) = llm_call.output_tokens {
                session.completion_tokens += i64::from(completion_tokens);
                changed = true;
            }
            if let Some(cost_usd) = llm_call.cost_usd {
                session.cost += cost_usd;
                changed = true;
            }
        }

        if !changed {
            return Ok(());
        }

        self.storage.update(&session)
    }
}

#[derive(Clone)]
pub struct MetricsProjector {
    storage: TelemetryMetricSampleStorage,
}

impl MetricsProjector {
    pub fn new(storage: TelemetryMetricSampleStorage) -> Self {
        Self { storage }
    }
}

impl TelemetryProjector for MetricsProjector {
    fn project(&self, event: &ExecutionTraceEvent) -> Result<()> {
        if event.category == ExecutionTraceCategory::MetricSample {
            self.storage.store(event)?;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct ProviderHealthProjector {
    storage: ProviderHealthSnapshotStorage,
}

impl ProviderHealthProjector {
    pub fn new(storage: ProviderHealthSnapshotStorage) -> Self {
        Self { storage }
    }
}

impl TelemetryProjector for ProviderHealthProjector {
    fn project(&self, event: &ExecutionTraceEvent) -> Result<()> {
        if event.category == ExecutionTraceCategory::ProviderHealth {
            self.storage.store(event)?;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct StructuredLogProjector {
    storage: StructuredExecutionLogStorage,
}

impl StructuredLogProjector {
    pub fn new(storage: StructuredExecutionLogStorage) -> Self {
        Self { storage }
    }
}

impl TelemetryProjector for StructuredLogProjector {
    fn project(&self, event: &ExecutionTraceEvent) -> Result<()> {
        if event.category == ExecutionTraceCategory::LogRecord {
            self.storage.store(event)?;
        }
        Ok(())
    }
}

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
        chat_sessions: ChatSessionStorage,
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

    fn project_primary_event(&self, event: &ExecutionTraceEvent) -> Result<()> {
        self.trace_projector.project(event)?;
        self.session_projector.project(event)?;
        self.metrics_projector.project(event)?;
        self.provider_health_projector.project(event)?;
        self.structured_log_projector.project(event)?;
        Ok(())
    }

    fn project_derived_event(&self, event: &ExecutionTraceEvent) -> Result<()> {
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

fn inherited_metric_dimensions(
    event: &ExecutionTraceEvent,
    extra: impl IntoIterator<Item = MetricDimension>,
) -> Vec<MetricDimension> {
    let mut dimensions = Vec::new();
    if let Some(provider) = event.provider.as_ref() {
        dimensions.push(MetricDimension {
            key: "provider".to_string(),
            value: provider.clone(),
        });
    }
    if let Some(model) = event.effective_model.as_ref() {
        dimensions.push(MetricDimension {
            key: "model".to_string(),
            value: model.clone(),
        });
    }
    dimensions.extend(extra);
    dimensions
}

fn inherit_trace_context(
    mut derived: ExecutionTraceEvent,
    source: &ExecutionTraceEvent,
) -> ExecutionTraceEvent {
    derived.timestamp = source.timestamp;
    derived.run_id = source.run_id.clone();
    derived.parent_run_id = source.parent_run_id.clone();
    derived.session_id = source.session_id.clone();
    derived.turn_id = source.turn_id.clone();
    derived.subflow_path = source.subflow_path.clone();
    derived.requested_model = source.requested_model.clone();
    derived.effective_model = source.effective_model.clone();
    derived.provider = source.provider.clone();
    derived.attempt = source.attempt;
    derived
}

fn derive_metric_events(event: &ExecutionTraceEvent) -> Vec<ExecutionTraceEvent> {
    let mut derived = Vec::new();
    match event.category {
        ExecutionTraceCategory::LlmCall => {
            let Some(llm_call) = event.llm_call.as_ref() else {
                return derived;
            };
            let llm_dimensions = inherited_metric_dimensions(
                event,
                [MetricDimension {
                    key: "call_type".to_string(),
                    value: "llm".to_string(),
                }],
            );
            if let Some(duration_ms) = llm_call.duration_ms {
                derived.push(inherit_trace_context(
                    ExecutionTraceEvent::metric_sample(
                        event.task_id.clone(),
                        event.agent_id.clone(),
                        MetricSampleTrace {
                            name: "llm_duration_ms".to_string(),
                            value: duration_ms as f64,
                            unit: Some("ms".to_string()),
                            dimensions: llm_dimensions.clone(),
                        },
                    ),
                    event,
                ));
            }
            if let Some(total_tokens) = llm_call.total_tokens {
                derived.push(inherit_trace_context(
                    ExecutionTraceEvent::metric_sample(
                        event.task_id.clone(),
                        event.agent_id.clone(),
                        MetricSampleTrace {
                            name: "llm_total_tokens".to_string(),
                            value: total_tokens as f64,
                            unit: Some("tokens".to_string()),
                            dimensions: llm_dimensions.clone(),
                        },
                    ),
                    event,
                ));
            }
            if let Some(cost_usd) = llm_call.cost_usd {
                derived.push(inherit_trace_context(
                    ExecutionTraceEvent::metric_sample(
                        event.task_id.clone(),
                        event.agent_id.clone(),
                        MetricSampleTrace {
                            name: "llm_cost_usd".to_string(),
                            value: cost_usd,
                            unit: Some("usd".to_string()),
                            dimensions: llm_dimensions,
                        },
                    ),
                    event,
                ));
            }
        }
        ExecutionTraceCategory::ToolCall => {
            let Some(tool_call) = event.tool_call.as_ref() else {
                return derived;
            };
            if tool_call.phase != ToolCallPhase::Completed {
                return derived;
            }
            if let Some(duration_ms) = tool_call.duration_ms {
                derived.push(inherit_trace_context(
                    ExecutionTraceEvent::metric_sample(
                        event.task_id.clone(),
                        event.agent_id.clone(),
                        MetricSampleTrace {
                            name: "tool_duration_ms".to_string(),
                            value: duration_ms as f64,
                            unit: Some("ms".to_string()),
                            dimensions: inherited_metric_dimensions(
                                event,
                                [
                                    MetricDimension {
                                        key: "call_type".to_string(),
                                        value: "tool".to_string(),
                                    },
                                    MetricDimension {
                                        key: "tool".to_string(),
                                        value: tool_call.tool_name.clone(),
                                    },
                                ],
                            ),
                        },
                    ),
                    event,
                ));
            }
        }
        ExecutionTraceCategory::ModelSwitch => {
            let Some(model_switch) = event.model_switch.as_ref() else {
                return derived;
            };
            if model_switch.reason.as_deref() != Some("failover") {
                return derived;
            }
            derived.push(inherit_trace_context(
                ExecutionTraceEvent::metric_sample(
                    event.task_id.clone(),
                    event.agent_id.clone(),
                    MetricSampleTrace {
                        name: "model_failover_count".to_string(),
                        value: 1.0,
                        unit: Some("count".to_string()),
                        dimensions: inherited_metric_dimensions(
                            event,
                            [
                                MetricDimension {
                                    key: "from_model".to_string(),
                                    value: model_switch.from_model.clone(),
                                },
                                MetricDimension {
                                    key: "to_model".to_string(),
                                    value: model_switch.to_model.clone(),
                                },
                            ],
                        ),
                    },
                ),
                event,
            ));
        }
        _ => {}
    }
    derived
}

fn derive_provider_health_events(event: &ExecutionTraceEvent) -> Vec<ExecutionTraceEvent> {
    let mut derived = Vec::new();
    match event.category {
        ExecutionTraceCategory::ModelSwitch => {
            let Some(model_switch) = event.model_switch.as_ref() else {
                return derived;
            };
            if model_switch.reason.as_deref() == Some("failover") {
                let provider = event.provider.clone().unwrap_or_else(|| {
                    ModelId::from_serialized_str(&model_switch.to_model)
                        .map(|model| model.provider().as_canonical_str().to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                });
                derived.push(inherit_trace_context(
                    ExecutionTraceEvent::provider_health(
                        event.task_id.clone(),
                        event.agent_id.clone(),
                        ProviderHealthTrace {
                            provider,
                            model: Some(model_switch.from_model.clone()),
                            status: "degraded".to_string(),
                            reason: Some("failover".to_string()),
                            error_kind: None,
                        },
                    ),
                    event,
                ));
            }
        }
        ExecutionTraceCategory::Lifecycle => {
            let Some(lifecycle) = event.lifecycle.as_ref() else {
                return derived;
            };
            if lifecycle.status == "run_failed" {
                let provider = event.provider.clone().unwrap_or_else(|| {
                    event
                        .effective_model
                        .as_deref()
                        .and_then(ModelId::from_serialized_str)
                        .map(|model| model.provider().as_canonical_str().to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                });
                derived.push(inherit_trace_context(
                    ExecutionTraceEvent::provider_health(
                        event.task_id.clone(),
                        event.agent_id.clone(),
                        ProviderHealthTrace {
                            provider,
                            model: event
                                .effective_model
                                .clone()
                                .or_else(|| event.requested_model.clone()),
                            status: "error".to_string(),
                            reason: lifecycle.error.clone(),
                            error_kind: Some("run_failed".to_string()),
                        },
                    ),
                    event,
                ));
            }
        }
        _ => {}
    }
    derived
}

fn derive_log_events(event: &ExecutionTraceEvent) -> Vec<ExecutionTraceEvent> {
    let mut derived = Vec::new();
    match event.category {
        ExecutionTraceCategory::ModelSwitch => {
            let Some(model_switch) = event.model_switch.as_ref() else {
                return derived;
            };
            if model_switch.reason.as_deref() == Some("failover") {
                derived.push(inherit_trace_context(
                    ExecutionTraceEvent::log_record(
                        event.task_id.clone(),
                        event.agent_id.clone(),
                        LogRecordTrace {
                            level: "warn".to_string(),
                            message: format!(
                                "Model failover from {} to {}",
                                model_switch.from_model, model_switch.to_model
                            ),
                            fields: vec![
                                crate::models::ExecutionLogField {
                                    key: "from_model".to_string(),
                                    value: model_switch.from_model.clone(),
                                },
                                crate::models::ExecutionLogField {
                                    key: "to_model".to_string(),
                                    value: model_switch.to_model.clone(),
                                },
                            ],
                        },
                    ),
                    event,
                ));
            }
        }
        ExecutionTraceCategory::Lifecycle => {
            let Some(lifecycle) = event.lifecycle.as_ref() else {
                return derived;
            };
            if let Some(error) = lifecycle.error.as_ref() {
                let level = if lifecycle.status == "run_interrupted" {
                    "warn"
                } else {
                    "error"
                };
                derived.push(inherit_trace_context(
                    ExecutionTraceEvent::log_record(
                        event.task_id.clone(),
                        event.agent_id.clone(),
                        LogRecordTrace {
                            level: level.to_string(),
                            message: error.clone(),
                            fields: vec![crate::models::ExecutionLogField {
                                key: "status".to_string(),
                                value: lifecycle.status.clone(),
                            }],
                        },
                    ),
                    event,
                ));
            }
        }
        _ => {}
    }
    derived
}

fn derive_projection_events(event: &ExecutionTraceEvent) -> Vec<ExecutionTraceEvent> {
    let mut derived = derive_metric_events(event);
    derived.extend(derive_provider_health_events(event));
    derived.extend(derive_log_events(event));
    derived
}

pub fn execution_event_to_trace_event(event: &ExecutionEventEnvelope) -> ExecutionTraceEvent {
    let mut trace_event = match &event.event {
        ExecutionEvent::RunStarted => ExecutionTraceEvent::lifecycle(
            event.trace.scope_id.clone(),
            event.trace.actor_id.clone(),
            LifecycleTrace {
                status: "run_started".to_string(),
                message: Some(format!("Run started: {}", event.trace.run_id)),
                error: None,
                ai_duration_ms: None,
            },
        ),
        ExecutionEvent::RunCompleted { ai_duration_ms } => ExecutionTraceEvent::lifecycle(
            event.trace.scope_id.clone(),
            event.trace.actor_id.clone(),
            LifecycleTrace {
                status: "run_completed".to_string(),
                message: Some(format!("Run completed: {}", event.trace.run_id)),
                error: None,
                ai_duration_ms: ai_duration_ms.map(|value| value as i64),
            },
        ),
        ExecutionEvent::RunFailed {
            error,
            ai_duration_ms,
        } => ExecutionTraceEvent::lifecycle(
            event.trace.scope_id.clone(),
            event.trace.actor_id.clone(),
            LifecycleTrace {
                status: "run_failed".to_string(),
                message: Some(format!("Run failed: {}", event.trace.run_id)),
                error: Some(error.clone()),
                ai_duration_ms: ai_duration_ms.map(|value| value as i64),
            },
        ),
        ExecutionEvent::RunInterrupted {
            reason,
            ai_duration_ms,
        } => ExecutionTraceEvent::lifecycle(
            event.trace.scope_id.clone(),
            event.trace.actor_id.clone(),
            LifecycleTrace {
                status: "run_interrupted".to_string(),
                message: Some(format!("Run interrupted: {}", event.trace.run_id)),
                error: Some(reason.clone()),
                ai_duration_ms: ai_duration_ms.map(|value| value as i64),
            },
        ),
        ExecutionEvent::ModelSwitch {
            from_model,
            to_model,
            reason,
            success,
        } => ExecutionTraceEvent::model_switch(
            event.trace.scope_id.clone(),
            event.trace.actor_id.clone(),
            ModelSwitchTrace {
                from_model: from_model.clone(),
                to_model: to_model.clone(),
                reason: reason.clone(),
                success: *success,
            },
        ),
        ExecutionEvent::LlmCall(trace) => ExecutionTraceEvent::llm_call(
            event.trace.scope_id.clone(),
            event.trace.actor_id.clone(),
            crate::models::LlmCallTrace {
                model: trace.model.clone(),
                input_tokens: trace.input_tokens,
                output_tokens: trace.output_tokens,
                total_tokens: trace.total_tokens,
                cost_usd: trace.cost_usd,
                duration_ms: trace.duration_ms.map(|value| value as i64),
                is_reasoning: trace.is_reasoning,
                message_count: trace.message_count,
            },
        ),
        ExecutionEvent::ToolCallStarted(trace) => ExecutionTraceEvent::tool_call(
            event.trace.scope_id.clone(),
            event.trace.actor_id.clone(),
            ToolCallTrace {
                phase: ToolCallPhase::Started,
                tool_call_id: trace.tool_call_id.clone(),
                tool_name: trace.tool_name.clone(),
                input: trace.input.clone(),
                input_summary: trace.input.clone(),
                output: None,
                output_ref: None,
                success: None,
                error: None,
                duration_ms: None,
            },
        ),
        ExecutionEvent::ToolCallCompleted(trace) => ExecutionTraceEvent::tool_call(
            event.trace.scope_id.clone(),
            event.trace.actor_id.clone(),
            ToolCallTrace {
                phase: ToolCallPhase::Completed,
                tool_call_id: trace.tool_call_id.clone(),
                tool_name: trace.tool_name.clone(),
                input: trace.input_summary.clone(),
                input_summary: trace.input_summary.clone(),
                output: trace.output.clone(),
                output_ref: trace.output_ref.clone(),
                success: Some(trace.success),
                error: trace.error.clone(),
                duration_ms: trace.duration_ms.map(|value| value as i64),
            },
        ),
        ExecutionEvent::Message(trace) => ExecutionTraceEvent::message(
            event.trace.scope_id.clone(),
            event.trace.actor_id.clone(),
            crate::models::MessageTrace {
                role: trace.role.clone(),
                content_preview: trace.content_preview.clone(),
                tool_call_count: trace.tool_call_count,
            },
        ),
        ExecutionEvent::MetricSample(trace) => ExecutionTraceEvent::metric_sample(
            event.trace.scope_id.clone(),
            event.trace.actor_id.clone(),
            MetricSampleTrace {
                name: trace.name.clone(),
                value: trace.value,
                unit: trace.unit.clone(),
                dimensions: trace
                    .dimensions
                    .iter()
                    .map(|dimension| MetricDimension {
                        key: dimension.key.clone(),
                        value: dimension.value.clone(),
                    })
                    .collect(),
            },
        ),
        ExecutionEvent::ProviderHealthChanged(trace) => ExecutionTraceEvent::provider_health(
            event.trace.scope_id.clone(),
            event.trace.actor_id.clone(),
            ProviderHealthTrace {
                provider: trace.provider.clone(),
                model: trace.model.clone(),
                status: trace.status.clone(),
                reason: trace.reason.clone(),
                error_kind: trace.error_kind.clone(),
            },
        ),
        ExecutionEvent::LogRecord(trace) => ExecutionTraceEvent::log_record(
            event.trace.scope_id.clone(),
            event.trace.actor_id.clone(),
            LogRecordTrace {
                level: trace.level.clone(),
                message: trace.message.clone(),
                fields: trace
                    .fields
                    .iter()
                    .map(|field| crate::models::ExecutionLogField {
                        key: field.key.clone(),
                        value: field.value.clone(),
                    })
                    .collect(),
            },
        ),
    };

    trace_event.id = event.event_id.clone();
    trace_event.timestamp = event.occurred_at_ms;
    trace_event = trace_event.with_trace_context(&event.trace);
    if let Some(requested_model) = event.requested_model.as_ref() {
        trace_event = trace_event.with_requested_model(requested_model.clone());
    }
    if let Some(effective_model) = event.effective_model.as_ref() {
        trace_event = trace_event.with_effective_model(effective_model.clone());
    }
    if let Some(provider) = event.provider.as_ref() {
        trace_event = trace_event.with_provider(provider.clone());
    }
    if let Some(attempt) = event.attempt {
        trace_event = trace_event.with_attempt(attempt);
    }
    trace_event
}

pub fn build_metric_sample_event(
    trace: restflow_trace::RestflowTrace,
    name: impl Into<String>,
    value: f64,
    unit: Option<String>,
    dimensions: Vec<MetricDimension>,
) -> ExecutionEventEnvelope {
    ExecutionEventEnvelope::new(
        trace,
        ExecutionEvent::MetricSample(ExecutionMetricSample {
            name: name.into(),
            value,
            unit,
            dimensions: dimensions
                .into_iter()
                .map(|dimension| restflow_trace::ExecutionMetricDimension {
                    key: dimension.key,
                    value: dimension.value,
                })
                .collect(),
        }),
    )
}

pub fn build_provider_health_event(
    trace: restflow_trace::RestflowTrace,
    provider: impl Into<String>,
    model: Option<String>,
    status: impl Into<String>,
    reason: Option<String>,
    error_kind: Option<String>,
) -> ExecutionEventEnvelope {
    ExecutionEventEnvelope::new(
        trace,
        ExecutionEvent::ProviderHealthChanged(ProviderHealthChanged {
            provider: provider.into(),
            model,
            status: status.into(),
            reason,
            error_kind,
        }),
    )
}

pub fn build_log_record_event(
    trace: restflow_trace::RestflowTrace,
    level: impl Into<String>,
    message: impl Into<String>,
    fields: Vec<crate::models::ExecutionLogField>,
) -> ExecutionEventEnvelope {
    ExecutionEventEnvelope::new(
        trace,
        ExecutionEvent::LogRecord(ExecutionLogRecord {
            level: level.into(),
            message: message.into(),
            fields: fields
                .into_iter()
                .map(|field| restflow_trace::ExecutionLogField {
                    key: field.key,
                    value: field.value,
                })
                .collect(),
        }),
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        ExecutionLogQuery, ExecutionMetricQuery, ExecutionTraceCategory, ExecutionTraceQuery,
        ExecutionTraceSource, Provider, ProviderHealthQuery,
    };
    use crate::storage::Storage;
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
            restflow_trace::RestflowTrace::new("run-1", "session-1", "session-1", "agent-1");
        sink.emit(
            restflow_trace::ExecutionEventEnvelope::new(
                trace,
                restflow_trace::ExecutionEvent::ModelSwitch {
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
            restflow_trace::RestflowTrace::new("run-1", "session-1", "session-1", "agent-1");
        sink.emit(
            restflow_trace::ExecutionEventEnvelope::new(
                llm_trace,
                restflow_trace::ExecutionEvent::LlmCall(restflow_trace::TraceLlmCall {
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
        let trace = restflow_trace::RestflowTrace::new("run-1", "session-1", "scope-1", "agent-1");
        let event = build_metric_sample_event(trace, "latency_ms", 42.0, None, Vec::new());
        let projected = execution_event_to_trace_event(&event);
        assert_eq!(projected.category, ExecutionTraceCategory::MetricSample);
        assert_eq!(projected.source, ExecutionTraceSource::Telemetry);
    }
}
