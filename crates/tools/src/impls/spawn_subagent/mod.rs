//! spawn_subagent tool - Spawn a sub-agent to work on a task in parallel.

#[cfg(test)]
mod tests;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::time::{Duration, timeout};

use crate::impls::operation_assessment::{enforce_confirmation_or_defer, preview_output};
use crate::impls::spawn_subagent_batch::{
    BatchSubagentSpec, SpawnSubagentBatchOperation, SpawnSubagentBatchTool,
};
use crate::{Result, Tool, ToolError, ToolOutput};
use ::types::AgentOperationAssessor;
use ::types::{SubagentManager, subagent::SubagentDefSummary};
use types::request::{
    InlineAgentRunConfig as ContractInlineAgentRunConfig,
    RunSpawnRequest as ContractRunSpawnRequest,
};
use types::{DEFAULT_SUBAGENT_TIMEOUT_SECS, SubagentCompletion, SubagentStatus};

/// Parameters for spawn_subagent tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnSubagentParams {
    /// Operation to perform. Defaults to `spawn`.
    #[serde(default)]
    pub operation: SpawnSubagentBatchOperation,

    /// Agent type to spawn. When omitted, runtime creates a temporary sub-agent
    /// from inline config.
    #[serde(default)]
    pub agent: Option<String>,

    /// Task description for single spawn, or transient fallback task for batch spawn.
    #[serde(default)]
    pub task: Option<String>,

    /// Transient per-instance task list for batch spawn.
    #[serde(default)]
    pub tasks: Option<Vec<String>>,

    /// If true, wait for completion. If false (default), run concurrently.
    #[serde(default)]
    pub wait: bool,

    /// Timeout in seconds. If omitted, uses sub-agent manager default timeout.
    #[serde(default)]
    pub timeout_secs: Option<u64>,

    /// Optional model override for this spawn.
    #[serde(default)]
    pub model: Option<String>,

    /// Optional provider selector paired with model.
    #[serde(default)]
    pub provider: Option<String>,

    /// Optional parent run ID (runtime-injected, internal use).
    #[serde(default)]
    pub parent_run_id: Option<String>,

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

    /// Optional list-based worker specs for unified single/multi spawn.
    #[serde(default)]
    pub workers: Option<Vec<BatchSubagentSpec>>,

    /// If true, validate and preview capability warnings/blockers without executing.
    #[serde(default)]
    pub preview: bool,

    /// Approval ID returned by preview when warnings require explicit confirmation.
    #[serde(default)]
    pub approval_id: Option<String>,
}

/// spawn_subagent tool for the shared agent execution engine.
pub struct SpawnSubagentTool {
    manager: Arc<dyn SubagentManager>,
    assessor: Option<Arc<dyn AgentOperationAssessor>>,
}

impl SpawnSubagentTool {
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
impl Tool for SpawnSubagentTool {
    fn name(&self) -> &str {
        "spawn_subagent"
    }

    fn description(&self) -> &str {
        "Spawn a specialized sub-agent to work on a task in parallel. Use wait_subagents to check completion."
    }

    fn parameters_schema(&self) -> Value {
        parameters_schema(&self.available_agents())
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: SpawnSubagentParams = serde_json::from_value(input)
            .map_err(|e| ToolError::Tool(format!("Invalid parameters: {}", e)))?;
        execute_spawn(self, params).await
    }
}

fn parameters_schema(available: &[SubagentDefSummary]) -> Value {
    let agent_property = if available.is_empty() {
        json!({
            "type": "string",
            "description": "Optional agent ID or name. Omit to create a temporary sub-agent. Call list_subagents to discover available agents."
        })
    } else {
        let enum_values: Vec<String> = available.iter().map(|agent| agent.id.clone()).collect();
        let enum_labels: Vec<String> = available
            .iter()
            .map(|agent| format!("{} ({})", agent.name, agent.id))
            .collect();
        json!({
            "type": "string",
            "enum": enum_values,
            "x-enumNames": enum_labels,
            "description": "Optional agent ID. You can also pass agent name at runtime. Omit to create a temporary sub-agent."
        })
    };

    json!({
        "type": "object",
        "properties": {
            "operation": {
                "type": "string",
                "enum": ["spawn"],
                "default": "spawn",
                "description": "Operation to perform."
            },
            "agent": agent_property,
            "task": {
                "type": "string",
                "description": "Detailed task description for single spawn, or transient fallback task for batch worker specs. Required for single spawn."
            },
            "tasks": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Transient per-instance task list for batch spawn. Tasks are assigned in worker order."
            },
            "wait": {
                "type": "boolean",
                "default": false,
                "description": "If true, wait for completion. Applies to spawn only."
            },
            "timeout_secs": {
                "type": "integer",
                "default": DEFAULT_SUBAGENT_TIMEOUT_SECS,
                "description": format!(
                    "Timeout in seconds for single spawn or batch spawn (default: {})",
                    DEFAULT_SUBAGENT_TIMEOUT_SECS
                )
            },
            "model": {
                "type": "string",
                "description": "Optional model override for this sub-agent (e.g., 'minimax/coding-plan')"
            },
            "provider": {
                "type": "string",
                "description": "Provider selector paired with model override (e.g., 'openai-codex'). Required when model is set."
            },
            "parent_run_id": {
                "type": "string",
                "description": "Optional parent run ID for context propagation (runtime-injected)"
            },
            "inline_name": {
                "type": "string",
                "description": "Optional temporary sub-agent name when 'agent' is omitted."
            },
            "inline_system_prompt": {
                "type": "string",
                "description": "Optional system prompt for temporary sub-agent when 'agent' is omitted."
            },
            "inline_allowed_tools": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Optional tool allowlist for temporary sub-agent when 'agent' is omitted."
            },
            "inline_max_iterations": {
                "type": "integer",
                "minimum": 1,
                "description": "Optional max iterations for temporary sub-agent when 'agent' is omitted."
            },
            "workers": {
                "type": "array",
                "description": "Optional unified list-based batch specs. Use for batch spawn.",
                "items": {
                    "type": "object",
                    "properties": {
                        "agent": { "type": "string", "description": "Optional agent ID or name." },
                        "count": { "type": "integer", "minimum": 1, "default": 1, "description": "Number of instances for this worker spec." },
                        "task": { "type": "string", "description": "Optional transient per-worker task override." },
                        "tasks": { "type": "array", "items": { "type": "string" }, "description": "Optional transient per-instance task list for distinct prompts." },
                        "timeout_secs": { "type": "integer", "minimum": 0, "description": "Optional per-worker timeout." },
                        "model": { "type": "string", "description": "Optional model override for this worker." },
                        "provider": { "type": "string", "description": "Optional provider paired with model." },
                        "inline_name": { "type": "string", "description": "Optional temporary sub-agent name." },
                        "inline_system_prompt": { "type": "string", "description": "Optional temporary sub-agent system prompt." },
                        "inline_allowed_tools": { "type": "array", "items": { "type": "string" }, "description": "Optional temporary sub-agent tool allowlist." },
                        "inline_max_iterations": { "type": "integer", "minimum": 1, "description": "Optional temporary sub-agent max iterations." }
                    }
                }
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

fn completion_output(
    task_id: &str,
    agent_name: &str,
    completion: SubagentCompletion,
    effective_limits: &types::SubagentEffectiveLimits,
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

fn build_inline_config(params: &SpawnSubagentParams) -> Option<ContractInlineAgentRunConfig> {
    let config = ContractInlineAgentRunConfig {
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
    params.workers.is_some() || params.tasks.is_some()
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

fn build_contract_request(params: &SpawnSubagentParams, task: String) -> ContractRunSpawnRequest {
    ContractRunSpawnRequest {
        agent_id: params.agent.clone(),
        inline: build_inline_config(params),
        task,
        timeout_secs: params.timeout_secs,
        max_iterations: None,
        priority: None,
        model: params.model.clone(),
        model_provider: params.provider.clone(),
        parent_run_id: params.parent_run_id.clone(),
    }
}

async fn execute_spawn(
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
                "Batch mode uses 'workers'; do not combine with single-spawn fields like 'agent', top-level model/provider, or top-level inline settings.".to_string(),
            ));
        }

        let mut batch_tool = SpawnSubagentBatchTool::new(tool.manager.clone());
        if let Some(assessor) = tool.assessor.clone() {
            batch_tool = batch_tool.with_assessor(assessor);
        }

        let operation = params.operation.clone();
        let task = normalize_optional_text(params.task.as_deref());
        let tasks = params.tasks.clone();

        return batch_tool
            .execute(json!({
                "operation": operation,
                "specs": params.workers,
                "task": task,
                "tasks": tasks,
                "wait": params.wait,
                "timeout_secs": params.timeout_secs,
                "parent_run_id": params.parent_run_id,
                "preview": params.preview,
                "approval_id": params.approval_id
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
            enforce_confirmation_or_defer(&assessment, params.approval_id.as_deref())?
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
