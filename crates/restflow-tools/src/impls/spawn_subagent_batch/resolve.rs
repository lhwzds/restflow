use restflow_contracts::request::{
    InlineAgentRunConfig as ContractInlineAgentRunConfig,
    RunSpawnRequest as ContractRunSpawnRequest,
};

use super::types::{BatchSubagentSpec, SpawnSubagentBatchParams};

fn build_inline_config(spec: &BatchSubagentSpec) -> Option<ContractInlineAgentRunConfig> {
    let config = ContractInlineAgentRunConfig {
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

pub(super) fn preview_request_from_spec(spec: &BatchSubagentSpec) -> ContractRunSpawnRequest {
    ContractRunSpawnRequest {
        agent_id: spec.agent.clone(),
        inline: build_inline_config(spec),
        task: "Structural team preview".to_string(),
        timeout_secs: spec.timeout_secs,
        max_iterations: None,
        priority: None,
        model: spec.model.clone(),
        model_provider: spec.provider.clone(),
        parent_run_id: None,
        trace_session_id: None,
        trace_scope_id: None,
        team_run_id: None,
        team_member_id: None,
        leader_member_id: None,
        team_role: None,
    }
}

pub(super) fn spawn_request_from_spec(
    spec: &BatchSubagentSpec,
    task: String,
    params: &SpawnSubagentBatchParams,
) -> ContractRunSpawnRequest {
    ContractRunSpawnRequest {
        agent_id: spec.agent.clone(),
        inline: build_inline_config(spec),
        task,
        timeout_secs: spec.timeout_secs.or(params.timeout_secs),
        max_iterations: None,
        priority: None,
        model: spec.model.clone(),
        model_provider: spec.provider.clone(),
        parent_run_id: params.parent_run_id.clone(),
        trace_session_id: params.trace_session_id.clone(),
        trace_scope_id: params.trace_scope_id.clone(),
        team_run_id: None,
        team_member_id: None,
        leader_member_id: None,
        team_role: None,
    }
}
