use crate::impls::operation_assessment::{enforce_confirmation, preview_output};
use restflow_contracts::request::{
    DurabilityMode as ContractDurabilityMode, MemoryConfig as ContractMemoryConfig,
    ResourceLimits as ContractResourceLimits, TaskSchedule as ContractTaskSchedule,
};
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{Result, ToolError, ToolOutput};
use restflow_traits::store::{BackgroundAgentControlRequest, BackgroundAgentCreateRequest};
use restflow_traits::{OperationAssessmentIntent, RuntimeTaskPayload};

use super::BackgroundAgentTool;
use super::team::{load_team_workers, save_team_workers};
use super::types::BackgroundBatchWorkerSpec;

fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

fn expand_worker_specs(
    workers: &[BackgroundBatchWorkerSpec],
    fallback_input: Option<&str>,
    fallback_inputs: Option<&[String]>,
) -> Result<Vec<(usize, String, BackgroundBatchWorkerSpec)>> {
    RuntimeTaskPayload {
        task: fallback_input.map(str::to_string),
        tasks: fallback_inputs.map(|items| items.to_vec()),
    }
    .validate("input", "inputs")
    .map_err(ToolError::Tool)?;

    if let Some(inputs) = fallback_inputs {
        if inputs.is_empty() {
            return Err(ToolError::Tool(
                "Top-level 'inputs' must not be empty.".to_string(),
            ));
        }

        for (spec_index, spec) in workers.iter().enumerate() {
            if spec.input.is_some() || spec.inputs.is_some() {
                return Err(ToolError::Tool(format!(
                    "Top-level 'inputs' cannot be combined with per-worker 'input' or 'inputs' (worker index {}).",
                    spec_index
                )));
            }
            if spec.count == 0 {
                return Err(ToolError::Tool(format!(
                    "Worker index {} count must be >= 1.",
                    spec_index
                )));
            }
        }

        let expected = workers
            .iter()
            .map(|worker| worker.count as usize)
            .sum::<usize>();
        if inputs.len() != expected {
            return Err(ToolError::Tool(format!(
                "Top-level 'inputs' length {} does not match total requested instances {}.",
                inputs.len(),
                expected
            )));
        }

        let mut expanded = Vec::with_capacity(expected);
        let mut offset = 0usize;
        for (spec_index, spec) in workers.iter().enumerate() {
            for instance_index in 0..spec.count as usize {
                let input = inputs[offset + instance_index].trim();
                if input.is_empty() {
                    return Err(ToolError::Tool(format!(
                        "Top-level 'inputs' has empty input at index {}.",
                        offset + instance_index
                    )));
                }
                expanded.push((spec_index, input.to_string(), spec.clone()));
            }
            offset += spec.count as usize;
        }
        return Ok(expanded);
    }

    let mut expanded = Vec::new();
    for (spec_index, spec) in workers.iter().enumerate() {
        if spec.input.is_some() && spec.inputs.is_some() {
            return Err(ToolError::Tool(format!(
                "Worker index {} cannot set both 'input' and 'inputs'.",
                spec_index
            )));
        }
        if let Some(inputs) = &spec.inputs {
            if inputs.is_empty() {
                return Err(ToolError::Tool(format!(
                    "Worker index {} has empty 'inputs'.",
                    spec_index
                )));
            }
            if spec.count != 1 && spec.count as usize != inputs.len() {
                return Err(ToolError::Tool(format!(
                    "Worker index {} has count={} but inputs.len()={}. Set count to 1 (default) or match inputs length.",
                    spec_index,
                    spec.count,
                    inputs.len()
                )));
            }
            for (instance_index, input) in inputs.iter().enumerate() {
                let trimmed = input.trim();
                if trimmed.is_empty() {
                    return Err(ToolError::Tool(format!(
                        "Worker index {} has empty input at inputs[{}].",
                        spec_index, instance_index
                    )));
                }
                expanded.push((spec_index, trimmed.to_string(), spec.clone()));
            }
            continue;
        }

        if spec.count == 0 {
            return Err(ToolError::Tool(
                "Each worker count must be >= 1.".to_string(),
            ));
        }
        let resolved_input = spec
            .input
            .as_deref()
            .map(str::trim)
            .filter(|input| !input.is_empty())
            .or_else(|| {
                fallback_input
                    .map(str::trim)
                    .filter(|input| !input.is_empty())
            })
            .ok_or_else(|| {
                ToolError::Tool(format!(
                    "Worker index {} requires non-empty 'input' or top-level input.",
                    spec_index
                ))
            })?;
        for _ in 0..spec.count {
            expanded.push((spec_index, resolved_input.to_string(), spec.clone()));
        }
    }
    if expanded.is_empty() {
        return Err(ToolError::Tool(
            "No background workers requested.".to_string(),
        ));
    }
    Ok(expanded)
}

fn extract_task_id(value: &Value) -> Option<String> {
    value
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            value
                .get("task")
                .and_then(|task| task.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute_run_batch(
    tool: &BackgroundAgentTool,
    agent_id: Option<String>,
    name: Option<String>,
    input: Option<String>,
    inputs: Option<Vec<String>>,
    workers: Option<Vec<BackgroundBatchWorkerSpec>>,
    team: Option<String>,
    save_as_team: Option<String>,
    input_template: Option<String>,
    chat_session_id: Option<String>,
    schedule: Option<ContractTaskSchedule>,
    timeout_secs: Option<u64>,
    durability_mode: Option<ContractDurabilityMode>,
    memory: Option<ContractMemoryConfig>,
    memory_scope: Option<String>,
    resource_limits: Option<ContractResourceLimits>,
    run_now: Option<bool>,
    preview: bool,
    confirmation_token: Option<String>,
) -> Result<ToolOutput> {
    tool.write_guard()?;
    if input_template.is_some() {
        return Err(ToolError::Tool(
            "run_batch does not support 'input_template'. Pass runtime 'input' or 'inputs' instead."
                .to_string(),
        ));
    }

    let run_group_id = format!("bg-batch-{}", now_unix_seconds());
    let resolved_workers = match (workers, team.as_deref()) {
        (Some(_), Some(_)) => {
            return Err(ToolError::Tool(
                "run_batch accepts either 'workers' or 'team', not both.".to_string(),
            ));
        }
        (Some(specs), None) => specs,
        (None, Some(team_name)) => {
            let store = tool.team_store()?;
            load_team_workers(store.as_ref(), team_name)?
        }
        (None, None) => {
            return Err(ToolError::Tool(
                "run_batch requires 'workers' or 'team'.".to_string(),
            ));
        }
    };

    if let Some(team_name) = save_as_team.as_deref() {
        let store = tool.team_store()?;
        save_team_workers(store.as_ref(), team_name, &resolved_workers, false)?;
    }

    let expanded_workers =
        expand_worker_specs(&resolved_workers, input.as_deref(), inputs.as_deref())?;
    let resolved_agent_ids = expanded_workers
        .iter()
        .map(|(spec_index, _, worker_spec)| {
            worker_spec
                .agent_id
                .clone()
                .or_else(|| agent_id.clone())
                .ok_or_else(|| {
                    ToolError::Tool(format!(
                        "Worker index {} requires agent_id (set per worker or top-level).",
                        spec_index
                    ))
                })
        })
        .collect::<Result<Vec<_>>>()?;
    let should_run_now = run_now.unwrap_or(true);
    let assessment = tool
        .assessor()?
        .assess_background_agent_template(
            "run_batch",
            if should_run_now {
                OperationAssessmentIntent::Run
            } else {
                OperationAssessmentIntent::Save
            },
            resolved_agent_ids.clone(),
            false,
        )
        .await?;
    if preview {
        return Ok(preview_output(assessment));
    }
    enforce_confirmation(&assessment, confirmation_token.as_deref())?;
    let default_name_prefix = name.unwrap_or_else(|| format!("Background Batch {}", run_group_id));
    let mut tasks = Vec::with_capacity(expanded_workers.len());

    for (worker_index, ((spec_index, worker_input, worker_spec), resolved_agent_id)) in
        expanded_workers
            .into_iter()
            .zip(resolved_agent_ids.into_iter())
            .enumerate()
    {
        let worker_name = worker_spec
            .name
            .clone()
            .unwrap_or_else(|| format!("{} - {}", default_name_prefix, worker_index + 1));
        let created = tool
            .store
            .create_background_agent(BackgroundAgentCreateRequest {
                name: worker_name,
                agent_id: resolved_agent_id,
                chat_session_id: worker_spec
                    .chat_session_id
                    .clone()
                    .or_else(|| chat_session_id.clone()),
                schedule: worker_spec.schedule.clone().or_else(|| schedule.clone()),
                input: Some(worker_input),
                input_template: None,
                timeout_secs: worker_spec.timeout_secs.or(timeout_secs),
                durability_mode: worker_spec
                    .durability_mode
                    .clone()
                    .or_else(|| durability_mode.clone()),
                memory: worker_spec.memory.clone().or_else(|| memory.clone()),
                memory_scope: worker_spec
                    .memory_scope
                    .clone()
                    .or_else(|| memory_scope.clone()),
                resource_limits: worker_spec
                    .resource_limits
                    .clone()
                    .or_else(|| resource_limits.clone()),
            })
            .map_err(|e| {
                ToolError::Tool(format!(
                    "Failed to create background agent for worker {}: {e}.",
                    worker_index + 1
                ))
            })?;

        let task_id = extract_task_id(&created).ok_or_else(|| {
            ToolError::Tool(format!(
                "Failed to extract task id from worker {} create result.",
                worker_index + 1
            ))
        })?;

        if should_run_now {
            tool.store
                .control_background_agent(BackgroundAgentControlRequest {
                    id: task_id.clone(),
                    action: "run_now".to_string(),
                })
                .map_err(|e| {
                    ToolError::Tool(format!("Failed to run background agent {}: {e}.", task_id))
                })?;
        }

        tasks.push(json!({
            "run_group_id": run_group_id.clone(),
            "worker_index": worker_index,
            "spec_index": spec_index,
            "task_id": task_id,
            "run_now": should_run_now,
            "task": created
        }));
    }

    Ok(ToolOutput::success(json!({
        "operation": "run_batch",
        "run_group_id": run_group_id,
        "total": tasks.len(),
        "run_now": should_run_now,
        "team": team,
        "tasks": tasks
    })))
}
