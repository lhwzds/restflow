use serde_json::{Value, json};
use tokio::time::{Duration, timeout};

use crate::impls::operation_assessment::{enforce_confirmation_or_defer, preview_output};
use crate::impls::spawn_subagent_batch::{SpawnSubagentBatchOperation, SpawnSubagentBatchTool};
use crate::{Result, Tool, ToolError, ToolOutput};
use restflow_contracts::request::{
    InlineSubagentConfig as ContractInlineSubagentConfig,
    SubagentSpawnRequest as ContractSubagentSpawnRequest,
};
use restflow_traits::{SubagentCompletion, SubagentStatus};

use super::{SpawnSubagentParams, SpawnSubagentTool};

fn completion_output(
    task_id: &str,
    agent_name: &str,
    completion: SubagentCompletion,
    effective_limits: &restflow_traits::SubagentEffectiveLimits,
) -> Value {
    let status = match completion.status {
        SubagentStatus::Completed => "completed",
        SubagentStatus::Failed => "failed",
        SubagentStatus::Interrupted => "interrupted",
        SubagentStatus::TimedOut => "timed_out",
        SubagentStatus::Pending => "pending",
        SubagentStatus::Running => "running",
    };

    let mut output = json!({
        "task_id": task_id,
        "agent": agent_name,
        "status": status,
        "effective_limits": effective_limits,
    });

    if let Some(result) = completion.result {
        output["duration_ms"] = json!(result.duration_ms);
        if result.success {
            output["output"] = json!(result.output);
        } else {
            output["error"] = json!(result.error.unwrap_or_else(|| "Unknown error".to_string()));
            if !result.output.is_empty() {
                output["output"] = json!(result.output);
            }
        }
    }

    output
}

fn build_inline_config(params: &SpawnSubagentParams) -> Option<ContractInlineSubagentConfig> {
    let config = ContractInlineSubagentConfig {
        name: params.inline_name.clone(),
        system_prompt: params.inline_system_prompt.clone(),
        allowed_tools: params.inline_allowed_tools.clone(),
        max_iterations: params.inline_max_iterations,
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

fn uses_batch_mode(params: &SpawnSubagentParams) -> bool {
    params.workers.is_some()
        || params.team.is_some()
        || params.save_as_team.is_some()
        || params.tasks.is_some()
}

fn routes_to_batch_tool(params: &SpawnSubagentParams) -> bool {
    params.operation != SpawnSubagentBatchOperation::Spawn || uses_batch_mode(params)
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn build_contract_request(
    params: &SpawnSubagentParams,
    task: String,
) -> ContractSubagentSpawnRequest {
    ContractSubagentSpawnRequest {
        agent_id: params.agent.clone(),
        inline: build_inline_config(params),
        task,
        timeout_secs: params.timeout_secs,
        max_iterations: None,
        priority: None,
        model: params.model.clone(),
        model_provider: params.provider.clone(),
        parent_execution_id: params.parent_execution_id.clone(),
        trace_session_id: params.trace_session_id.clone(),
        trace_scope_id: params.trace_scope_id.clone(),
    }
}

fn resolve_batch_team(params: &SpawnSubagentParams) -> Result<Option<String>> {
    let team = normalize_optional_text(params.team.as_deref());
    let save_as_team = normalize_optional_text(params.save_as_team.as_deref());

    if params.operation == SpawnSubagentBatchOperation::SaveTeam {
        return match (team, save_as_team) {
            (Some(team_name), Some(alias)) if team_name != alias => Err(ToolError::Tool(
                "When operation is 'save_team', 'team' and 'save_as_team' must match if both are provided.".to_string(),
            )),
            (Some(team_name), _) => Ok(Some(team_name)),
            (None, Some(alias)) => Ok(Some(alias)),
            (None, None) => Ok(None),
        };
    }

    if params.operation != SpawnSubagentBatchOperation::Spawn && save_as_team.is_some() {
        return Err(ToolError::Tool(
            "'save_as_team' is only supported for 'spawn', or as an alias of 'team' when operation is 'save_team'.".to_string(),
        ));
    }

    Ok(team)
}

pub(super) async fn execute(
    tool: &SpawnSubagentTool,
    params: SpawnSubagentParams,
) -> Result<ToolOutput> {
    if routes_to_batch_tool(&params) {
        if params.agent.is_some()
            || params.model.is_some()
            || params.provider.is_some()
            || params.inline_name.is_some()
            || params.inline_system_prompt.is_some()
            || params.inline_allowed_tools.is_some()
            || params.inline_max_iterations.is_some()
        {
            return Err(ToolError::Tool(
                "Batch mode uses 'workers'/'team'; do not combine with single-spawn fields like 'agent', top-level model/provider, or top-level inline settings.".to_string(),
            ));
        }

        let mut batch_tool = SpawnSubagentBatchTool::new(tool.manager.clone());
        if let Some(kv_store) = tool.kv_store.clone() {
            batch_tool = batch_tool.with_kv_store(kv_store);
        }
        if let Some(assessor) = tool.assessor.clone() {
            batch_tool = batch_tool.with_assessor(assessor);
        }

        let operation = params.operation.clone();
        let task = normalize_optional_text(params.task.as_deref());
        let tasks = params.tasks.clone();
        let team = resolve_batch_team(&params)?;
        let save_as_team = if operation == SpawnSubagentBatchOperation::Spawn {
            normalize_optional_text(params.save_as_team.as_deref())
        } else {
            None
        };

        return batch_tool
            .execute(json!({
                "operation": operation,
                "team": team,
                "specs": params.workers,
                "task": task,
                "tasks": tasks,
                "wait": params.wait,
                "timeout_secs": params.timeout_secs,
                "save_as_team": save_as_team,
                "parent_execution_id": params.parent_execution_id,
                "trace_session_id": params.trace_session_id,
                "trace_scope_id": params.trace_scope_id,
                "preview": params.preview,
                "confirmation_token": params.confirmation_token
            }))
            .await;
    }

    let request = build_contract_request(
        &params,
        normalize_optional_text(params.task.as_deref()).unwrap_or_default(),
    );

    if let Some(assessor) = &tool.assessor {
        let assessment = assessor
            .assess_subagent_spawn("spawn_subagent", request.clone(), false)
            .await?;
        if params.preview {
            return Ok(preview_output(assessment));
        }
        if let Some(output) =
            enforce_confirmation_or_defer(&assessment, params.confirmation_token.as_deref())?
        {
            return Ok(output);
        }
    } else if params.preview {
        return Err(ToolError::Tool(
            "Sub-agent capability preview is unavailable in this runtime.".to_string(),
        ));
    }

    let handle = tool.manager.spawn(request)?;

    if params.wait {
        let wait_timeout = params
            .timeout_secs
            .unwrap_or(tool.manager.config().subagent_timeout_secs);

        let result = if wait_timeout == 0 {
            match tool.manager.wait(&handle.id).await {
                Some(result) => result,
                None => return Ok(ToolOutput::error("Sub-agent not found")),
            }
        } else {
            match timeout(
                Duration::from_secs(wait_timeout),
                tool.manager.wait(&handle.id),
            )
            .await
            {
                Ok(Some(result)) => result,
                Ok(None) => return Ok(ToolOutput::error("Sub-agent not found")),
                Err(_) => {
                    return Ok(ToolOutput::success(json!({
                        "task_id": handle.id,
                        "agent": handle.agent_name,
                        "status": "timeout",
                        "message": "Timeout waiting for sub-agent",
                        "effective_limits": handle.effective_limits,
                    })));
                }
            }
        };

        Ok(ToolOutput::success(completion_output(
            &handle.id,
            &handle.agent_name,
            result,
            &handle.effective_limits,
        )))
    } else {
        Ok(ToolOutput::success(json!({
            "task_id": handle.id,
            "agent": handle.agent_name,
            "status": "spawned",
            "effective_limits": handle.effective_limits,
            "message": format!(
                "Agent '{}' is now working on the task concurrently. Use wait_subagents to check completion.",
                handle.agent_name
            )
        })))
    }
}
