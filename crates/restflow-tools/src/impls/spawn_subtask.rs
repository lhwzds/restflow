//! Spawn subtask tool - convenience wrapper for creating and running background agents.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

use super::background_agent::BackgroundAgentTool;
use crate::{Tool, ToolOutput};
use crate::Result;

/// Simplified request for spawning a subtask
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpawnSubtaskRequest {
    /// Task name
    pub name: String,
    /// Agent ID to use for execution
    pub agent_id: String,
    /// Task input/instruction
    pub input: String,
    /// Optional timeout in seconds (default: 300)
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

/// Convenience tool for spawning sub-agents.
/// This is a simplified interface over manage_background_agents
/// that creates and immediately runs a background task.
pub struct SpawnSubtaskTool {
    background_agent_tool: Arc<BackgroundAgentTool>,
}

impl SpawnSubtaskTool {
    pub fn new(background_agent_tool: Arc<BackgroundAgentTool>) -> Self {
        Self {
            background_agent_tool,
        }
    }
}

#[async_trait]
impl Tool for SpawnSubtaskTool {
    fn name(&self) -> &str {
        "spawn_subtask"
    }

    fn description(&self) -> &str {
        "Spawn a sub-agent to execute a task. Returns task_id for progress tracking. \
         This is a convenience tool for coordinators to delegate work to sub-agents."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Descriptive name for the subtask"
                },
                "agent_id": {
                    "type": "string",
                    "description": "Agent ID to use for execution"
                },
                "input": {
                    "type": "string",
                    "description": "Task instruction/input for the agent"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Optional timeout in seconds (default: 300)"
                }
            },
            "required": ["name", "agent_id", "input"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let request: SpawnSubtaskRequest = serde_json::from_value(input)?;

        // Step 1: Create the background task
        let create_payload = json!({
            "operation": "create",
            "name": request.name,
            "agent_id": request.agent_id,
            "input": request.input,
            "timeout_secs": request.timeout_secs.unwrap_or(300),
            "execution_mode": {
                "type": "api"
            }
        });

        let create_result = self.background_agent_tool.execute(create_payload).await?;

        if !create_result.success {
            return Ok(create_result);
        }

        // Extract task_id from create result
        let task_id = create_result
            .result
            .get("task_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                crate::ToolError::Tool(
                    "Failed to extract task_id from create result".to_string(),
                )
            })?
            .to_string();

        // Step 2: Run the task
        let run_payload = json!({
            "operation": "run",
            "id": task_id
        });

        let run_result = self.background_agent_tool.execute(run_payload).await?;

        if !run_result.success {
            return Ok(run_result);
        }

        // Return task_id and status
        Ok(ToolOutput::success(json!({
            "task_id": task_id,
            "status": "running",
            "message": format!("Spawned subtask '{}' with task_id: {}", request.name, task_id)
        })))
    }
}
