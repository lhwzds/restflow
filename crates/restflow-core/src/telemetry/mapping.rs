use restflow_telemetry::{
    ExecutionEvent, ExecutionEventEnvelope, ExecutionLogRecord, ExecutionMetricSample,
    ProviderHealthChanged,
};

use crate::models::{
    ExecutionStepInfo, ExecutionTraceCategory, ExecutionTraceEvent, LifecycleTrace, LogRecordTrace,
    MetricDimension, MetricSampleTrace, ModelSwitchTrace, ProviderHealthTrace, ToolCallPhase,
    ToolCallTrace, execution_trace_builders,
};

/// Build persisted execution steps from unified execution-trace events.
pub fn build_execution_steps(events: &[ExecutionTraceEvent]) -> Vec<ExecutionStepInfo> {
    let mut sorted_events = events.iter().collect::<Vec<_>>();
    sorted_events.sort_by(|left, right| {
        left.timestamp
            .cmp(&right.timestamp)
            .then_with(|| left.id.cmp(&right.id))
    });

    let mut steps = Vec::new();
    for event in sorted_events {
        match event.category {
            ExecutionTraceCategory::ToolCall => {
                let Some(tool_call) = event.tool_call.as_ref() else {
                    continue;
                };
                if tool_call.phase != ToolCallPhase::Completed {
                    continue;
                }
                steps.push(build_tool_call_step(tool_call));
            }
            ExecutionTraceCategory::LlmCall => {
                let Some(llm_call) = event.llm_call.as_ref() else {
                    continue;
                };
                steps.push(build_llm_call_step(event, llm_call));
            }
            ExecutionTraceCategory::ModelSwitch => {
                let Some(model_switch) = event.model_switch.as_ref() else {
                    continue;
                };
                steps.push(build_model_switch_step(model_switch));
            }
            ExecutionTraceCategory::Lifecycle => {
                let Some(lifecycle) = event.lifecycle.as_ref() else {
                    continue;
                };
                if let Some(step) = build_lifecycle_step(lifecycle) {
                    steps.push(step);
                }
            }
            _ => {}
        }
    }

    steps
}

fn build_tool_call_step(tool_call: &ToolCallTrace) -> ExecutionStepInfo {
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
    let mut step = ExecutionStepInfo::new("tool_call", tool_name.to_string()).with_status(status);
    if let Some(duration_ms) = to_duration_ms(tool_call.duration_ms) {
        step = step.with_duration(duration_ms);
    }
    step
}

fn build_llm_call_step(
    event: &ExecutionTraceEvent,
    llm_call: &crate::models::LlmCallTrace,
) -> ExecutionStepInfo {
    let model_candidates = [
        llm_call.model.as_str(),
        event.effective_model.as_deref().unwrap_or_default(),
        event.requested_model.as_deref().unwrap_or_default(),
    ];
    let model_name = first_non_empty(&model_candidates).unwrap_or("llm");

    let mut step =
        ExecutionStepInfo::new("llm_call", model_name.to_string()).with_status("completed");
    if let Some(duration_ms) = to_duration_ms(llm_call.duration_ms) {
        step = step.with_duration(duration_ms);
    }
    step
}

fn build_model_switch_step(model_switch: &ModelSwitchTrace) -> ExecutionStepInfo {
    let from_model = if model_switch.from_model.trim().is_empty() {
        "unknown"
    } else {
        model_switch.from_model.as_str()
    };
    let to_model = if model_switch.to_model.trim().is_empty() {
        "unknown"
    } else {
        model_switch.to_model.as_str()
    };
    let status = if model_switch.success {
        "completed"
    } else {
        "failed"
    };

    ExecutionStepInfo::new("model_switch", format!("{from_model} -> {to_model}"))
        .with_status(status)
}

fn build_lifecycle_step(lifecycle: &LifecycleTrace) -> Option<ExecutionStepInfo> {
    let status = match lifecycle.status.as_str() {
        "run_failed" | "turn_failed" => "failed",
        "run_interrupted" | "turn_interrupted" => "failed",
        _ => return None,
    };

    let mut name = lifecycle.status.clone();
    if let Some(error) = first_non_empty(&[
        lifecycle.error.as_deref().unwrap_or_default(),
        lifecycle.message.as_deref().unwrap_or_default(),
    ]) {
        name.push_str(": ");
        name.push_str(&truncate_step_name(error, 72));
    }

    let mut step = ExecutionStepInfo::new("lifecycle", name).with_status(status);
    if let Some(duration_ms) = to_duration_ms(lifecycle.ai_duration_ms) {
        step = step.with_duration(duration_ms);
    }
    Some(step)
}

fn to_duration_ms(value: Option<i64>) -> Option<u64> {
    value.and_then(|duration_ms| u64::try_from(duration_ms).ok())
}

fn first_non_empty<'a>(values: &'a [&'a str]) -> Option<&'a str> {
    values
        .iter()
        .copied()
        .find(|value| !value.trim().is_empty())
        .map(str::trim)
}

fn truncate_step_name(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    let truncated = trimmed.chars().take(max_chars).collect::<String>();
    if trimmed.chars().count() > max_chars {
        format!("{truncated}...")
    } else {
        truncated
    }
}

pub fn execution_event_to_trace_event(event: &ExecutionEventEnvelope) -> ExecutionTraceEvent {
    let mut trace_event = match &event.event {
        ExecutionEvent::RunStarted => execution_trace_builders::lifecycle(
            event.trace.scope_id.clone(),
            event.trace.actor_id.clone(),
            LifecycleTrace {
                status: "run_started".to_string(),
                message: Some(format!("Run started: {}", event.trace.run_id)),
                error: None,
                ai_duration_ms: None,
            },
        ),
        ExecutionEvent::RunCompleted { ai_duration_ms } => execution_trace_builders::lifecycle(
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
        } => execution_trace_builders::lifecycle(
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
        } => execution_trace_builders::lifecycle(
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
        } => execution_trace_builders::model_switch(
            event.trace.scope_id.clone(),
            event.trace.actor_id.clone(),
            ModelSwitchTrace {
                from_model: from_model.clone(),
                to_model: to_model.clone(),
                reason: reason.clone(),
                success: *success,
            },
        ),
        ExecutionEvent::LlmCall(trace) => execution_trace_builders::llm_call(
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
        ExecutionEvent::ToolCallStarted(trace) => execution_trace_builders::tool_call(
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
        ExecutionEvent::ToolCallCompleted(trace) => execution_trace_builders::tool_call(
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
        ExecutionEvent::Message(trace) => execution_trace_builders::message(
            event.trace.scope_id.clone(),
            event.trace.actor_id.clone(),
            crate::models::MessageTrace {
                role: trace.role.clone(),
                content_preview: trace.content_preview.clone(),
                tool_call_count: trace.tool_call_count,
            },
        ),
        ExecutionEvent::MetricSample(trace) => execution_trace_builders::metric_sample(
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
        ExecutionEvent::ProviderHealthChanged(trace) => execution_trace_builders::provider_health(
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
        ExecutionEvent::LogRecord(trace) => execution_trace_builders::log_record(
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
    trace_event = execution_trace_builders::with_trace_context(trace_event, &event.trace);
    if let Some(requested_model) = event.requested_model.as_ref() {
        trace_event =
            execution_trace_builders::with_requested_model(trace_event, requested_model.clone());
    }
    if let Some(effective_model) = event.effective_model.as_ref() {
        trace_event =
            execution_trace_builders::with_effective_model(trace_event, effective_model.clone());
    }
    if let Some(provider) = event.provider.as_ref() {
        trace_event = execution_trace_builders::with_provider(trace_event, provider.clone());
    }
    if let Some(attempt) = event.attempt {
        trace_event = execution_trace_builders::with_attempt(trace_event, attempt);
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

#[cfg(test)]
mod tests {
    use super::build_execution_steps;
    use crate::models::{
        LifecycleTrace, LlmCallTrace, ModelSwitchTrace, ToolCallPhase, ToolCallTrace,
        execution_trace_builders,
    };

    #[test]
    fn build_execution_steps_includes_non_tool_events_in_time_order() {
        let mut run_started = execution_trace_builders::lifecycle(
            "session-1",
            "agent-1",
            LifecycleTrace {
                status: "run_started".to_string(),
                message: Some("run started".to_string()),
                error: None,
                ai_duration_ms: None,
            },
        );
        run_started.id = "evt-1".to_string();
        run_started.timestamp = 10;

        let mut llm_call = execution_trace_builders::llm_call(
            "session-1",
            "agent-1",
            LlmCallTrace {
                model: "gpt-5".to_string(),
                input_tokens: Some(10),
                output_tokens: Some(20),
                total_tokens: Some(30),
                cost_usd: Some(0.02),
                duration_ms: Some(450),
                is_reasoning: Some(false),
                message_count: Some(2),
            },
        );
        llm_call.id = "evt-2".to_string();
        llm_call.timestamp = 20;

        let mut model_switch = execution_trace_builders::model_switch(
            "session-1",
            "agent-1",
            ModelSwitchTrace {
                from_model: "gpt-4".to_string(),
                to_model: "gpt-5".to_string(),
                reason: Some("failover".to_string()),
                success: true,
            },
        );
        model_switch.id = "evt-3".to_string();
        model_switch.timestamp = 30;

        let mut tool_completed = execution_trace_builders::tool_call(
            "session-1",
            "agent-1",
            ToolCallTrace {
                phase: ToolCallPhase::Completed,
                tool_call_id: "tool-1".to_string(),
                tool_name: "web_search".to_string(),
                input: None,
                input_summary: Some("{\"query\":\"latest\"}".to_string()),
                output: Some("{\"items\":[]}".to_string()),
                output_ref: None,
                success: Some(true),
                error: None,
                duration_ms: Some(1200),
            },
        );
        tool_completed.id = "evt-4".to_string();
        tool_completed.timestamp = 40;

        let mut run_failed = execution_trace_builders::lifecycle(
            "session-1",
            "agent-1",
            LifecycleTrace {
                status: "run_failed".to_string(),
                message: Some("run failed".to_string()),
                error: Some("upstream timeout".to_string()),
                ai_duration_ms: Some(2400),
            },
        );
        run_failed.id = "evt-5".to_string();
        run_failed.timestamp = 50;

        let steps = build_execution_steps(&[
            tool_completed,
            run_failed,
            model_switch,
            llm_call,
            run_started,
        ]);

        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].step_type, "llm_call");
        assert_eq!(steps[0].name, "gpt-5");
        assert_eq!(steps[0].duration_ms, Some(450));

        assert_eq!(steps[1].step_type, "model_switch");
        assert_eq!(steps[1].name, "gpt-4 -> gpt-5");
        assert_eq!(steps[1].status, "completed");

        assert_eq!(steps[2].step_type, "tool_call");
        assert_eq!(steps[2].name, "web_search");
        assert_eq!(steps[2].duration_ms, Some(1200));

        assert_eq!(steps[3].step_type, "lifecycle");
        assert_eq!(steps[3].status, "failed");
        assert!(steps[3].name.contains("run_failed"));
        assert!(steps[3].name.contains("upstream timeout"));
        assert_eq!(steps[3].duration_ms, Some(2400));
    }
}
