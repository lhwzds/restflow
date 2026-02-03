//! Spawn tool for creating subagents.

use crate::agent::tools::ToolResult;
use async_trait::async_trait;
use restflow_ai::error::{AiError, Result};
use restflow_ai::tools::Tool;
use serde_json::{Value, json};
use std::sync::Arc;

/// Trait for spawning subagents.
pub trait SubagentSpawner: Send + Sync {
    fn spawn(&self, task: String) -> Result<String>;
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
    fn name(&self) -> &str {
        "spawn"
    }

    fn description(&self) -> &str {
        "Spawn a subagent to handle a task and return its task id."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task": {
                    "type": "string",
                    "description": "Task description for the subagent"
                }
            },
            "required": ["task"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let task = args
            .get("task")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AiError::Tool("Missing 'task' argument".to_string()))?;

        let task_id = self.spawner.spawn(task.to_string())?;
        Ok(ToolResult::success(json!(task_id)))
    }
}
