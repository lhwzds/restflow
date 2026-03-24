use crate::impls::operation_assessment::{enforce_confirmation, preview_output};
use serde_json::{Value, json};
use tokio::time::{Duration, timeout};

use crate::{Result, ToolError, ToolOutput};
use restflow_traits::boundary::subagent::spawn_request_from_contract;
use restflow_traits::{SubagentCompletion, SubagentResult, SubagentStatus};

use super::SpawnSubagentBatchTool;
use super::resolve::spawn_request_from_spec;
use super::team::{load_team_specs, save_team_specs};
use super::types::{
    BatchSubagentSpec, PreparedSpawnRequest, SpawnFailure, SpawnSubagentBatchParams, SpawnedTask,
};
use super::validate::{resolve_batch_tasks, total_instances, validate_structural_specs};

pub(super) fn specs_for_spawn(
    tool: &SpawnSubagentBatchTool,
    params: &SpawnSubagentBatchParams,
) -> Result<Vec<BatchSubagentSpec>> {
    if params.team.is_some() && params.specs.is_some() {
        return Err(ToolError::Tool(
            "Use either 'team' or 'specs' for spawn, not both.".to_string(),
        ));
    }

    let specs = if let Some(team_name) = params.team.as_deref() {
        load_team_specs(tool, team_name)?
    } else {
        params.specs.clone().ok_or_else(|| {
            ToolError::Tool("Spawn requires either 'team' or 'specs'.".to_string())
        })?
    };

    if specs.is_empty() {
        return Err(ToolError::Tool("Specs must not be empty.".to_string()));
    }

    validate_structural_specs(tool, &specs)?;

    for spec in &specs {
        if spec.task.is_some() && spec.tasks.is_some() {
            return Err(ToolError::Tool(
                "Each spec can use either 'task' or 'tasks', not both.".to_string(),
            ));
        }
    }

    Ok(specs)
}

async fn wait_result(
    tool: &SpawnSubagentBatchTool,
    task_id: &str,
    timeout_secs: u64,
) -> Option<SubagentCompletion> {
    if timeout_secs == 0 {
        return tool.manager.wait(task_id).await;
    }
    timeout(
        Duration::from_secs(timeout_secs),
        tool.manager.wait(task_id),
    )
    .await
    .unwrap_or_default()
}

fn task_entries(spawned: &[SpawnedTask]) -> Vec<Value> {
    spawned
        .iter()
        .map(|task| {
            json!({
                "task_id": task.task_id,
                "agent": task.agent_name,
                "spec_index": task.spec_index,
                "instance_index": task.instance_index,
                "effective_limits": task.effective_limits,
            })
        })
        .collect()
}

async fn wait_for_spawned_tasks(
    tool: &SpawnSubagentBatchTool,
    spawned: &[SpawnedTask],
    wait_timeout: u64,
) -> Vec<Value> {
    let mut results = Vec::with_capacity(spawned.len());
    for task in spawned {
        let wait_result = wait_result(tool, &task.task_id, wait_timeout).await;
        match wait_result {
            Some(completion) if completion.status == SubagentStatus::Completed => {
                let result = completion.result.unwrap_or(SubagentResult {
                    success: true,
                    output: String::new(),
                    summary: None,
                    duration_ms: 0,
                    tokens_used: None,
                    cost_usd: None,
                    error: None,
                });
                results.push(json!({
                    "task_id": task.task_id,
                    "agent": task.agent_name,
                    "spec_index": task.spec_index,
                    "instance_index": task.instance_index,
                    "status": "completed",
                    "output": result.output,
                    "duration_ms": result.duration_ms,
                    "effective_limits": task.effective_limits,
                }))
            }
            Some(completion) => {
                let status = match completion.status {
                    SubagentStatus::Interrupted => "interrupted",
                    SubagentStatus::TimedOut => "timed_out",
                    SubagentStatus::Failed => "failed",
                    SubagentStatus::Pending => "pending",
                    SubagentStatus::Running => "running",
                    SubagentStatus::Completed => "completed",
                };
                let result = completion.result;
                results.push(json!({
                    "task_id": task.task_id,
                    "agent": task.agent_name,
                    "spec_index": task.spec_index,
                    "instance_index": task.instance_index,
                    "status": status,
                    "error": result.as_ref().and_then(|value| value.error.clone()).unwrap_or_else(|| "Unknown error".to_string()),
                    "duration_ms": result.as_ref().map(|value| value.duration_ms).unwrap_or_default(),
                    "effective_limits": task.effective_limits,
                }));
            }
            None => results.push(json!({
                "task_id": task.task_id,
                "agent": task.agent_name,
                "spec_index": task.spec_index,
                "instance_index": task.instance_index,
                "status": "timeout",
                "effective_limits": task.effective_limits,
            })),
        }
    }
    results
}

pub(super) async fn spawn_batch(
    tool: &SpawnSubagentBatchTool,
    params: SpawnSubagentBatchParams,
) -> Result<ToolOutput> {
    let specs = specs_for_spawn(tool, &params)?;
    let total_requested = total_instances(&specs)?;
    let max_parallel = tool.manager.config().max_parallel_agents;
    let running_now = tool.manager.running_count();
    let available_slots = max_parallel.saturating_sub(running_now);
    if total_requested > available_slots {
        return Err(ToolError::Tool(format!(
            "Requested {} sub-agents, but only {} slots are available (running: {}, max_parallel: {}).",
            total_requested, available_slots, running_now, max_parallel
        )));
    }

    let resolved_tasks =
        resolve_batch_tasks(&specs, params.task.as_deref(), params.tasks.as_deref())?;

    let mut prepared = Vec::with_capacity(total_requested);
    for (spec_index, (spec, instance_tasks)) in
        specs.iter().zip(resolved_tasks.into_iter()).enumerate()
    {
        for (instance_index, task) in instance_tasks.into_iter().enumerate() {
            if instance_index > u32::MAX as usize {
                return Err(ToolError::Tool(format!(
                    "Spec index {} has too many instances to index as u32.",
                    spec_index
                )));
            }
            let instance_index = instance_index as u32;

            let request = spawn_request_from_contract(
                &tool.available_agents(),
                spawn_request_from_spec(spec, task, &params),
            )?;
            prepared.push(PreparedSpawnRequest {
                spec_index,
                instance_index,
                request,
            });
        }
    }

    if let Some(assessor) = &tool.assessor {
        let assessment = assessor
            .assess_subagent_batch(
                "spawn_subagent_batch",
                prepared.iter().map(|item| item.request.clone()).collect(),
                false,
            )
            .await?;
        if params.preview {
            return Ok(preview_output(assessment));
        }
        enforce_confirmation(&assessment, params.confirmation_token.as_deref())?;
    } else if params.preview {
        return Err(ToolError::Tool(
            "Sub-agent capability preview is unavailable in this runtime.".to_string(),
        ));
    }

    if let Some(team_name) = params.save_as_team.as_deref() {
        save_team_specs(tool, team_name, &specs)?;
    }

    let mut spawned = Vec::with_capacity(prepared.len());
    let mut spawn_failure = None;
    for item in prepared {
        match tool.manager.spawn(item.request) {
            Ok(handle) => spawned.push(SpawnedTask {
                task_id: handle.id,
                agent_name: handle.agent_name,
                spec_index: item.spec_index,
                instance_index: item.instance_index,
                effective_limits: handle.effective_limits,
            }),
            Err(error) => {
                spawn_failure = Some(SpawnFailure {
                    spec_index: item.spec_index,
                    instance_index: item.instance_index,
                    error,
                });
                break;
            }
        }
    }

    if let Some(failure) = spawn_failure {
        if spawned.is_empty() {
            return Err(failure.error);
        }

        let wait_timeout = params
            .timeout_secs
            .unwrap_or(tool.manager.config().subagent_timeout_secs);
        let tasks = task_entries(&spawned);
        let task_ids = spawned
            .iter()
            .map(|task| task.task_id.clone())
            .collect::<Vec<_>>();
        let mut payload = json!({
            "operation": "spawn",
            "status": "partial_failure",
            "spawned_count": spawned.len(),
            "running_before": running_now,
            "max_parallel": max_parallel,
            "team": params.team,
            "saved_team": params.save_as_team,
            "task_ids": task_ids,
            "tasks": tasks,
            "failed_spec_index": failure.spec_index,
            "failed_instance_index": failure.instance_index,
            "error": failure.error.to_string(),
        });

        if params.wait {
            payload["results"] =
                Value::Array(wait_for_spawned_tasks(tool, &spawned, wait_timeout).await);
        }

        return Ok(ToolOutput::success(payload));
    }

    if !params.wait {
        let tasks = task_entries(&spawned);
        return Ok(ToolOutput::success(json!({
            "operation": "spawn",
            "status": "spawned",
            "spawned_count": spawned.len(),
            "running_before": running_now,
            "max_parallel": max_parallel,
            "team": params.team,
            "saved_team": params.save_as_team,
            "task_ids": spawned.iter().map(|task| task.task_id.clone()).collect::<Vec<_>>(),
            "tasks": tasks
        })));
    }

    let wait_timeout = params
        .timeout_secs
        .unwrap_or(tool.manager.config().subagent_timeout_secs);
    let results = wait_for_spawned_tasks(tool, &spawned, wait_timeout).await;

    Ok(ToolOutput::success(json!({
        "operation": "spawn",
        "status": "completed",
        "spawned_count": spawned.len(),
        "team": params.team,
        "saved_team": params.save_as_team,
        "results": results
    })))
}
