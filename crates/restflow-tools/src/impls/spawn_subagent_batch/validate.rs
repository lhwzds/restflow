use crate::{Result, ToolError};
use restflow_traits::RuntimeTaskPayload;
use restflow_traits::boundary::subagent::spawn_request_from_contract;

use super::SpawnSubagentBatchTool;
use super::resolve::preview_request_from_spec;
use super::team::structural_count;
use super::types::BatchSubagentSpec;

pub(super) fn total_instances(specs: &[BatchSubagentSpec]) -> Result<usize> {
    let mut total: usize = 0;
    for (spec_index, spec) in specs.iter().enumerate() {
        if spec.task.is_some() && spec.tasks.is_some() {
            return Err(ToolError::Tool(format!(
                "Spec index {} cannot set both 'task' and 'tasks'.",
                spec_index
            )));
        }

        if let Some(tasks) = &spec.tasks {
            if tasks.is_empty() {
                return Err(ToolError::Tool(format!(
                    "Spec index {} has empty 'tasks'.",
                    spec_index
                )));
            }

            for (task_index, task) in tasks.iter().enumerate() {
                if task.trim().is_empty() {
                    return Err(ToolError::Tool(format!(
                        "Spec index {} has empty task at tasks[{}].",
                        spec_index, task_index
                    )));
                }
            }

            if spec.count != 1 && spec.count as usize != tasks.len() {
                return Err(ToolError::Tool(format!(
                    "Spec index {} has count={} but tasks.len()={}. Set count to 1 (default) or match tasks length.",
                    spec_index,
                    spec.count,
                    tasks.len()
                )));
            }

            total = total.saturating_add(tasks.len());
            continue;
        }

        if spec.count == 0 {
            return Err(ToolError::Tool("Each spec count must be >= 1.".to_string()));
        }
        total = total.saturating_add(spec.count as usize);
    }
    if total == 0 {
        return Err(ToolError::Tool("No sub-agents requested.".to_string()));
    }
    Ok(total)
}

pub(super) fn validate_save_team_request(
    task: Option<&str>,
    tasks: Option<&[String]>,
    specs: &[BatchSubagentSpec],
) -> Result<()> {
    RuntimeTaskPayload {
        task: task.map(str::to_string),
        tasks: tasks.map(|items| items.to_vec()),
    }
    .validate("task", "tasks")
    .map_err(ToolError::Tool)?;
    if task.is_some() || tasks.is_some() {
        return Err(ToolError::Tool(
            "save_team stores worker structure only. Remove top-level 'task'/'tasks' and pass prompts during spawn.".to_string(),
        ));
    }
    for (spec_index, spec) in specs.iter().enumerate() {
        if spec.task.is_some() || spec.tasks.is_some() {
            return Err(ToolError::Tool(format!(
                "save_team stores worker structure only. Remove 'task'/'tasks' from spec index {} and pass prompts during spawn.",
                spec_index
            )));
        }
    }
    Ok(())
}

pub(super) fn validate_structural_specs(
    tool: &SpawnSubagentBatchTool,
    specs: &[BatchSubagentSpec],
) -> Result<()> {
    let _ = total_instances(specs)?;
    for spec in specs {
        let _ =
            spawn_request_from_contract(&tool.available_agents(), preview_request_from_spec(spec))?;
    }
    Ok(())
}

fn resolve_instance_tasks(
    spec: &BatchSubagentSpec,
    fallback_task: Option<&str>,
    spec_index: usize,
) -> Result<Vec<String>> {
    if spec.task.is_some() && spec.tasks.is_some() {
        return Err(ToolError::Tool(format!(
            "Spec index {} cannot set both 'task' and 'tasks'.",
            spec_index
        )));
    }

    if let Some(tasks) = &spec.tasks {
        if tasks.is_empty() {
            return Err(ToolError::Tool(format!(
                "Spec index {} has empty 'tasks'.",
                spec_index
            )));
        }

        let mut resolved = Vec::with_capacity(tasks.len());
        for (task_index, task) in tasks.iter().enumerate() {
            let trimmed = task.trim();
            if trimmed.is_empty() {
                return Err(ToolError::Tool(format!(
                    "Spec index {} has empty task at tasks[{}].",
                    spec_index, task_index
                )));
            }
            resolved.push(trimmed.to_string());
        }
        return Ok(resolved);
    }

    let task = spec.task.as_deref().or(fallback_task).ok_or_else(|| {
        ToolError::Tool(format!(
            "Missing task for spec index {}. Provide top-level 'task', top-level 'tasks', per-spec 'task', or per-spec 'tasks'.",
            spec_index
        ))
    })?;
    let trimmed = task.trim();
    if trimmed.is_empty() {
        return Err(ToolError::Tool(format!(
            "Task for spec index {} must not be empty.",
            spec_index
        )));
    }

    Ok((0..spec.count).map(|_| trimmed.to_string()).collect())
}

pub(super) fn resolve_batch_tasks(
    specs: &[BatchSubagentSpec],
    fallback_task: Option<&str>,
    fallback_tasks: Option<&[String]>,
) -> Result<Vec<Vec<String>>> {
    if fallback_task.is_some() && fallback_tasks.is_some() {
        return Err(ToolError::Tool(
            "Use either top-level 'task' or top-level 'tasks', not both.".to_string(),
        ));
    }

    if let Some(tasks) = fallback_tasks {
        if tasks.is_empty() {
            return Err(ToolError::Tool(
                "Top-level 'tasks' must not be empty.".to_string(),
            ));
        }

        for (spec_index, spec) in specs.iter().enumerate() {
            if spec.task.is_some() || spec.tasks.is_some() {
                return Err(ToolError::Tool(format!(
                    "Top-level 'tasks' cannot be combined with per-spec 'task' or 'tasks' (spec index {}).",
                    spec_index
                )));
            }
        }

        let mut normalized = Vec::with_capacity(tasks.len());
        for (task_index, task) in tasks.iter().enumerate() {
            let trimmed = task.trim();
            if trimmed.is_empty() {
                return Err(ToolError::Tool(format!(
                    "Top-level 'tasks' has empty task at index {}.",
                    task_index
                )));
            }
            normalized.push(trimmed.to_string());
        }

        let expected = total_instances(specs)?;
        if normalized.len() != expected {
            return Err(ToolError::Tool(format!(
                "Top-level 'tasks' length {} does not match total requested instances {}.",
                normalized.len(),
                expected
            )));
        }

        let mut offset = 0usize;
        let mut resolved = Vec::with_capacity(specs.len());
        for (spec_index, spec) in specs.iter().enumerate() {
            let count = usize::try_from(structural_count(spec, spec_index)?).map_err(|_| {
                ToolError::Tool(format!(
                    "Spec index {} count exceeds supported runtime size.",
                    spec_index
                ))
            })?;
            let end = offset + count;
            resolved.push(normalized[offset..end].to_vec());
            offset = end;
        }

        return Ok(resolved);
    }

    specs
        .iter()
        .enumerate()
        .map(|(spec_index, spec)| resolve_instance_tasks(spec, fallback_task, spec_index))
        .collect()
}
