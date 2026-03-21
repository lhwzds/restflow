use std::sync::Arc;

use serde_json::json;
use tokio::sync::oneshot;
use tokio::time::{Duration, timeout};

use crate::agent::PromptFlags;
use crate::agent::executor::{AgentConfig, AgentExecutor, AgentResult};
use crate::agent::stream::StreamEmitter;
use crate::agent::{AgentState, ResourceUsage};
use crate::error::{AiError, Result};
use crate::llm::{LlmClient, LlmClientFactory};
use crate::tools::{FilteredToolset, ToolRegistry};
use restflow_telemetry::{
    ExecutionEvent, ExecutionEventEnvelope, RestflowTrace, RunTraceContext, TelemetryContext,
    TelemetrySink,
};
use restflow_traits::{AgentOrchestrator, ExecutionMode, ExecutionOutcome, ExecutionPlan, Toolset};

use super::model_resolution::resolve_llm_client;
use super::tracker::SubagentTracker;

pub use restflow_traits::SubagentConfig;
pub use restflow_traits::subagent::{
    InlineSubagentConfig, SpawnHandle, SpawnRequest, SubagentDefLookup, SubagentDefSnapshot,
    SubagentEffectiveLimits, SubagentLimitSource,
};

const TEMPORARY_SUBAGENT_NAME: &str = "Temporary Subagent";
const TEMPORARY_SUBAGENT_PROMPT: &str = "You are a temporary sub-agent. Complete the task autonomously, use tools when needed, and return a concise final result.";

#[derive(Clone)]
struct ResolvedSubagentExecution {
    max_depth: usize,
    effective_limits: SubagentEffectiveLimits,
    trace_context: RunTraceContext,
    telemetry_context: Option<TelemetryContext>,
    telemetry_sink: Option<Arc<dyn TelemetrySink>>,
}

#[derive(Clone, Default)]
pub struct SubagentExecutionBridge {
    pub llm_client_factory: Option<Arc<dyn LlmClientFactory>>,
    pub orchestrator: Option<Arc<dyn AgentOrchestrator>>,
    pub telemetry_sink: Option<Arc<dyn TelemetrySink>>,
}

#[derive(Clone)]
struct SubagentExecutionInvocation {
    llm_client: Arc<dyn LlmClient>,
    tool_registry: Arc<ToolRegistry>,
    bridge: SubagentExecutionBridge,
    request: SpawnRequest,
}

fn normalize_trace_identity(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn resolve_plan_provider(
    request: &SpawnRequest,
    bridge: &SubagentExecutionBridge,
) -> Option<String> {
    match (
        request
            .model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
        request
            .model_provider
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
    ) {
        (_, Some(provider)) => Some(provider.to_string()),
        (Some(model), None) => bridge
            .llm_client_factory
            .as_ref()
            .and_then(|factory| factory.provider_for_model(model))
            .map(|provider| provider.as_str().to_string()),
        (None, None) => None,
    }
}

fn map_subagent_error(success: bool, error: Option<String>) -> Option<String> {
    if success {
        None
    } else {
        error.or_else(|| Some("Sub-agent execution failed".to_string()))
    }
}

fn build_telemetry_context(
    trace_context: &RunTraceContext,
    requested_model: Option<&str>,
    effective_model: &str,
    provider: Option<&str>,
) -> TelemetryContext {
    let mut telemetry_context = TelemetryContext::new(RestflowTrace::from_context(trace_context))
        .with_effective_model(effective_model.to_string())
        .with_attempt(1);
    let requested_model = requested_model
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(effective_model);
    telemetry_context = telemetry_context.with_requested_model(requested_model.to_string());
    if let Some(provider) = provider.map(str::trim).filter(|value| !value.is_empty()) {
        telemetry_context = telemetry_context.with_provider(provider.to_string());
    }
    telemetry_context
}

async fn emit_subagent_lifecycle_event(
    telemetry_sink: Option<&Arc<dyn TelemetrySink>>,
    telemetry_context: Option<&TelemetryContext>,
    event: ExecutionEvent,
) {
    let (Some(telemetry_sink), Some(telemetry_context)) = (telemetry_sink, telemetry_context)
    else {
        return;
    };

    telemetry_sink
        .emit(ExecutionEventEnvelope::from_telemetry_context(
            telemetry_context,
            event,
        ))
        .await;
}

fn resolve_effective_limits(
    agent_def: &SubagentDefSnapshot,
    config: &SubagentConfig,
    request: &SpawnRequest,
) -> SubagentEffectiveLimits {
    let (timeout_secs, timeout_source) = match request.timeout_secs {
        Some(value) => (value, SubagentLimitSource::RequestOverride),
        None => (
            config.subagent_timeout_secs,
            SubagentLimitSource::ConfigDefault,
        ),
    };
    let (max_iterations, max_iterations_source) =
        match request.max_iterations.filter(|value| *value > 0) {
            Some(value) => (value as usize, SubagentLimitSource::RequestOverride),
            None => match agent_def.max_iterations {
                Some(value) => {
                    let source = if request.agent_id.is_some() {
                        SubagentLimitSource::AgentDefinition
                    } else {
                        SubagentLimitSource::InlineConfig
                    };
                    (value as usize, source)
                }
                None => (config.max_iterations, SubagentLimitSource::ConfigDefault),
            },
        };

    SubagentEffectiveLimits {
        timeout_secs,
        timeout_source,
        max_iterations,
        max_iterations_source,
    }
}

fn build_trace_context(task_id: &str, agent_name: &str, request: &SpawnRequest) -> RunTraceContext {
    let parent_execution_id = request.parent_execution_id.clone();
    let trace_session_id = normalize_trace_identity(request.trace_session_id.as_deref())
        .or_else(|| normalize_trace_identity(request.trace_scope_id.as_deref()))
        .or_else(|| normalize_trace_identity(parent_execution_id.as_deref()))
        .unwrap_or_else(|| task_id.to_string());
    let trace_scope_id = normalize_trace_identity(request.trace_scope_id.as_deref())
        .or_else(|| normalize_trace_identity(request.trace_session_id.as_deref()))
        .or_else(|| normalize_trace_identity(parent_execution_id.as_deref()))
        .unwrap_or_else(|| task_id.to_string());

    RunTraceContext {
        run_id: task_id.to_string(),
        actor_id: agent_name.to_string(),
        parent_run_id: parent_execution_id,
        session_id: trace_session_id,
        scope_id: trace_scope_id,
    }
}

fn execution_outcome_from_agent_result(
    result: AgentResult,
    duration_ms: u64,
    agent_name: &str,
    effective_limits: &SubagentEffectiveLimits,
    active_model: &str,
) -> ExecutionOutcome {
    let AgentResult {
        success,
        answer,
        error,
        iterations,
        total_tokens,
        total_cost_usd,
        ..
    } = result;
    let cost_usd = if total_cost_usd > 0.0 {
        Some(total_cost_usd)
    } else {
        None
    };

    ExecutionOutcome {
        success,
        text: Some(answer.unwrap_or_default()),
        error: map_subagent_error(success, error),
        iterations: Some(iterations as u32),
        model: Some(active_model.to_string()),
        duration_ms: Some(duration_ms),
        metadata: Some(json!({
            "agent_name": agent_name,
            "effective_limits": effective_limits,
            "tokens_used": total_tokens,
            "cost_usd": cost_usd,
        })),
        ..ExecutionOutcome::default()
    }
}

/// Execute one subagent request directly without tracker registration or nested spawn.
pub async fn execute_subagent_once(
    definitions: Arc<dyn SubagentDefLookup>,
    llm_client: Arc<dyn LlmClient>,
    tool_registry: Arc<ToolRegistry>,
    config: SubagentConfig,
    request: SpawnRequest,
    bridge: SubagentExecutionBridge,
) -> Result<ExecutionOutcome> {
    let agent_def = resolve_subagent_definition(&definitions, &tool_registry, &request)?;
    let effective_limits = resolve_effective_limits(&agent_def, &config, &request);
    let llm_client = resolve_llm_client(
        request.model.as_deref(),
        request.model_provider.as_deref(),
        agent_def.default_model.as_deref(),
        &llm_client,
        bridge.llm_client_factory.as_ref(),
    )?;
    let active_model = llm_client.model().to_string();
    let trace_context =
        build_trace_context(&uuid::Uuid::new_v4().to_string(), &agent_def.name, &request);
    let requested_model = request
        .model
        .as_deref()
        .or(agent_def.default_model.as_deref());
    let resolved_provider = resolve_plan_provider(&request, &bridge);
    let execution = ResolvedSubagentExecution {
        max_depth: config.max_depth,
        effective_limits: effective_limits.clone(),
        trace_context: trace_context.clone(),
        telemetry_context: bridge.telemetry_sink.as_ref().map(|_| {
            build_telemetry_context(
                &trace_context,
                requested_model,
                &active_model,
                resolved_provider.as_deref(),
            )
        }),
        telemetry_sink: bridge.telemetry_sink.clone(),
    };
    let invocation = SubagentExecutionInvocation {
        llm_client,
        tool_registry,
        bridge: SubagentExecutionBridge {
            llm_client_factory: bridge.llm_client_factory,
            orchestrator: None,
            telemetry_sink: bridge.telemetry_sink,
        },
        request: request.clone(),
    };

    let start = std::time::Instant::now();
    let result = timeout(
        Duration::from_secs(effective_limits.timeout_secs),
        execute_subagent_entry(
            invocation,
            agent_def.clone(),
            request.task,
            execution.clone(),
            None,
        ),
    )
    .await;
    let duration_ms = start.elapsed().as_millis() as u64;

    Ok(match result {
        Ok(Ok(result)) => execution_outcome_from_agent_result(
            result,
            duration_ms,
            &agent_def.name,
            &execution.effective_limits,
            &active_model,
        ),
        Ok(Err(error)) => ExecutionOutcome {
            success: false,
            text: Some(String::new()),
            error: Some(error.to_string()),
            model: Some(active_model),
            duration_ms: Some(duration_ms),
            metadata: Some(json!({
                "agent_name": agent_def.name,
                "effective_limits": execution.effective_limits,
            })),
            ..ExecutionOutcome::default()
        },
        Err(_) => ExecutionOutcome {
            success: false,
            text: Some(String::new()),
            error: Some("Sub-agent timed out".to_string()),
            model: Some(active_model),
            duration_ms: Some(duration_ms),
            metadata: Some(json!({
                "agent_name": agent_def.name,
                "effective_limits": execution.effective_limits,
            })),
            ..ExecutionOutcome::default()
        },
    })
}

/// Spawn a sub-agent with the given request.
pub fn spawn_subagent(
    tracker: Arc<SubagentTracker>,
    definitions: Arc<dyn SubagentDefLookup>,
    llm_client: Arc<dyn LlmClient>,
    tool_registry: Arc<ToolRegistry>,
    config: SubagentConfig,
    request: SpawnRequest,
    bridge: SubagentExecutionBridge,
) -> Result<SpawnHandle> {
    let agent_def = resolve_subagent_definition(&definitions, &tool_registry, &request)?;
    let effective_limits = resolve_effective_limits(&agent_def, &config, &request);

    let llm_client = resolve_llm_client(
        request.model.as_deref(),
        request.model_provider.as_deref(),
        agent_def.default_model.as_deref(),
        &llm_client,
        bridge.llm_client_factory.as_ref(),
    )?;

    let task_id = uuid::Uuid::new_v4().to_string();
    let timeout_secs = effective_limits.timeout_secs;

    let agent_name_for_register = agent_def.name.clone();
    let agent_name_for_return = agent_def.name.clone();
    let task_for_register = request.task.clone();
    let trace_context = build_trace_context(&task_id, &agent_name_for_return, &request);
    let telemetry_sink = bridge
        .telemetry_sink
        .clone()
        .or_else(|| tracker.telemetry_sink());
    let telemetry_context = telemetry_sink.as_ref().map(|_| {
        build_telemetry_context(
            &trace_context,
            request
                .model
                .as_deref()
                .or(agent_def.default_model.as_deref()),
            llm_client.model(),
            resolve_plan_provider(&request, &bridge).as_deref(),
        )
    });
    let max_parallel = config.max_parallel_agents;

    tracker.try_reserve(
        max_parallel,
        task_id.clone(),
        agent_name_for_register,
        task_for_register,
    )?;

    if let (Some(telemetry_sink), Some(telemetry_context)) =
        (telemetry_sink.clone(), telemetry_context.clone())
    {
        tokio::spawn(async move {
            emit_subagent_lifecycle_event(
                Some(&telemetry_sink),
                Some(&telemetry_context),
                ExecutionEvent::RunStarted,
            )
            .await;
        });
    }

    let task = request.task.clone();
    let tracker_clone = tracker.clone();
    let task_id_for_spawn = task_id.clone();
    let invocation = SubagentExecutionInvocation {
        llm_client: llm_client.clone(),
        tool_registry: tool_registry.clone(),
        bridge: bridge.clone(),
        request: request.clone(),
    };
    let execution = ResolvedSubagentExecution {
        max_depth: config.max_depth,
        effective_limits: effective_limits.clone(),
        trace_context: trace_context.clone(),
        telemetry_context: telemetry_context.clone(),
        telemetry_sink: telemetry_sink.clone(),
    };

    let (completion_tx, completion_rx) = oneshot::channel();
    let (start_tx, start_rx) = oneshot::channel();

    let handle = tokio::spawn(async move {
        let task_id = task_id_for_spawn;
        if start_rx.await.is_err() {
            return restflow_traits::SubagentResult {
                success: false,
                output: String::new(),
                summary: None,
                duration_ms: 0,
                tokens_used: None,
                cost_usd: None,
                error: Some("Sub-agent registration interrupted".to_string()),
            };
        }
        let start = std::time::Instant::now();
        let future = execute_subagent_entry(
            invocation.clone(),
            agent_def,
            task.clone(),
            execution.clone(),
            None,
        );
        let result = timeout(Duration::from_secs(timeout_secs), future).await;

        let duration_ms = start.elapsed().as_millis() as u64;

        let (subagent_result, timed_out) = match result {
            Ok(Ok(result)) => {
                let AgentResult {
                    success,
                    answer,
                    error,
                    total_tokens,
                    total_cost_usd,
                    ..
                } = result;
                let cost_usd = if total_cost_usd > 0.0 {
                    Some(total_cost_usd)
                } else {
                    None
                };
                (
                    restflow_traits::SubagentResult {
                        success,
                        output: answer.unwrap_or_default(),
                        summary: None,
                        duration_ms,
                        tokens_used: Some(total_tokens),
                        cost_usd,
                        error: map_subagent_error(success, error),
                    },
                    false,
                )
            }
            Ok(Err(error)) => (
                restflow_traits::SubagentResult {
                    success: false,
                    output: String::new(),
                    summary: None,
                    duration_ms,
                    tokens_used: None,
                    cost_usd: None,
                    error: Some(error.to_string()),
                },
                false,
            ),
            Err(_) => (
                restflow_traits::SubagentResult {
                    success: false,
                    output: String::new(),
                    summary: None,
                    duration_ms,
                    tokens_used: None,
                    cost_usd: None,
                    error: Some("Sub-agent timed out".to_string()),
                },
                true,
            ),
        };

        if timed_out {
            tracker_clone.mark_timed_out_with_result(&task_id, subagent_result.clone());
        } else {
            tracker_clone.mark_completed(&task_id, subagent_result.clone());
        }

        if let (Some(telemetry_sink), Some(telemetry_context)) = (
            execution.telemetry_sink.as_ref(),
            execution.telemetry_context.as_ref(),
        ) {
            let lifecycle_event = if subagent_result.success {
                ExecutionEvent::RunCompleted {
                    ai_duration_ms: Some(duration_ms),
                }
            } else {
                ExecutionEvent::RunFailed {
                    error: subagent_result
                        .error
                        .clone()
                        .unwrap_or_else(|| "Sub-agent execution failed".to_string()),
                    ai_duration_ms: Some(duration_ms),
                }
            };
            emit_subagent_lifecycle_event(
                Some(telemetry_sink),
                Some(telemetry_context),
                lifecycle_event,
            )
            .await;
        }

        let _ = completion_tx.send(subagent_result.clone());
        subagent_result
    });

    if let Err(error) = tracker.attach_execution(task_id.clone(), handle, completion_rx) {
        let failure = restflow_traits::SubagentResult {
            success: false,
            output: String::new(),
            summary: None,
            duration_ms: 0,
            tokens_used: None,
            cost_usd: None,
            error: Some(error.to_string()),
        };
        if let (Some(telemetry_sink), Some(telemetry_context)) =
            (telemetry_sink.as_ref(), telemetry_context.as_ref())
        {
            tokio::spawn({
                let telemetry_sink = Arc::clone(telemetry_sink);
                let telemetry_context = telemetry_context.clone();
                let error = failure
                    .error
                    .clone()
                    .unwrap_or_else(|| "Sub-agent execution failed".to_string());
                async move {
                    emit_subagent_lifecycle_event(
                        Some(&telemetry_sink),
                        Some(&telemetry_context),
                        ExecutionEvent::RunFailed {
                            error,
                            ai_duration_ms: None,
                        },
                    )
                    .await;
                }
            });
        }
        tracker.mark_completed(&task_id, failure);
        return Err(error);
    }

    let _ = start_tx.send(());

    Ok(SpawnHandle {
        id: task_id,
        agent_name: agent_name_for_return,
        effective_limits,
    })
}

fn resolve_subagent_definition(
    definitions: &Arc<dyn SubagentDefLookup>,
    tool_registry: &Arc<ToolRegistry>,
    request: &SpawnRequest,
) -> Result<SubagentDefSnapshot> {
    if let Some(agent_id) = request
        .agent_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        return definitions
            .lookup(agent_id)
            .ok_or_else(|| AiError::Agent(format!("Unknown agent type: {agent_id}")));
    }

    Ok(build_temporary_subagent_definition(
        request.inline.as_ref(),
        tool_registry,
    ))
}

fn build_temporary_subagent_definition(
    inline: Option<&InlineSubagentConfig>,
    tool_registry: &Arc<ToolRegistry>,
) -> SubagentDefSnapshot {
    let fallback_tools = tool_registry
        .list()
        .into_iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let name = inline
        .and_then(|cfg| cfg.name.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(TEMPORARY_SUBAGENT_NAME)
        .to_string();
    let system_prompt = inline
        .and_then(|cfg| cfg.system_prompt.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(TEMPORARY_SUBAGENT_PROMPT)
        .to_string();
    let allowed_tools = inline
        .and_then(|cfg| cfg.allowed_tools.clone())
        .unwrap_or(fallback_tools);

    SubagentDefSnapshot {
        name,
        system_prompt,
        allowed_tools,
        max_iterations: inline
            .and_then(|cfg| cfg.max_iterations)
            .filter(|value| *value > 0),
        default_model: None,
    }
}

async fn execute_subagent(
    llm_client: Arc<dyn LlmClient>,
    tool_registry: Arc<ToolRegistry>,
    agent_def: SubagentDefSnapshot,
    task: String,
    execution: ResolvedSubagentExecution,
    mut emitter: Option<&mut dyn StreamEmitter>,
) -> Result<AgentResult> {
    let registry = build_registry_for_agent(
        &tool_registry,
        &agent_def.allowed_tools,
        1,
        execution.max_depth,
    );
    let registry = Arc::new(registry);

    let agent_config = build_subagent_agent_config(
        task.clone(),
        agent_def.system_prompt.clone(),
        execution.effective_limits.max_iterations,
        &execution.effective_limits,
        execution.trace_context.parent_run_id.as_deref(),
        Some(execution.trace_context.session_id.as_str()),
        Some(execution.trace_context.scope_id.as_str()),
    );
    let agent_config = if let (Some(telemetry_sink), Some(telemetry_context)) = (
        execution.telemetry_sink.clone(),
        execution.telemetry_context.clone(),
    ) {
        agent_config
            .with_telemetry_sink(telemetry_sink)
            .with_telemetry_context(telemetry_context)
    } else {
        agent_config
    };

    let executor = AgentExecutor::new(llm_client, registry);
    let result = if let Some(emitter) = emitter.as_mut() {
        executor.run_with_emitter(agent_config, *emitter).await?
    } else {
        executor.run(agent_config).await?
    };

    Ok(result)
}

async fn execute_subagent_entry(
    invocation: SubagentExecutionInvocation,
    agent_def: SubagentDefSnapshot,
    task: String,
    execution: ResolvedSubagentExecution,
    emitter: Option<&mut dyn StreamEmitter>,
) -> Result<AgentResult> {
    if let Some(orchestrator) = invocation.bridge.orchestrator.clone() {
        execute_subagent_with_orchestrator(
            orchestrator,
            agent_def,
            task,
            execution,
            invocation.request,
            &invocation.bridge,
        )
        .await
    } else {
        execute_subagent(
            invocation.llm_client,
            invocation.tool_registry,
            agent_def,
            task,
            execution,
            emitter,
        )
        .await
    }
}

async fn execute_subagent_with_orchestrator(
    orchestrator: Arc<dyn AgentOrchestrator>,
    agent_def: SubagentDefSnapshot,
    task: String,
    execution: ResolvedSubagentExecution,
    request: SpawnRequest,
    bridge: &SubagentExecutionBridge,
) -> Result<AgentResult> {
    let plan = ExecutionPlan {
        mode: Some(ExecutionMode::Subagent),
        agent_id: request.agent_id.clone(),
        inline_subagent: request.inline.clone(),
        input: Some(task.clone()),
        timeout_secs: Some(execution.effective_limits.timeout_secs),
        model: request.model.clone(),
        provider: resolve_plan_provider(&request, bridge),
        max_iterations: Some(execution.effective_limits.max_iterations as u32),
        parent_execution_id: request.parent_execution_id.clone(),
        trace_session_id: Some(execution.trace_context.session_id.clone()),
        trace_scope_id: Some(execution.trace_context.scope_id.clone()),
        metadata: Some(json!({
            "subagent_name": agent_def.name,
            "effective_limits": execution.effective_limits,
        })),
        ..ExecutionPlan::default()
    };
    plan.validate()
        .map_err(|error| AiError::Agent(error.to_string()))?;

    let outcome = orchestrator
        .run(plan)
        .await
        .map_err(|error| AiError::Agent(error.to_string()))?;
    Ok(agent_result_from_outcome(outcome))
}

fn agent_result_from_outcome(outcome: ExecutionOutcome) -> AgentResult {
    let text = outcome.text.unwrap_or_default();
    let error = if outcome.success {
        None
    } else {
        outcome
            .error
            .or_else(|| Some("Sub-agent execution failed".to_string()))
    };
    let mut state = AgentState::new(uuid::Uuid::new_v4().to_string(), 1);
    if outcome.success {
        state.complete(text.clone());
    } else {
        state.fail(
            error
                .clone()
                .unwrap_or_else(|| "Sub-agent execution failed".to_string()),
        );
    }
    AgentResult {
        success: outcome.success,
        answer: Some(text),
        error,
        iterations: outcome.iterations.unwrap_or_default() as usize,
        total_tokens: 0,
        total_cost_usd: 0.0,
        state,
        resource_usage: ResourceUsage {
            tool_calls: 0,
            wall_clock: Duration::from_millis(outcome.duration_ms.unwrap_or_default()),
            depth: 0,
            total_cost_usd: 0.0,
        },
    }
}

fn build_subagent_agent_config(
    task: String,
    system_prompt: String,
    max_iterations: usize,
    effective_limits: &SubagentEffectiveLimits,
    parent_execution_id: Option<&str>,
    trace_session_id: Option<&str>,
    trace_scope_id: Option<&str>,
) -> AgentConfig {
    let mut agent_config = AgentConfig::new(task);
    agent_config.system_prompt = Some(system_prompt);
    agent_config.max_iterations = max_iterations;
    agent_config.prompt_flags = PromptFlags::new().without_workspace_context();
    agent_config.yolo_mode = true;
    agent_config = agent_config.with_context(
        "execution_context",
        json!({
            "role": "subagent",
            "parent_execution_id": parent_execution_id,
            "trace_session_id": trace_session_id,
            "trace_scope_id": trace_scope_id,
            "effective_limits": effective_limits,
        }),
    );
    agent_config = agent_config.with_context("execution_role", json!("subagent"));
    if let Some(trace_session_id) = trace_session_id {
        agent_config = agent_config.with_context("trace_session_id", json!(trace_session_id));
    }
    if let Some(trace_scope_id) = trace_scope_id {
        agent_config = agent_config.with_context("trace_scope_id", json!(trace_scope_id));
    }
    agent_config
}

fn build_registry_for_agent(
    parent: &Arc<ToolRegistry>,
    allowed_tools: &[String],
    current_depth: usize,
    max_depth: usize,
) -> ToolRegistry {
    let filtered = FilteredToolset::from_allowlist(parent.clone(), allowed_tools);
    let mut registry = ToolRegistry::new();

    const COLLAB_TOOLS: &[&str] = &[
        "spawn_subagent",
        "wait_subagents",
        "list_subagents",
        "cancel_agent",
        "send_input",
    ];
    let at_depth_limit = max_depth > 0 && current_depth >= max_depth;

    for schema in filtered.list_tools() {
        if at_depth_limit && COLLAB_TOOLS.contains(&schema.name.as_str()) {
            continue;
        }
        if let Some(tool) = parent.get(&schema.name) {
            registry.register_arc(tool);
        }
    }

    registry
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use tokio::sync::mpsc;
    use tokio::time::Duration;

    use crate::error::AiError;
    use crate::llm::{
        ClientKind, CompletionRequest, CompletionResponse, FinishReason, LlmClient,
        LlmClientFactory, LlmProvider, MockLlmClient, MockStep, StreamResult, TokenUsage,
    };

    use super::super::tracker::SubagentTracker;
    use super::*;
    use restflow_telemetry::{ExecutionEvent, ExecutionEventEnvelope, TelemetrySink};
    use restflow_traits::ToolError;
    use restflow_traits::subagent::{
        SpawnPriority, SubagentDefLookup, SubagentDefSummary, SubagentStatus,
    };

    fn sample_effective_limits() -> SubagentEffectiveLimits {
        SubagentEffectiveLimits {
            timeout_secs: 300,
            timeout_source: SubagentLimitSource::ConfigDefault,
            max_iterations: 7,
            max_iterations_source: SubagentLimitSource::ConfigDefault,
        }
    }

    #[test]
    fn build_subagent_agent_config_sets_execution_context() {
        let config = build_subagent_agent_config(
            "Sub-task".to_string(),
            "System prompt".to_string(),
            3,
            &sample_effective_limits(),
            None,
            Some("session-1"),
            Some("scope-1"),
        );

        assert_eq!(
            config.context.get("execution_role"),
            Some(&serde_json::Value::String("subagent".to_string()))
        );
        assert_eq!(config.context["execution_context"]["role"], "subagent");
        assert_eq!(config.context["trace_session_id"], "session-1");
        assert_eq!(config.context["trace_scope_id"], "scope-1");
    }

    #[test]
    fn build_subagent_agent_config_sets_parent_execution_id_when_provided() {
        let config = build_subagent_agent_config(
            "Sub-task".to_string(),
            "System prompt".to_string(),
            3,
            &sample_effective_limits(),
            Some("exec-parent-1"),
            Some("session-2"),
            Some("scope-2"),
        );

        assert_eq!(
            config.context["execution_context"]["parent_execution_id"],
            "exec-parent-1"
        );
        assert_eq!(
            config.context["execution_context"]["trace_session_id"],
            "session-2"
        );
        assert_eq!(
            config.context["execution_context"]["trace_scope_id"],
            "scope-2"
        );
    }

    #[test]
    fn resolve_effective_limits_prefers_request_override_for_max_iterations() {
        let agent_def = SubagentDefSnapshot {
            name: "tester".to_string(),
            system_prompt: "You are a test agent.".to_string(),
            allowed_tools: Vec::new(),
            max_iterations: Some(9),
            default_model: None,
        };
        let config = SubagentConfig {
            max_parallel_agents: 1,
            subagent_timeout_secs: 30,
            max_iterations: 5,
            max_depth: 1,
        };
        let request = SpawnRequest {
            agent_id: Some("tester".to_string()),
            inline: Some(InlineSubagentConfig {
                name: None,
                system_prompt: None,
                allowed_tools: None,
                max_iterations: Some(7),
            }),
            task: "test".to_string(),
            timeout_secs: None,
            max_iterations: Some(11),
            priority: None,
            model: None,
            model_provider: None,
            parent_execution_id: None,
            trace_session_id: None,
            trace_scope_id: None,
        };

        let limits = resolve_effective_limits(&agent_def, &config, &request);
        assert_eq!(limits.max_iterations, 11);
        assert_eq!(
            limits.max_iterations_source,
            SubagentLimitSource::RequestOverride
        );
    }

    struct MockDefLookup {
        defs: HashMap<String, SubagentDefSnapshot>,
    }

    impl MockDefLookup {
        fn with_agent(id: &str) -> Self {
            let mut defs = HashMap::new();
            defs.insert(
                id.to_string(),
                SubagentDefSnapshot {
                    name: id.to_string(),
                    system_prompt: "You are a test agent.".to_string(),
                    allowed_tools: vec![],
                    max_iterations: Some(1),
                    default_model: None,
                },
            );
            Self { defs }
        }

        fn empty() -> Self {
            Self {
                defs: HashMap::new(),
            }
        }
    }

    impl SubagentDefLookup for MockDefLookup {
        fn lookup(&self, id: &str) -> Option<SubagentDefSnapshot> {
            self.defs.get(id).cloned()
        }

        fn list_callable(&self) -> Vec<SubagentDefSummary> {
            vec![]
        }
    }

    struct TestLlmFactory {
        client: Arc<dyn LlmClient>,
        model: String,
        provider: LlmProvider,
    }

    impl TestLlmFactory {
        fn new(client: Arc<dyn LlmClient>, model: &str, provider: LlmProvider) -> Self {
            Self {
                client,
                model: model.to_string(),
                provider,
            }
        }
    }

    impl LlmClientFactory for TestLlmFactory {
        fn create_client(&self, model: &str, _api_key: Option<&str>) -> Result<Arc<dyn LlmClient>> {
            if model == self.model {
                Ok(self.client.clone())
            } else {
                Err(AiError::Llm(format!("unexpected model request: {model}")))
            }
        }

        fn available_models(&self) -> Vec<String> {
            vec![self.model.clone()]
        }

        fn resolve_api_key(&self, _provider: LlmProvider) -> Option<String> {
            None
        }

        fn provider_for_model(&self, model: &str) -> Option<LlmProvider> {
            if model == self.model {
                Some(self.provider)
            } else {
                None
            }
        }

        fn client_kind_for_model(&self, model: &str) -> Option<ClientKind> {
            (model == self.model).then_some(ClientKind::Http)
        }
    }

    #[derive(Default)]
    struct MockOrchestrator {
        plans: Mutex<Vec<ExecutionPlan>>,
    }

    #[async_trait]
    impl AgentOrchestrator for MockOrchestrator {
        async fn run(
            &self,
            plan: ExecutionPlan,
        ) -> std::result::Result<ExecutionOutcome, ToolError> {
            self.plans.lock().expect("plans lock").push(plan);
            Ok(ExecutionOutcome {
                success: true,
                text: Some("orchestrated".to_string()),
                iterations: Some(2),
                duration_ms: Some(7),
                ..ExecutionOutcome::default()
            })
        }
    }

    #[derive(Default)]
    struct RecordingTelemetrySink {
        events: Mutex<Vec<ExecutionEventEnvelope>>,
    }

    #[async_trait]
    impl TelemetrySink for RecordingTelemetrySink {
        async fn emit(&self, event: ExecutionEventEnvelope) {
            self.events.lock().expect("events lock").push(event);
        }
    }

    #[test]
    fn resolve_subagent_definition_from_inline_config() {
        let definitions: Arc<dyn SubagentDefLookup> = Arc::new(MockDefLookup::empty());
        let tool_registry = Arc::new(ToolRegistry::new());
        let request = SpawnRequest {
            agent_id: None,
            inline: Some(InlineSubagentConfig {
                name: Some("tmp".to_string()),
                system_prompt: Some("Inline prompt".to_string()),
                allowed_tools: Some(vec!["http_request".to_string()]),
                max_iterations: Some(7),
            }),
            task: "test".to_string(),
            timeout_secs: None,
            max_iterations: None,
            priority: None,
            model: None,
            model_provider: None,
            parent_execution_id: None,
            trace_session_id: None,
            trace_scope_id: None,
        };

        let snapshot = resolve_subagent_definition(&definitions, &tool_registry, &request)
            .expect("inline definition should resolve");
        assert_eq!(snapshot.name, "tmp");
        assert_eq!(snapshot.system_prompt, "Inline prompt");
        assert_eq!(snapshot.allowed_tools, vec!["http_request".to_string()]);
        assert_eq!(snapshot.max_iterations, Some(7));
    }

    #[tokio::test]
    async fn spawn_subagent_without_agent_id_uses_temporary_definition() {
        let (tx, rx) = mpsc::channel(16);
        let tracker = Arc::new(SubagentTracker::new(tx, rx));
        let definitions: Arc<dyn SubagentDefLookup> = Arc::new(MockDefLookup::empty());
        let llm_client: Arc<dyn LlmClient> = Arc::new(MockLlmClient::from_steps(
            "mock",
            vec![MockStep::text("temporary done")],
        ));
        let tool_registry = Arc::new(ToolRegistry::new());
        let config = SubagentConfig {
            max_parallel_agents: 2,
            subagent_timeout_secs: 10,
            max_iterations: 5,
            max_depth: 1,
        };

        let handle = spawn_subagent(
            tracker.clone(),
            definitions,
            llm_client,
            tool_registry,
            config,
            SpawnRequest {
                agent_id: None,
                inline: None,
                task: "temporary task".to_string(),
                timeout_secs: Some(10),
                max_iterations: None,
                priority: None,
                model: None,
                model_provider: None,
                parent_execution_id: None,
                trace_session_id: None,
                trace_scope_id: None,
            },
            SubagentExecutionBridge::default(),
        )
        .expect("spawn should succeed without explicit agent");

        let result = tracker
            .wait(&handle.id)
            .await
            .expect("temporary subagent result should be available");
        let result = result.result.expect("temporary subagent payload");
        assert!(result.success);
        assert_eq!(handle.agent_name, TEMPORARY_SUBAGENT_NAME);
    }

    #[tokio::test]
    async fn spawn_subagent_can_delegate_to_shared_orchestrator() {
        let (tx, rx) = mpsc::channel(16);
        let tracker = Arc::new(SubagentTracker::new(tx, rx));
        let definitions: Arc<dyn SubagentDefLookup> = Arc::new(MockDefLookup::with_agent("tester"));
        let llm_client: Arc<dyn LlmClient> = Arc::new(MockLlmClient::from_steps("mock", vec![]));
        let tool_registry = Arc::new(ToolRegistry::new());
        let orchestrator = Arc::new(MockOrchestrator::default());
        let config = SubagentConfig {
            max_parallel_agents: 2,
            subagent_timeout_secs: 10,
            max_iterations: 5,
            max_depth: 1,
        };

        let handle = spawn_subagent(
            tracker.clone(),
            definitions,
            llm_client,
            tool_registry,
            config,
            SpawnRequest {
                agent_id: Some("tester".to_string()),
                inline: None,
                task: "delegate task".to_string(),
                timeout_secs: Some(10),
                max_iterations: None,
                priority: None,
                model: None,
                model_provider: None,
                parent_execution_id: Some("parent-1".to_string()),
                trace_session_id: Some("session-1".to_string()),
                trace_scope_id: Some("scope-1".to_string()),
            },
            SubagentExecutionBridge {
                llm_client_factory: None,
                orchestrator: Some(orchestrator.clone()),
                telemetry_sink: None,
            },
        )
        .expect("spawn should succeed");

        let result = tracker
            .wait(&handle.id)
            .await
            .expect("subagent result should be available");
        let result = result.result.expect("subagent result payload");
        assert!(result.success);
        assert_eq!(result.output, "orchestrated");

        let plans = orchestrator.plans.lock().expect("plans lock");
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].mode, Some(ExecutionMode::Subagent));
        assert_eq!(plans[0].input.as_deref(), Some("delegate task"));
        assert_eq!(plans[0].agent_id.as_deref(), Some("tester"));
    }

    #[tokio::test]
    async fn spawn_subagent_orchestrator_infers_provider_from_model_override() {
        let (tx, rx) = mpsc::channel(16);
        let tracker = Arc::new(SubagentTracker::new(tx, rx));
        let definitions: Arc<dyn SubagentDefLookup> = Arc::new(MockDefLookup::with_agent("tester"));
        let llm_client: Arc<dyn LlmClient> = Arc::new(MockLlmClient::from_steps("mock", vec![]));
        let tool_registry = Arc::new(ToolRegistry::new());
        let orchestrator = Arc::new(MockOrchestrator::default());
        let llm_factory: Arc<dyn LlmClientFactory> = Arc::new(TestLlmFactory::new(
            llm_client.clone(),
            "gpt-5.3-codex",
            LlmProvider::OpenAI,
        ));
        let config = SubagentConfig {
            max_parallel_agents: 2,
            subagent_timeout_secs: 10,
            max_iterations: 5,
            max_depth: 1,
        };

        let handle = spawn_subagent(
            tracker.clone(),
            definitions,
            llm_client,
            tool_registry,
            config,
            SpawnRequest {
                agent_id: Some("tester".to_string()),
                inline: None,
                task: "delegate task".to_string(),
                timeout_secs: Some(10),
                max_iterations: None,
                priority: None,
                model: Some("gpt-5.3-codex".to_string()),
                model_provider: None,
                parent_execution_id: None,
                trace_session_id: None,
                trace_scope_id: None,
            },
            SubagentExecutionBridge {
                llm_client_factory: Some(llm_factory),
                orchestrator: Some(orchestrator.clone()),
                telemetry_sink: None,
            },
        )
        .expect("spawn should succeed");

        let result = tracker
            .wait(&handle.id)
            .await
            .expect("subagent result should be available");
        let result = result.result.expect("subagent result payload");
        assert!(result.success);

        let plans = orchestrator.plans.lock().expect("plans lock");
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].model.as_deref(), Some("gpt-5.3-codex"));
        assert_eq!(plans[0].provider.as_deref(), Some("openai"));
    }

    #[tokio::test]
    async fn spawn_subagent_orchestrator_supports_temporary_model_provider_only() {
        let (tx, rx) = mpsc::channel(16);
        let tracker = Arc::new(SubagentTracker::new(tx, rx));
        let definitions: Arc<dyn SubagentDefLookup> = Arc::new(MockDefLookup::with_agent("tester"));
        let llm_client: Arc<dyn LlmClient> = Arc::new(MockLlmClient::from_steps("mock", vec![]));
        let tool_registry = Arc::new(ToolRegistry::new());
        let orchestrator = Arc::new(MockOrchestrator::default());
        let config = SubagentConfig {
            max_parallel_agents: 2,
            subagent_timeout_secs: 10,
            max_iterations: 5,
            max_depth: 1,
        };

        let handle = spawn_subagent(
            tracker.clone(),
            definitions,
            llm_client,
            tool_registry,
            config,
            SpawnRequest {
                agent_id: None,
                inline: None,
                task: "temporary task".to_string(),
                timeout_secs: Some(10),
                max_iterations: None,
                priority: None,
                model: Some("gpt-5.3-codex".to_string()),
                model_provider: Some("openai".to_string()),
                parent_execution_id: None,
                trace_session_id: None,
                trace_scope_id: None,
            },
            SubagentExecutionBridge {
                llm_client_factory: None,
                orchestrator: Some(orchestrator.clone()),
                telemetry_sink: None,
            },
        )
        .expect("spawn should succeed");

        let result = tracker
            .wait(&handle.id)
            .await
            .expect("subagent result should be available");
        let result = result.result.expect("subagent result payload");
        assert!(result.success);

        let plans = orchestrator.plans.lock().expect("plans lock");
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].agent_id, None);
        assert!(plans[0].inline_subagent.is_none());
        assert_eq!(plans[0].model.as_deref(), Some("gpt-5.3-codex"));
        assert_eq!(plans[0].provider.as_deref(), Some("openai"));
    }

    #[tokio::test]
    async fn execute_subagent_once_bypasses_orchestrator_and_runs_directly() {
        let definitions: Arc<dyn SubagentDefLookup> = Arc::new(MockDefLookup::with_agent("tester"));
        let llm_client: Arc<dyn LlmClient> = Arc::new(MockLlmClient::from_steps(
            "mock-direct",
            vec![MockStep::text("direct execution")],
        ));
        let tool_registry = Arc::new(ToolRegistry::new());
        let orchestrator = Arc::new(MockOrchestrator::default());
        let config = SubagentConfig {
            max_parallel_agents: 2,
            subagent_timeout_secs: 10,
            max_iterations: 5,
            max_depth: 1,
        };

        let outcome = execute_subagent_once(
            definitions,
            llm_client,
            tool_registry,
            config,
            SpawnRequest {
                agent_id: Some("tester".to_string()),
                inline: None,
                task: "direct task".to_string(),
                timeout_secs: Some(10),
                max_iterations: None,
                priority: None,
                model: None,
                model_provider: None,
                parent_execution_id: None,
                trace_session_id: None,
                trace_scope_id: None,
            },
            SubagentExecutionBridge {
                llm_client_factory: None,
                orchestrator: Some(orchestrator.clone()),
                telemetry_sink: None,
            },
        )
        .await
        .expect("direct execution should succeed");

        assert!(outcome.success);
        assert_eq!(outcome.text.as_deref(), Some("direct execution"));
        assert_eq!(
            outcome
                .metadata
                .as_ref()
                .and_then(|value| value.get("agent_name"))
                .and_then(|value| value.as_str()),
            Some("tester")
        );

        let plans = orchestrator.plans.lock().expect("plans lock");
        assert!(plans.is_empty());
    }

    #[derive(Clone)]
    struct ErrorFinishLlmClient;

    #[async_trait]
    impl LlmClient for ErrorFinishLlmClient {
        fn provider(&self) -> &str {
            "mock"
        }

        fn model(&self) -> &str {
            "mock-error-finish"
        }

        async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
            Ok(CompletionResponse {
                content: Some(String::new()),
                tool_calls: vec![],
                finish_reason: FinishReason::Error,
                usage: Some(TokenUsage {
                    prompt_tokens: 1,
                    completion_tokens: 0,
                    total_tokens: 1,
                    cost_usd: Some(0.0),
                }),
            })
        }

        fn complete_stream(&self, _request: CompletionRequest) -> StreamResult {
            panic!("complete_stream is not used in these tests");
        }

        fn supports_streaming(&self) -> bool {
            false
        }
    }

    #[test]
    fn spawn_request_serialization_round_trips() {
        let request = SpawnRequest {
            agent_id: Some("researcher".to_string()),
            inline: None,
            task: "Research topic X".to_string(),
            timeout_secs: Some(300),
            max_iterations: None,
            priority: Some(SpawnPriority::High),
            model: None,
            model_provider: None,
            parent_execution_id: None,
            trace_session_id: Some("session-1".to_string()),
            trace_scope_id: Some("scope-1".to_string()),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("researcher"));

        let parsed: SpawnRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.agent_id.as_deref(), Some("researcher"));
        assert_eq!(parsed.trace_session_id.as_deref(), Some("session-1"));
        assert_eq!(parsed.trace_scope_id.as_deref(), Some("scope-1"));
    }

    #[test]
    fn spawn_handle_serialization_round_trips() {
        let handle = SpawnHandle {
            id: "task-123".to_string(),
            agent_name: "Researcher".to_string(),
            effective_limits: SubagentEffectiveLimits {
                timeout_secs: 300,
                timeout_source: SubagentLimitSource::ConfigDefault,
                max_iterations: 100,
                max_iterations_source: SubagentLimitSource::ConfigDefault,
            },
        };

        let json = serde_json::to_string(&handle).unwrap();
        assert!(json.contains("task-123"));
    }

    #[tokio::test]
    async fn spawn_subagent_uses_explicit_trace_session_and_scope() {
        let (tx, rx) = mpsc::channel(16);
        let tracker = Arc::new(SubagentTracker::new(tx, rx));
        let sink = Arc::new(RecordingTelemetrySink::default());
        tracker.set_telemetry_sink(sink.clone());
        let definitions: Arc<dyn SubagentDefLookup> = Arc::new(MockDefLookup::with_agent("tester"));
        let llm_client: Arc<dyn LlmClient> = Arc::new(MockLlmClient::from_steps(
            "mock",
            vec![MockStep::text("done")],
        ));
        let tool_registry = Arc::new(ToolRegistry::new());
        let config = SubagentConfig {
            max_parallel_agents: 2,
            subagent_timeout_secs: 10,
            max_iterations: 5,
            max_depth: 1,
        };

        let handle = spawn_subagent(
            tracker.clone(),
            definitions,
            llm_client,
            tool_registry,
            config,
            SpawnRequest {
                agent_id: Some("tester".to_string()),
                inline: None,
                task: "trace me".to_string(),
                timeout_secs: Some(10),
                max_iterations: None,
                priority: None,
                model: None,
                model_provider: None,
                parent_execution_id: Some("exec-parent-1".to_string()),
                trace_session_id: Some("session-main-1".to_string()),
                trace_scope_id: Some("scope-main-1".to_string()),
            },
            SubagentExecutionBridge::default(),
        )
        .expect("spawn should succeed");

        let result = tracker
            .wait(&handle.id)
            .await
            .expect("subagent result should be available");
        let result = result.result.expect("subagent result payload");
        assert!(result.success);

        let events = sink.events.lock().expect("events lock").clone();
        assert_eq!(events.len(), 3);
        assert!(matches!(events[0].event, ExecutionEvent::RunStarted));
        assert!(
            events
                .iter()
                .any(|event| matches!(event.event, ExecutionEvent::LlmCall(_)))
        );
        assert!(matches!(
            events[2].event,
            ExecutionEvent::RunCompleted { .. }
        ));
        for event in events {
            assert_eq!(event.trace.run_id, handle.id);
            assert_eq!(event.trace.parent_run_id.as_deref(), Some("exec-parent-1"));
            assert_eq!(event.trace.session_id, "session-main-1");
            assert_eq!(event.trace.scope_id, "scope-main-1");
        }
    }

    #[tokio::test]
    async fn spawn_over_max_parallel_does_not_execute() {
        let (tx, rx) = mpsc::channel(16);
        let tracker = Arc::new(SubagentTracker::new(tx, rx));
        let definitions: Arc<dyn SubagentDefLookup> = Arc::new(MockDefLookup::with_agent("tester"));
        let llm_client: Arc<dyn LlmClient> = Arc::new(MockLlmClient::from_steps(
            "mock",
            vec![
                MockStep::text("result-1").with_delay(2000),
                MockStep::text("result-2"),
            ],
        ));
        let tool_registry = Arc::new(ToolRegistry::new());
        let config = SubagentConfig {
            max_parallel_agents: 1,
            subagent_timeout_secs: 10,
            max_iterations: 5,
            max_depth: 1,
        };

        let result1 = spawn_subagent(
            tracker.clone(),
            definitions.clone(),
            llm_client.clone(),
            tool_registry.clone(),
            config.clone(),
            SpawnRequest {
                agent_id: Some("tester".to_string()),
                inline: None,
                task: "first task".to_string(),
                timeout_secs: Some(10),
                max_iterations: None,
                priority: None,
                model: None,
                model_provider: None,
                parent_execution_id: None,
                trace_session_id: None,
                trace_scope_id: None,
            },
            SubagentExecutionBridge::default(),
        );
        assert!(result1.is_ok());

        let result2 = spawn_subagent(
            tracker.clone(),
            definitions.clone(),
            llm_client.clone(),
            tool_registry.clone(),
            config.clone(),
            SpawnRequest {
                agent_id: Some("tester".to_string()),
                inline: None,
                task: "second task (should not execute)".to_string(),
                timeout_secs: Some(10),
                max_iterations: None,
                priority: None,
                model: None,
                model_provider: None,
                parent_execution_id: None,
                trace_session_id: None,
                trace_scope_id: None,
            },
            SubagentExecutionBridge::default(),
        );
        assert!(result2.is_err());

        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(tracker.all().len(), 1);
    }

    #[tokio::test]
    async fn spawn_subagent_propagates_agent_failure_success_flag() {
        let (tx, rx) = mpsc::channel(16);
        let tracker = Arc::new(SubagentTracker::new(tx, rx));
        let definitions: Arc<dyn SubagentDefLookup> = Arc::new(MockDefLookup::with_agent("tester"));
        let llm_client: Arc<dyn LlmClient> = Arc::new(ErrorFinishLlmClient);
        let tool_registry = Arc::new(ToolRegistry::new());
        let config = SubagentConfig {
            max_parallel_agents: 2,
            subagent_timeout_secs: 10,
            max_iterations: 5,
            max_depth: 1,
        };

        let handle = spawn_subagent(
            tracker.clone(),
            definitions,
            llm_client,
            tool_registry,
            config,
            SpawnRequest {
                agent_id: Some("tester".to_string()),
                inline: None,
                task: "force failure status".to_string(),
                timeout_secs: Some(10),
                max_iterations: None,
                priority: None,
                model: None,
                model_provider: None,
                parent_execution_id: None,
                trace_session_id: None,
                trace_scope_id: None,
            },
            SubagentExecutionBridge::default(),
        )
        .expect("spawn should succeed");

        let result = tracker
            .wait(&handle.id)
            .await
            .expect("subagent result should be available");
        let result = result.result.expect("subagent result payload");

        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("LLM returned an error"));

        let state = tracker.get(&handle.id).expect("state should exist");
        assert_eq!(state.status, SubagentStatus::Failed);
    }

    #[tokio::test]
    async fn spawn_subagent_maps_max_iterations_to_failed_result() {
        let (tx, rx) = mpsc::channel(16);
        let tracker = Arc::new(SubagentTracker::new(tx, rx));
        let definitions: Arc<dyn SubagentDefLookup> = Arc::new(MockDefLookup::with_agent("tester"));
        let llm_client: Arc<dyn LlmClient> = Arc::new(MockLlmClient::from_steps(
            "mock",
            vec![MockStep::tool_call(
                "call-1",
                "missing_tool",
                serde_json::json!({"input":"x"}),
            )],
        ));
        let tool_registry = Arc::new(ToolRegistry::new());
        let config = SubagentConfig {
            max_parallel_agents: 2,
            subagent_timeout_secs: 10,
            max_iterations: 5,
            max_depth: 1,
        };

        let handle = spawn_subagent(
            tracker.clone(),
            definitions,
            llm_client,
            tool_registry,
            config,
            SpawnRequest {
                agent_id: Some("tester".to_string()),
                inline: None,
                task: "hit max iterations".to_string(),
                timeout_secs: Some(10),
                max_iterations: None,
                priority: None,
                model: None,
                model_provider: None,
                parent_execution_id: None,
                trace_session_id: None,
                trace_scope_id: None,
            },
            SubagentExecutionBridge::default(),
        )
        .expect("spawn should succeed");

        let result = tracker
            .wait(&handle.id)
            .await
            .expect("subagent result should be available");
        let result = result.result.expect("subagent result payload");

        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("Max iterations reached"));

        let state = tracker.get(&handle.id).expect("state should exist");
        assert_eq!(state.status, SubagentStatus::Failed);
    }

    #[test]
    fn build_registry_excludes_collab_tools_at_depth_limit() {
        let mut parent = ToolRegistry::new();

        struct DummyTool(&'static str);
        #[async_trait::async_trait]
        impl restflow_traits::Tool for DummyTool {
            fn name(&self) -> &str {
                self.0
            }

            fn description(&self) -> &str {
                ""
            }

            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({})
            }

            async fn execute(
                &self,
                _input: serde_json::Value,
            ) -> std::result::Result<restflow_traits::ToolOutput, restflow_traits::ToolError>
            {
                unimplemented!()
            }
        }

        parent.register(DummyTool("http"));
        parent.register(DummyTool("bash"));
        parent.register(DummyTool("spawn_subagent"));
        parent.register(DummyTool("wait_subagents"));
        parent.register(DummyTool("list_subagents"));
        parent.register(DummyTool("cancel_agent"));
        parent.register(DummyTool("send_input"));

        let parent = Arc::new(parent);
        let all_tools: Vec<String> = vec![
            "http",
            "bash",
            "spawn_subagent",
            "wait_subagents",
            "list_subagents",
            "cancel_agent",
            "send_input",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let registry = build_registry_for_agent(&parent, &all_tools, 1, 1);
        let names: Vec<String> = registry
            .list_tools()
            .into_iter()
            .map(|schema| schema.name)
            .collect();
        assert!(names.contains(&"http".to_string()));
        assert!(names.contains(&"bash".to_string()));
        assert!(!names.contains(&"spawn_subagent".to_string()));
        assert!(!names.contains(&"wait_subagents".to_string()));
        assert!(!names.contains(&"list_subagents".to_string()));
        assert!(!names.contains(&"cancel_agent".to_string()));
        assert!(!names.contains(&"send_input".to_string()));

        let registry = build_registry_for_agent(&parent, &all_tools, 0, 2);
        let names: Vec<String> = registry
            .list_tools()
            .into_iter()
            .map(|schema| schema.name)
            .collect();
        assert!(names.contains(&"spawn_subagent".to_string()));
        assert!(names.contains(&"wait_subagents".to_string()));
    }

    #[test]
    fn subagent_config_disables_workspace_instruction_injection() {
        let config = build_subagent_agent_config(
            "task".to_string(),
            "You are subagent".to_string(),
            7,
            &sample_effective_limits(),
            None,
            Some("session-3"),
            Some("scope-3"),
        );
        assert_eq!(config.max_iterations, 7);
        assert_eq!(config.system_prompt.as_deref(), Some("You are subagent"));
        assert!(!config.prompt_flags.include_workspace_context);
        assert!(config.yolo_mode);
        assert_eq!(config.context["trace_session_id"], "session-3");
        assert_eq!(config.context["trace_scope_id"], "scope-3");
    }

    #[test]
    fn map_subagent_error_uses_default_message_on_missing_failure_error() {
        let mapped = map_subagent_error(false, None);
        assert_eq!(mapped.as_deref(), Some("Sub-agent execution failed"));
    }

    #[test]
    fn map_subagent_error_clears_error_on_success() {
        let mapped = map_subagent_error(true, Some("ignored".to_string()));
        assert!(mapped.is_none());
    }
}
