use chrono::Utc;
use restflow_telemetry::RestflowTrace;

use super::{
    ExecutionTraceCategory, ExecutionTraceEvent, ExecutionTraceSource, LifecycleTrace,
    LlmCallTrace, LogRecordTrace, MessageTrace, MetricSampleTrace, ModelSwitchTrace,
    ProviderHealthTrace, ToolCallTrace,
};

pub(crate) fn new_event(
    task_id: impl Into<String>,
    agent_id: impl Into<String>,
    category: ExecutionTraceCategory,
    source: ExecutionTraceSource,
) -> ExecutionTraceEvent {
    ExecutionTraceEvent {
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

pub(crate) fn llm_call(
    task_id: impl Into<String>,
    agent_id: impl Into<String>,
    trace: LlmCallTrace,
) -> ExecutionTraceEvent {
    with_llm_call(
        new_event(
            task_id,
            agent_id,
            ExecutionTraceCategory::LlmCall,
            ExecutionTraceSource::AgentExecutor,
        ),
        trace,
    )
}

pub(crate) fn tool_call(
    task_id: impl Into<String>,
    agent_id: impl Into<String>,
    trace: ToolCallTrace,
) -> ExecutionTraceEvent {
    with_tool_call(
        new_event(
            task_id,
            agent_id,
            ExecutionTraceCategory::ToolCall,
            ExecutionTraceSource::AgentExecutor,
        ),
        trace,
    )
}

pub(crate) fn model_switch(
    task_id: impl Into<String>,
    agent_id: impl Into<String>,
    trace: ModelSwitchTrace,
) -> ExecutionTraceEvent {
    with_model_switch(
        new_event(
            task_id,
            agent_id,
            ExecutionTraceCategory::ModelSwitch,
            ExecutionTraceSource::AgentExecutor,
        ),
        trace,
    )
}

pub(crate) fn lifecycle(
    task_id: impl Into<String>,
    agent_id: impl Into<String>,
    trace: LifecycleTrace,
) -> ExecutionTraceEvent {
    with_lifecycle(
        new_event(
            task_id,
            agent_id,
            ExecutionTraceCategory::Lifecycle,
            ExecutionTraceSource::Runtime,
        ),
        trace,
    )
}

pub(crate) fn message(
    task_id: impl Into<String>,
    agent_id: impl Into<String>,
    trace: MessageTrace,
) -> ExecutionTraceEvent {
    with_message(
        new_event(
            task_id,
            agent_id,
            ExecutionTraceCategory::Message,
            ExecutionTraceSource::AgentExecutor,
        ),
        trace,
    )
}

pub(crate) fn metric_sample(
    task_id: impl Into<String>,
    agent_id: impl Into<String>,
    trace: MetricSampleTrace,
) -> ExecutionTraceEvent {
    with_metric_sample(
        new_event(
            task_id,
            agent_id,
            ExecutionTraceCategory::MetricSample,
            ExecutionTraceSource::Telemetry,
        ),
        trace,
    )
}

pub(crate) fn provider_health(
    task_id: impl Into<String>,
    agent_id: impl Into<String>,
    trace: ProviderHealthTrace,
) -> ExecutionTraceEvent {
    with_provider_health(
        new_event(
            task_id,
            agent_id,
            ExecutionTraceCategory::ProviderHealth,
            ExecutionTraceSource::Telemetry,
        ),
        trace,
    )
}

pub(crate) fn log_record(
    task_id: impl Into<String>,
    agent_id: impl Into<String>,
    trace: LogRecordTrace,
) -> ExecutionTraceEvent {
    with_log_record(
        new_event(
            task_id,
            agent_id,
            ExecutionTraceCategory::LogRecord,
            ExecutionTraceSource::Telemetry,
        ),
        trace,
    )
}

pub(crate) fn with_llm_call(mut event: ExecutionTraceEvent, trace: LlmCallTrace) -> ExecutionTraceEvent {
    event.llm_call = Some(trace);
    event
}

pub(crate) fn with_tool_call(
    mut event: ExecutionTraceEvent,
    trace: ToolCallTrace,
) -> ExecutionTraceEvent {
    event.tool_call = Some(trace);
    event
}

pub(crate) fn with_model_switch(
    mut event: ExecutionTraceEvent,
    trace: ModelSwitchTrace,
) -> ExecutionTraceEvent {
    event.model_switch = Some(trace);
    event
}

pub(crate) fn with_lifecycle(
    mut event: ExecutionTraceEvent,
    trace: LifecycleTrace,
) -> ExecutionTraceEvent {
    event.lifecycle = Some(trace);
    event
}

pub(crate) fn with_message(mut event: ExecutionTraceEvent, trace: MessageTrace) -> ExecutionTraceEvent {
    event.message = Some(trace);
    event
}

pub(crate) fn with_metric_sample(
    mut event: ExecutionTraceEvent,
    trace: MetricSampleTrace,
) -> ExecutionTraceEvent {
    event.metric_sample = Some(trace);
    event
}

pub(crate) fn with_provider_health(
    mut event: ExecutionTraceEvent,
    trace: ProviderHealthTrace,
) -> ExecutionTraceEvent {
    event.provider_health = Some(trace);
    event
}

pub(crate) fn with_log_record(
    mut event: ExecutionTraceEvent,
    trace: LogRecordTrace,
) -> ExecutionTraceEvent {
    event.log_record = Some(trace);
    event
}

#[allow(dead_code)]
pub(crate) fn with_subflow_path(
    mut event: ExecutionTraceEvent,
    path: Vec<String>,
) -> ExecutionTraceEvent {
    event.subflow_path = path;
    event
}

pub(crate) fn with_trace_context(
    mut event: ExecutionTraceEvent,
    trace: &RestflowTrace,
) -> ExecutionTraceEvent {
    event.run_id = Some(trace.run_id.clone());
    event.parent_run_id = trace.parent_run_id.clone();
    event.session_id = Some(trace.session_id.clone());
    event.turn_id = Some(trace.turn_id.clone());
    if event.subflow_path.is_empty() {
        let mut path = Vec::new();
        if let Some(parent_run_id) = trace.parent_run_id.as_ref()
            && !parent_run_id.trim().is_empty()
        {
            path.push(parent_run_id.clone());
        }
        path.push(trace.run_id.clone());
        event.subflow_path = path;
    }
    event
}

pub(crate) fn with_requested_model(
    mut event: ExecutionTraceEvent,
    requested_model: impl Into<String>,
) -> ExecutionTraceEvent {
    event.requested_model = Some(requested_model.into());
    event
}

pub(crate) fn with_effective_model(
    mut event: ExecutionTraceEvent,
    effective_model: impl Into<String>,
) -> ExecutionTraceEvent {
    event.effective_model = Some(effective_model.into());
    event
}

pub(crate) fn with_provider(
    mut event: ExecutionTraceEvent,
    provider: impl Into<String>,
) -> ExecutionTraceEvent {
    event.provider = Some(provider.into());
    event
}

pub(crate) fn with_attempt(mut event: ExecutionTraceEvent, attempt: u32) -> ExecutionTraceEvent {
    event.attempt = Some(attempt);
    event
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builders_assign_expected_category_and_source() {
        let llm = llm_call(
            "task-1",
            "agent-1",
            LlmCallTrace {
                model: "gpt-5".to_string(),
                input_tokens: Some(1),
                output_tokens: Some(2),
                total_tokens: Some(3),
                cost_usd: Some(0.1),
                duration_ms: Some(10),
                is_reasoning: Some(false),
                message_count: Some(1),
            },
        );
        assert_eq!(llm.category, ExecutionTraceCategory::LlmCall);
        assert_eq!(llm.source, ExecutionTraceSource::AgentExecutor);

        let life = lifecycle(
            "task-1",
            "agent-1",
            LifecycleTrace {
                status: "running".to_string(),
                message: None,
                error: None,
                ai_duration_ms: None,
            },
        );
        assert_eq!(life.category, ExecutionTraceCategory::Lifecycle);
        assert_eq!(life.source, ExecutionTraceSource::Runtime);
    }

    #[test]
    fn builder_mutators_assign_trace_context_and_model_fields() {
        let trace = RestflowTrace::new("run-1", "session-1", "task-1", "agent-1");
        let event = with_attempt(
            with_provider(
                with_effective_model(
                    with_requested_model(
                        with_trace_context(
                            with_subflow_path(
                                with_lifecycle(
                                    new_event(
                                        "task-1",
                                        "agent-1",
                                        ExecutionTraceCategory::Lifecycle,
                                        ExecutionTraceSource::Runtime,
                                    ),
                                    LifecycleTrace {
                                        status: "running".to_string(),
                                        message: None,
                                        error: None,
                                        ai_duration_ms: None,
                                    },
                                ),
                                Vec::new(),
                            ),
                            &trace,
                        ),
                        "gpt-5",
                    ),
                    "gpt-5-mini",
                ),
                "openai",
            ),
            2,
        );

        assert_eq!(event.run_id.as_deref(), Some("run-1"));
        assert_eq!(event.session_id.as_deref(), Some("session-1"));
        assert_eq!(event.turn_id.as_deref(), Some("run-run-1"));
        assert_eq!(event.subflow_path, vec!["run-1".to_string()]);
        assert_eq!(event.requested_model.as_deref(), Some("gpt-5"));
        assert_eq!(event.effective_model.as_deref(), Some("gpt-5-mini"));
        assert_eq!(event.provider.as_deref(), Some("openai"));
        assert_eq!(event.attempt, Some(2));
    }
}
