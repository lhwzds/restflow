//! spawn_agent tool - Spawn a sub-agent to work on a task in parallel.

use crate::main_agent::{MainAgent, spawn::SpawnRequest};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use ts_rs::TS;

/// Parameters for spawn_agent tool
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SpawnAgentParams {
    /// Agent type to spawn (researcher, coder, reviewer, writer, analyst)
    pub agent: String,

    /// Task description for the agent
    pub task: String,

    /// If true, wait for completion. If false (default), run in background.
    #[serde(default)]
    pub wait: bool,

    /// Timeout in seconds (default: 300)
    pub timeout_secs: Option<u64>,
}

/// spawn_agent tool for the main agent
pub struct SpawnAgentTool {
    main_agent: Arc<MainAgent>,
}

impl SpawnAgentTool {
    /// Create a new spawn_agent tool
    pub fn new(main_agent: Arc<MainAgent>) -> Self {
        Self { main_agent }
    }

    /// Get tool name
    pub fn name(&self) -> &str {
        "spawn_agent"
    }

    /// Get tool description
    pub fn description(&self) -> &str {
        "Spawn a specialized agent to work on a task in parallel. \
         The agent runs in the background and you'll be notified when it completes. \
         Use this for tasks that can be delegated to specialists."
    }

    /// Get JSON schema for parameters
    pub fn parameters_schema(&self) -> Value {
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

    /// Execute the tool
    pub async fn execute(&self, input: Value) -> Result<Value> {
        let params: SpawnAgentParams =
            serde_json::from_value(input).map_err(|e| anyhow!("Invalid parameters: {}", e))?;

        let request = SpawnRequest {
            agent_id: params.agent.clone(),
            task: params.task.clone(),
            timeout_secs: params.timeout_secs,
            priority: None,
        };

        let handle = self.main_agent.spawn_subagent(request)?;

        if params.wait {
            // Synchronous mode: wait for completion
            let result = self
                .main_agent
                .wait_subagent(&handle.id)
                .await
                .ok_or_else(|| anyhow!("Failed to get result"))?;

            if result.success {
                Ok(json!({
                    "agent": handle.agent_name,
                    "status": "completed",
                    "output": result.output,
                    "duration_ms": result.duration_ms
                }))
            } else {
                Ok(json!({
                    "agent": handle.agent_name,
                    "status": "failed",
                    "error": result.error.unwrap_or_else(|| "Unknown error".to_string()),
                    "duration_ms": result.duration_ms
                }))
            }
        } else {
            // Asynchronous mode: return immediately
            Ok(json!({
                "task_id": handle.id,
                "agent": handle.agent_name,
                "status": "spawned",
                "message": format!(
                    "Agent '{}' is now working on the task in background. \
                     You will be notified when it completes.",
                    handle.agent_name
                )
            }))
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
