//! Agent task management tool.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

use crate::error::{AiError, Result};
use super::traits::{Tool, ToolOutput};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskCreateRequest {
    pub name: String,
    pub agent_id: String,
    #[serde(default)]
    pub schedule: Option<Value>,
    #[serde(default)]
    pub input: Option<String>,
}

pub trait TaskStore: Send + Sync {
    fn create_task(&self, request: TaskCreateRequest) -> Result<Value>;
    fn list_tasks(&self, status: Option<String>) -> Result<Value>;
    fn pause_task(&self, id: &str) -> Result<Value>;
    fn resume_task(&self, id: &str) -> Result<Value>;
    fn cancel_task(&self, id: &str) -> Result<Value>;
    fn run_task(&self, id: &str) -> Result<Value>;
}

#[derive(Clone)]
pub struct TaskTool {
    store: Arc<dyn TaskStore>,
    allow_write: bool,
}

impl TaskTool {
    pub fn new(store: Arc<dyn TaskStore>) -> Self {
        Self {
            store,
            allow_write: false,
        }
    }

    pub fn with_write(mut self, allow_write: bool) -> Self {
        self.allow_write = allow_write;
        self
    }

    fn write_guard(&self) -> Result<()> {
        if self.allow_write {
            Ok(())
        } else {
            Err(AiError::Tool(
                "Write access to tasks is disabled for this tool".to_string(),
            ))
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum TaskAction {
    Create {
        name: String,
        agent_id: String,
        #[serde(default)]
        schedule: Option<Value>,
        #[serde(default)]
        input: Option<String>,
    },
    List {
        #[serde(default)]
        status: Option<String>,
    },
    Pause { id: String },
    Resume { id: String },
    Cancel { id: String },
    Run { id: String },
}

#[async_trait]
impl Tool for TaskTool {
    fn name(&self) -> &str {
        "manage_tasks"
    }

    fn description(&self) -> &str {
        "Manage scheduled agent tasks. Supports create, list, pause, resume, cancel, and run."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["create", "list", "pause", "resume", "cancel", "run"],
                    "description": "Task operation to perform"
                },
                "id": {
                    "type": "string",
                    "description": "Task ID (for pause/resume/cancel/run)"
                },
                "name": {
                    "type": "string",
                    "description": "Task name (for create)"
                },
                "agent_id": {
                    "type": "string",
                    "description": "Agent ID (for create)"
                },
                "schedule": {
                    "type": "object",
                    "description": "Task schedule object (for create)"
                },
                "input": {
                    "type": "string",
                    "description": "Optional input for the task (for create)"
                },
                "status": {
                    "type": "string",
                    "description": "Filter list by status"
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let action: TaskAction = serde_json::from_value(input)?;

        let output = match action {
            TaskAction::List { status } => {
                let result = self.store.list_tasks(status)?;
                ToolOutput::success(result)
            }
            TaskAction::Create {
                name,
                agent_id,
                schedule,
                input,
            } => {
                self.write_guard()?;
                let result = self.store.create_task(TaskCreateRequest {
                    name,
                    agent_id,
                    schedule,
                    input,
                })?;
                ToolOutput::success(result)
            }
            TaskAction::Pause { id } => {
                self.write_guard()?;
                let result = self.store.pause_task(&id)?;
                ToolOutput::success(result)
            }
            TaskAction::Resume { id } => {
                self.write_guard()?;
                let result = self.store.resume_task(&id)?;
                ToolOutput::success(result)
            }
            TaskAction::Cancel { id } => {
                self.write_guard()?;
                let result = self.store.cancel_task(&id)?;
                ToolOutput::success(result)
            }
            TaskAction::Run { id } => {
                self.write_guard()?;
                let result = self.store.run_task(&id)?;
                ToolOutput::success(result)
            }
        };

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockStore;

    impl TaskStore for MockStore {
        fn create_task(&self, _request: TaskCreateRequest) -> Result<Value> {
            Ok(json!({ "id": "task-1" }))
        }

        fn list_tasks(&self, _status: Option<String>) -> Result<Value> {
            Ok(json!([{"id": "task-1"}]))
        }

        fn pause_task(&self, _id: &str) -> Result<Value> {
            Ok(json!({ "status": "paused" }))
        }

        fn resume_task(&self, _id: &str) -> Result<Value> {
            Ok(json!({ "status": "active" }))
        }

        fn cancel_task(&self, _id: &str) -> Result<Value> {
            Ok(json!({ "deleted": true }))
        }

        fn run_task(&self, _id: &str) -> Result<Value> {
            Ok(json!({ "status": "running" }))
        }
    }

    #[tokio::test]
    async fn test_list_tasks() {
        let tool = TaskTool::new(Arc::new(MockStore));
        let output = tool.execute(json!({ "operation": "list" })).await.unwrap();
        assert!(output.success);
    }

    #[tokio::test]
    async fn test_write_guard() {
        let tool = TaskTool::new(Arc::new(MockStore));
        let result = tool
            .execute(json!({
                "operation": "create",
                "name": "A",
                "agent_id": "agent-1"
            }))
            .await;
        assert!(result.is_err());
    }
}
