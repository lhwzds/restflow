use crate::models::{
    ExecutionTraceCategory, ExecutionTraceEvent, MetricDimension, MetricSampleTrace, ModelId,
    ProviderHealthTrace, ToolCallPhase, execution_trace_builders,
};

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

pub fn derive_metric_events(event: &ExecutionTraceEvent) -> Vec<ExecutionTraceEvent> {
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
                    execution_trace_builders::metric_sample(
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
                    execution_trace_builders::metric_sample(
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
                    execution_trace_builders::metric_sample(
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
                    execution_trace_builders::metric_sample(
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
                execution_trace_builders::metric_sample(
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

pub fn derive_provider_health_events(event: &ExecutionTraceEvent) -> Vec<ExecutionTraceEvent> {
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
                    execution_trace_builders::provider_health(
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
                    execution_trace_builders::provider_health(
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

pub fn derive_log_events(event: &ExecutionTraceEvent) -> Vec<ExecutionTraceEvent> {
    let mut derived = Vec::new();
    match event.category {
        ExecutionTraceCategory::ModelSwitch => {
            let Some(model_switch) = event.model_switch.as_ref() else {
                return derived;
            };
            if model_switch.reason.as_deref() == Some("failover") {
                derived.push(inherit_trace_context(
                    execution_trace_builders::log_record(
                        event.task_id.clone(),
                        event.agent_id.clone(),
                        crate::models::LogRecordTrace {
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
                    execution_trace_builders::log_record(
                        event.task_id.clone(),
                        event.agent_id.clone(),
                        crate::models::LogRecordTrace {
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

pub fn derive_projection_events(event: &ExecutionTraceEvent) -> Vec<ExecutionTraceEvent> {
    let mut derived = derive_metric_events(event);
    derived.extend(derive_provider_health_events(event));
    derived.extend(derive_log_events(event));
    derived
}
