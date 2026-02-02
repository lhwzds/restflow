//! Spawn tool for creating subagents.

use super::{Tool, ToolDefinition, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::Arc;

#[async_trait]
pub trait SubagentSpawner: Send + Sync {
    async fn spawn(&self, agent_id: &str, task: &str, timeout_secs: Option<u64>) -> Result<String>;
}

pub struct SpawnTool {
    spawner: Arc<dyn SubagentSpawner>,
}

impl SpawnTool {
    pub fn new(spawner: Arc<dyn SubagentSpawner>) -> Self {
        Self { spawner }
    }
}

#[async_trait]
impl Tool for SpawnTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "spawn".to_string(),
            description: "Spawn a subagent with a task.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "Agent ID to spawn"
                    },
                    "task": {
                        "type": "string",
                        "description": "Task for the subagent"
                    },
                    "timeout_secs": {
                        "type": "number",
                        "description": "Optional timeout in seconds"
                    }
                },
                "required": ["agent_id", "task"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let agent_id = args
            .get("agent_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'agent_id' argument"))?;
        let task = args
            .get("task")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'task' argument"))?;
        let timeout_secs = args.get("timeout_secs").and_then(|v| v.as_u64());

        let task_id = self.spawner.spawn(agent_id, task, timeout_secs).await?;
        Ok(ToolResult::success(task_id))
    }
}
