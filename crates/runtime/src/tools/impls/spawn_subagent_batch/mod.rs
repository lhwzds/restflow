//! spawn_subagent_batch tool - Batch spawn sub-agents.

#[cfg(test)]
mod tests;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::time::{Duration, timeout};

use crate::tools::impls::operation_assessment::{enforce_confirmation_or_defer, preview_output};
use crate::tools::{Result, Tool, ToolError, ToolOutput};
use ::types::AgentOperationAssessor;
use ::types::{SubagentManager, subagent::SubagentDefSummary};
use types::request::{
    InlineAgentRunConfig as ContractInlineAgentRunConfig,
    RunSpawnRequest as ContractRunSpawnRequest,
};
use types::subagent::spawn_request_from_contract;
use types::{SubagentCompletion, SubagentEffectiveLimits, SubagentResult, SubagentStatus};

/// Operation for spawn_subagent_batch tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SpawnSubagentBatchOperation {
    /// Spawn one batch of sub-agents immediately.
    #[default]
    Spawn,
}

fn default_member_count() -> u32 {
    1
}

/// One batch member specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchSubagentSpec {
    /// Optional agent ID or name. If omitted, a temporary sub-agent is created.
    #[serde(default)]
    pub agent: Option<String>,
    /// Number of identical sub-agents to spawn for this spec.
    #[serde(default = "default_member_count")]
    pub count: u32,
    /// Optional transient per-spec task override.
    #[serde(default)]
    pub task: Option<String>,
    /// Optional transient per-instance task list.
    #[serde(default)]
    pub tasks: Option<Vec<String>>,
    /// Optional per-spec timeout passed to sub-agent execution.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    /// Optional model override.
    #[serde(default)]
    pub model: Option<String>,
    /// Optional provider override paired with model.
    #[serde(default)]
    pub provider: Option<String>,
    /// Optional name for temporary sub-agent creation.
    #[serde(default)]
    pub inline_name: Option<String>,
    /// Optional system prompt for temporary sub-agent creation.
    #[serde(default)]
    pub inline_system_prompt: Option<String>,
    /// Optional allowlist for temporary sub-agent tools.
    #[serde(default)]
    pub inline_allowed_tools: Option<Vec<String>>,
    /// Optional max iterations override for temporary sub-agent creation.
    #[serde(default)]
    pub inline_max_iterations: Option<u32>,
}

/// Parameters for spawn_subagent_batch tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnSubagentBatchParams {
    /// Operation to perform.
    #[serde(default)]
    pub operation: SpawnSubagentBatchOperation,
    /// Batch member specs. Required for spawn.
    #[serde(default)]
    pub specs: Option<Vec<BatchSubagentSpec>>,
    /// Default transient task for specs that do not set per-spec task.
    #[serde(default)]
    pub task: Option<String>,
    /// Transient per-instance task list for this spawn.
    #[serde(default)]
    pub tasks: Option<Vec<String>>,
    /// If true, wait for all spawned tasks to complete.
    #[serde(default)]
    pub wait: bool,
    /// Timeout in seconds for wait and as fallback spawn timeout.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    /// Optional parent run ID for context propagation.
    #[serde(default)]
    pub parent_run_id: Option<String>,
    /// If true, validate and preview capability warnings/blockers without executing.
    #[serde(default)]
    pub preview: bool,
    /// Approval ID returned by preview when warnings require explicit confirmation.
    #[serde(default)]
    pub approval_id: Option<String>,
}

#[derive(Debug, Clone)]
struct SpawnedTask {
    task_id: String,
    agent_name: String,
    spec_index: usize,
    instance_index: u32,
    effective_limits: SubagentEffectiveLimits,
}

#[derive(Debug, Clone)]
struct PreparedSpawnRequest {
    spec_index: usize,
    instance_index: u32,
    request: ContractRunSpawnRequest,
}

#[derive(Debug)]
struct SpawnFailure {
    spec_index: usize,
    instance_index: u32,
    error: ToolError,
}

/// spawn_subagent_batch tool for shared agent execution engine.
pub struct SpawnSubagentBatchTool {
    manager: Arc<dyn SubagentManager>,
    assessor: Option<Arc<dyn AgentOperationAssessor>>,
}

impl SpawnSubagentBatchTool {
    pub fn new(manager: Arc<dyn SubagentManager>) -> Self {
        Self {
            manager,
            assessor: None,
        }
    }

    pub fn with_assessor(mut self, assessor: Arc<dyn AgentOperationAssessor>) -> Self {
        self.assessor = Some(assessor);
        self
    }

    fn available_agents(&self) -> Vec<SubagentDefSummary> {
        self.manager.list_callable()
    }
}

#[async_trait]
impl Tool for SpawnSubagentBatchTool {
    fn name(&self) -> &str {
        "spawn_subagent_batch"
    }

    fn description(&self) -> &str {
        "Batch spawn sub-agents with explicit model/count specs."
    }

    fn parameters_schema(&self) -> Value {
        parameters_schema()
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: SpawnSubagentBatchParams = serde_json::from_value(input)
            .map_err(|err| ToolError::Tool(format!("Invalid parameters: {}", err)))?;

        match params.operation {
            SpawnSubagentBatchOperation::Spawn => spawn_batch(self, params).await,
        }
    }
}

fn parameters_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "operation": {
                "type": "string",
                "enum": ["spawn"],
                "default": "spawn",
                "description": "Operation to perform."
            },
            "specs": {
                "type": "array",
                "description": "Batch member specs. Required for spawn.",
                "items": {
                    "type": "object",
                    "properties": {
                        "agent": {
                            "type": "string",
                            "description": "Optional agent ID or name. Omit for a temporary child run."
                        },
                        "count": {
                            "type": "integer",
                            "minimum": 1,
                            "default": 1,
                            "description": "How many child runs to spawn for this spec."
                        },
                        "task": {
                            "type": "string",
                            "description": "Optional per-spec task override."
                        },
                        "tasks": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional per-instance task list. When set, each spawned instance uses one prompt from this list."
                        },
                        "timeout_secs": {
                            "type": "integer",
                            "minimum": 0,
                            "description": "Optional per-spec timeout in seconds."
                        },
                        "model": {
                            "type": "string",
                            "description": "Optional model override."
                        },
                        "provider": {
                            "type": "string",
                            "description": "Optional provider paired with model."
                        },
                        "inline_name": {
                            "type": "string",
                            "description": "Optional temporary child-run name."
                        },
                        "inline_system_prompt": {
                            "type": "string",
                            "description": "Optional temporary child-run system prompt."
                        },
                        "inline_allowed_tools": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional temporary child-run tool allowlist."
                        },
                        "inline_max_iterations": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "Optional temporary child-run max iterations."
                        }
                    }
                }
            },
            "task": {
                "type": "string",
                "description": "Default task for specs that do not define per-spec 'task' or 'tasks'."
            },
            "tasks": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Per-instance task list for this spawn. Tasks are assigned in spec order."
            },
            "wait": {
                "type": "boolean",
                "default": false,
                "description": "If true, wait for all spawned tasks."
            },
            "timeout_secs": {
                "type": "integer",
                "minimum": 0,
                "description": "Wait timeout and fallback child-run timeout (seconds). Use 0 for no wait timeout."
            },
            "parent_run_id": {
                "type": "string",
                "description": "Optional parent run ID for context propagation (runtime-injected)."
            },
            "preview": {
                "type": "boolean",
                "description": "If true, validate capability warnings/blockers without executing."
            },
            "approval_id": {
                "type": "string",
                "description": "Approval ID returned by preview when warnings require explicit confirmation."
            }
        }
    })
}

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

fn preview_request_from_spec(spec: &BatchSubagentSpec) -> ContractRunSpawnRequest {
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
    }
}

fn spawn_request_from_spec(
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
    }
}

fn total_instances(specs: &[BatchSubagentSpec]) -> Result<usize> {
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

fn validate_structural_specs(
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

fn structural_count(spec: &BatchSubagentSpec, spec_index: usize) -> Result<u32> {
    if spec.count == 0 {
        return Err(ToolError::Tool(format!(
            "Spec index {} count must be >= 1.",
            spec_index
        )));
    }
    Ok(spec
        .tasks
        .as_ref()
        .map_or(spec.count, |tasks| tasks.len() as u32))
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

fn resolve_batch_tasks(
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

fn specs_for_spawn(
    tool: &SpawnSubagentBatchTool,
    params: &SpawnSubagentBatchParams,
) -> Result<Vec<BatchSubagentSpec>> {
    let specs = params
        .specs
        .clone()
        .ok_or_else(|| ToolError::Tool("Spawn requires non-empty 'specs'.".to_string()))?;

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

async fn spawn_batch(
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
    for (spec_index, (spec, instance_tasks)) in specs.iter().zip(resolved_tasks).enumerate() {
        for (instance_index, task) in instance_tasks.into_iter().enumerate() {
            if instance_index > u32::MAX as usize {
                return Err(ToolError::Tool(format!(
                    "Spec index {} has too many instances to index as u32.",
                    spec_index
                )));
            }
            let request = spawn_request_from_spec(spec, task, &params);
            prepared.push(PreparedSpawnRequest {
                spec_index,
                instance_index: instance_index as u32,
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
        if let Some(output) =
            enforce_confirmation_or_defer(&assessment, params.approval_id.as_deref())?
        {
            return Ok(output);
        }
    } else if params.preview {
        return Err(ToolError::Tool(
            "Sub-agent capability preview is unavailable in this runtime.".to_string(),
        ));
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
        "results": results
    })))
}
