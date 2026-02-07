//! spawn_agent tool - Spawn a sub-agent to work on a task in parallel.

use super::{SubagentDeps, Tool, ToolResult};
use crate::runtime::subagent::{SpawnRequest, spawn_subagent};
use async_trait::async_trait;
use restflow_ai::error::{AiError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::time::{Duration, timeout};
use ts_rs::TS;

/// Parameters for spawn_agent tool.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SpawnAgentParams {
    /// Agent type to spawn (researcher, coder, reviewer, writer, analyst).
    pub agent: String,

    /// Task description for the agent.
    pub task: String,

    /// If true, wait for completion. If false (default), run in background.
    #[serde(default)]
    pub wait: bool,

    /// Timeout in seconds (default: 300).
    pub timeout_secs: Option<u64>,
}

/// spawn_agent tool for the unified agent.
pub struct SpawnAgentTool {
    deps: Arc<SubagentDeps>,
}

impl SpawnAgentTool {
    /// Create a new spawn_agent tool.
    pub fn new(deps: Arc<SubagentDeps>) -> Self {
        Self { deps }
    }
}

#[async_trait]
impl Tool for SpawnAgentTool {
    fn name(&self) -> &str {
        "spawn_agent"
    }

    fn description(&self) -> &str {
        "Spawn a specialized agent to work on a task in parallel. The agent runs in the background; call wait_agents to check completion. Use this for tasks that can be delegated to specialists."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent": {
                    "type": "string",
                    "enum": ["researcher", "coder", "reviewer", "writer", "analyst"],
                    "description": "The specialized agent to spawn"
                },
                "task": {
                    "type": "string",
                    "description": "Detailed task description for the agent"
                },
                "wait": {
                    "type": "boolean",
                    "default": false,
                    "description": "If true, wait for completion. If false (default), run in background."
                },
                "timeout_secs": {
                    "type": "integer",
                    "default": 300,
                    "description": "Timeout in seconds (default: 300)"
                }
            },
            "required": ["agent", "task"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolResult> {
        let params: SpawnAgentParams = serde_json::from_value(input)
            .map_err(|e| AiError::Tool(format!("Invalid parameters: {}", e)))?;

        let request = SpawnRequest {
            agent_id: params.agent.clone(),
            task: params.task.clone(),
            timeout_secs: params.timeout_secs,
            priority: None,
        };

        let handle = spawn_subagent(
            self.deps.tracker.clone(),
            self.deps.definitions.clone(),
            self.deps.llm_client.clone(),
            self.deps.config.clone(),
            request,
        )
        .map_err(|e| AiError::Tool(e.to_string()))?;

        if params.wait {
            let wait_timeout = params
                .timeout_secs
                .unwrap_or(self.deps.config.subagent_timeout_secs);

            let result = match timeout(
                Duration::from_secs(wait_timeout),
                self.deps.tracker.wait(&handle.id),
            )
            .await
            {
                Ok(Some(result)) => result,
                Ok(None) => {
                    return Ok(ToolResult::error("Sub-agent not found"));
                }
                Err(_) => {
                    let output = json!({
                        "agent": handle.agent_name,
                        "status": "timeout",
                        "message": "Timeout waiting for sub-agent"
                    });
                    return Ok(ToolResult::success(output));
                }
            };

            let output = if result.success {
                json!({
                    "agent": handle.agent_name,
                    "status": "completed",
                    "output": result.output,
                    "duration_ms": result.duration_ms
                })
            } else {
                json!({
                    "agent": handle.agent_name,
                    "status": "failed",
                    "error": result.error.unwrap_or_else(|| "Unknown error".to_string()),
                    "duration_ms": result.duration_ms
                })
            };

            Ok(ToolResult::success(output))
        } else {
            let output = json!({
                "task_id": handle.id,
                "agent": handle.agent_name,
                "status": "spawned",
                "message": format!(
                    "Agent '{}' is now working on the task in background. Use wait_agents to check completion.",
                    handle.agent_name
                )
            });
            Ok(ToolResult::success(output))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_params_deserialization() {
        let json = r#"{
            "agent": "researcher",
            "task": "Research topic X"
        }"#;

        let params: SpawnAgentParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.agent, "researcher");
        assert!(!params.wait);
    }

    #[test]
    fn test_params_with_wait() {
        let json = r#"{
            "agent": "coder",
            "task": "Write function Y",
            "wait": true,
            "timeout_secs": 600
        }"#;

        let params: SpawnAgentParams = serde_json::from_str(json).unwrap();
        assert!(params.wait);
        assert_eq!(params.timeout_secs, Some(600));
    }
}
