//! Spawn tool for creating subagents.

use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::error::{Result, ToolError};
use crate::tool::{Tool, ToolOutput};
use restflow_ai::agent::SubagentSpawner;

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

    async fn execute(&self, args: Value) -> Result<ToolOutput> {
        let task = args
            .get("task")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::Tool("Missing 'task' argument".to_string()))?;

        let task_id = self.spawner.spawn(task.to_string())?;
        Ok(ToolOutput::success(json!(task_id)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::Tool;
    use restflow_ai::tools::ToolError;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct MockSpawner {
        should_fail: AtomicBool,
    }

    impl MockSpawner {
        fn ok() -> Self {
            Self {
                should_fail: AtomicBool::new(false),
            }
        }
        fn failing() -> Self {
            Self {
                should_fail: AtomicBool::new(true),
            }
        }
    }

    impl SubagentSpawner for MockSpawner {
        fn spawn(&self, _task: String) -> std::result::Result<String, ToolError> {
            if self.should_fail.load(Ordering::Relaxed) {
                Err(ToolError::Tool("spawn failed".to_string()))
            } else {
                Ok("mock-task-id".to_string())
            }
        }
    }

    #[tokio::test]
    async fn test_spawn_success() {
        let tool = SpawnTool::new(Arc::new(MockSpawner::ok()));
        let result = tool.execute(json!({"task": "do something"})).await.unwrap();
        assert!(result.success);
        assert_eq!(result.result, json!("mock-task-id"));
    }

    #[tokio::test]
    async fn test_spawn_missing_task() {
        let tool = SpawnTool::new(Arc::new(MockSpawner::ok()));
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_spawn_failure() {
        let tool = SpawnTool::new(Arc::new(MockSpawner::failing()));
        let result = tool.execute(json!({"task": "do something"})).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_name_and_schema() {
        let tool = SpawnTool::new(Arc::new(MockSpawner::ok()));
        assert_eq!(tool.name(), "spawn");
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["task"].is_object());
        assert_eq!(schema["required"][0], "task");
    }
}
