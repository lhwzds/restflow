use restflow_telemetry::{
    ExecutionEvent, ExecutionEventEnvelope, ExecutionLogRecord, ExecutionMetricSample,
    ProviderHealthChanged,
};

use crate::models::{
    ExecutionStepInfo, ExecutionTraceCategory, ExecutionTraceEvent, LifecycleTrace, LogRecordTrace,
    MetricDimension, MetricSampleTrace, ModelSwitchTrace, ProviderHealthTrace, ToolCallPhase,
    ToolCallTrace,
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
    trace: restflow_telemetry::RestflowTrace,
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
                .map(|dimension| restflow_telemetry::ExecutionMetricDimension {
                    key: dimension.key,
                    value: dimension.value,
                })
                .collect(),
        }),
    )
}

pub fn build_provider_health_event(
    trace: restflow_telemetry::RestflowTrace,
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
    trace: restflow_telemetry::RestflowTrace,
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
                .map(|field| restflow_telemetry::ExecutionLogField {
                    key: field.key,
                    value: field.value,
                })
                .collect(),
        }),
    )
}
