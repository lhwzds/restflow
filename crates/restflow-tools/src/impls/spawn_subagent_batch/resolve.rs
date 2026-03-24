use restflow_contracts::request::{
    InlineSubagentConfig as ContractInlineSubagentConfig,
    SubagentSpawnRequest as ContractSubagentSpawnRequest,
};

use super::types::{BatchSubagentSpec, SpawnSubagentBatchParams};

fn build_inline_config(spec: &BatchSubagentSpec) -> Option<ContractInlineSubagentConfig> {
    let config = ContractInlineSubagentConfig {
        name: spec.inline_name.clone(),
        system_prompt: spec.inline_system_prompt.clone(),
        allowed_tools: spec.inline_allowed_tools.clone(),
        max_iterations: spec.inline_max_iterations,
    };
    if config.name.is_none()
        && config.system_prompt.is_none()
        && config.allowed_tools.is_none()
        && config.max_iterations.is_none()
    {
        None
    } else {
        Some(config)
    }
}

pub(super) fn preview_request_from_spec(spec: &BatchSubagentSpec) -> ContractSubagentSpawnRequest {
    ContractSubagentSpawnRequest {
        agent_id: spec.agent.clone(),
        inline: build_inline_config(spec),
        task: "Structural team preview".to_string(),
        timeout_secs: spec.timeout_secs,
        max_iterations: None,
        priority: None,
        model: spec.model.clone(),
        model_provider: spec.provider.clone(),
        parent_execution_id: None,
        trace_session_id: None,
        trace_scope_id: None,
    }
}

pub(super) fn spawn_request_from_spec(
    spec: &BatchSubagentSpec,
    task: String,
    params: &SpawnSubagentBatchParams,
) -> ContractSubagentSpawnRequest {
    ContractSubagentSpawnRequest {
        agent_id: spec.agent.clone(),
        inline: build_inline_config(spec),
        task,
        timeout_secs: spec.timeout_secs.or(params.timeout_secs),
        max_iterations: None,
        priority: None,
        model: spec.model.clone(),
        model_provider: spec.provider.clone(),
        parent_execution_id: params.parent_execution_id.clone(),
        trace_session_id: params.trace_session_id.clone(),
        trace_scope_id: params.trace_scope_id.clone(),
    }
}
