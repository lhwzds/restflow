//! Agent task management tool.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

use super::traits::{Tool, ToolOutput};
use crate::error::{AiError, Result};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskCreateRequest {
    pub name: String,
    pub agent_id: String,
    #[serde(default)]
    pub schedule: Option<Value>,
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub input_template: Option<String>,
    #[serde(default)]
    pub memory_scope: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskUpdateRequest {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub input_template: Option<String>,
    #[serde(default)]
    pub schedule: Option<Value>,
    #[serde(default)]
    pub notification: Option<Value>,
    #[serde(default)]
    pub execution_mode: Option<Value>,
    #[serde(default)]
    pub memory: Option<Value>,
    #[serde(default)]
    pub memory_scope: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskControlRequest {
    pub id: String,
    pub action: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskProgressRequest {
    pub id: String,
    #[serde(default)]
    pub event_limit: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskMessageRequest {
    pub id: String,
    pub message: String,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskMessageListRequest {
    pub id: String,
    #[serde(default)]
    pub limit: Option<usize>,
}

pub trait TaskStore: Send + Sync {
    fn create_task(&self, request: TaskCreateRequest) -> Result<Value>;
    fn update_task(&self, request: TaskUpdateRequest) -> Result<Value>;
    fn delete_task(&self, id: &str) -> Result<Value>;
    fn list_tasks(&self, status: Option<String>) -> Result<Value>;
    fn control_task(&self, request: TaskControlRequest) -> Result<Value>;
    fn get_progress(&self, request: TaskProgressRequest) -> Result<Value>;
    fn send_message(&self, request: TaskMessageRequest) -> Result<Value>;
    fn list_messages(&self, request: TaskMessageListRequest) -> Result<Value>;
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
        #[serde(default)]
        input_template: Option<String>,
        #[serde(default)]
        memory_scope: Option<String>,
    },
    Update {
        id: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        agent_id: Option<String>,
        #[serde(default)]
        input: Option<String>,
        #[serde(default)]
        input_template: Option<String>,
        #[serde(default)]
        schedule: Option<Value>,
        #[serde(default)]
        notification: Option<Value>,
        #[serde(default)]
        execution_mode: Option<Value>,
        #[serde(default)]
        memory: Option<Value>,
        #[serde(default)]
        memory_scope: Option<String>,
    },
    Delete {
        id: String,
    },
    List {
        #[serde(default)]
        status: Option<String>,
    },
    Control {
        id: String,
        action: String,
    },
    Progress {
        id: String,
        #[serde(default)]
        event_limit: Option<usize>,
    },
    SendMessage {
        id: String,
        message: String,
        #[serde(default)]
        source: Option<String>,
    },
    ListMessages {
        id: String,
        #[serde(default)]
        limit: Option<usize>,
    },
    Pause {
        id: String,
    },
    Resume {
        id: String,
    },
    Cancel {
        id: String,
    },
    Run {
        id: String,
    },
}

#[async_trait]
impl Tool for TaskTool {
    fn name(&self) -> &str {
        "manage_tasks"
    }

    fn description(&self) -> &str {
        "Create, update, control, inspect progress, message, list, and delete background agent tasks."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": [
                        "create",
                        "update",
                        "delete",
                        "list",
                        "control",
                        "progress",
                        "send_message",
                        "list_messages",
                        "pause",
                        "resume",
                        "cancel",
                        "run"
                    ],
                    "description": "Task operation to perform"
                },
                "id": {
                    "type": "string"
                },
                "name": {
                    "type": "string",
                    "description": "Task name (for create/update)"
                },
                "agent_id": {
                    "type": "string",
                    "description": "Agent ID (for create/update)"
                },
                "description": {
                    "type": "string",
                    "description": "Task description (for update)"
                },
                "schedule": {
                    "type": "object",
                    "description": "Task schedule object (for create/update)"
                },
                "notification": {
                    "type": "object",
                    "description": "Notification configuration (for update)"
                },
                "execution_mode": {
                    "type": "object",
                    "description": "Execution mode payload (for update)"
                },
                "memory": {
                    "type": "object",
                    "description": "Memory configuration payload (for update)"
                },
                "input": {
                    "type": "string",
                    "description": "Optional input for the task (for create/update)"
                },
                "input_template": {
                    "type": "string",
                    "description": "Optional runtime template for task input (for create/update)"
                },
                "memory_scope": {
                    "type": "string",
                    "enum": ["shared_agent", "per_task"],
                    "description": "Memory namespace scope (for create/update)"
                },
                "status": {
                    "type": "string",
                    "description": "Filter list by status (for list)"
                },
                "action": {
                    "type": "string",
                    "enum": ["start", "pause", "resume", "stop", "run_now"],
                    "description": "Control action (for control)"
                },
                "event_limit": {
                    "type": "integer",
                    "description": "Recent event count for progress"
                },
                "message": {
                    "type": "string",
                    "description": "Message content for send_message"
                },
                "source": {
                    "type": "string",
                    "enum": ["user", "agent", "system"],
                    "description": "Message source for send_message"
                },
                "limit": {
                    "type": "integer",
                    "description": "Message list limit for list_messages"
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
                input_template,
                memory_scope,
            } => {
                self.write_guard()?;
                let result = self.store.create_task(TaskCreateRequest {
                    name,
                    agent_id,
                    schedule,
                    input,
                    input_template,
                    memory_scope,
                })?;
                ToolOutput::success(result)
            }
            TaskAction::Update {
                id,
                name,
                description,
                agent_id,
                input,
                input_template,
                schedule,
                notification,
                execution_mode,
                memory,
                memory_scope,
            } => {
                self.write_guard()?;
                let result = self.store.update_task(TaskUpdateRequest {
                    id,
                    name,
                    description,
                    agent_id,
                    input,
                    input_template,
                    schedule,
                    notification,
                    execution_mode,
                    memory,
                    memory_scope,
                })?;
                ToolOutput::success(result)
            }
            TaskAction::Delete { id } => {
                self.write_guard()?;
                ToolOutput::success(self.store.delete_task(&id)?)
            }
            TaskAction::Pause { id } => {
                self.write_guard()?;
                ToolOutput::success(self.store.control_task(TaskControlRequest {
                    id,
                    action: "pause".to_string(),
                })?)
            }
            TaskAction::Resume { id } => {
                self.write_guard()?;
                ToolOutput::success(self.store.control_task(TaskControlRequest {
                    id,
                    action: "resume".to_string(),
                })?)
            }
            TaskAction::Cancel { id } => {
                self.write_guard()?;
                ToolOutput::success(self.store.delete_task(&id)?)
            }
            TaskAction::Run { id } => {
                self.write_guard()?;
                ToolOutput::success(self.store.control_task(TaskControlRequest {
                    id,
                    action: "run_now".to_string(),
                })?)
            }
            TaskAction::Control { id, action } => {
                self.write_guard()?;
                ToolOutput::success(self.store.control_task(TaskControlRequest { id, action })?)
            }
            TaskAction::Progress { id, event_limit } => ToolOutput::success(
                self.store
                    .get_progress(TaskProgressRequest { id, event_limit })?,
            ),
            TaskAction::SendMessage {
                id,
                message,
                source,
            } => {
                self.write_guard()?;
                ToolOutput::success(self.store.send_message(TaskMessageRequest {
                    id,
                    message,
                    source,
                })?)
            }
            TaskAction::ListMessages { id, limit } => ToolOutput::success(
                self.store
                    .list_messages(TaskMessageListRequest { id, limit })?,
            ),
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

        fn update_task(&self, _request: TaskUpdateRequest) -> Result<Value> {
            Ok(json!({ "id": "task-1", "updated": true }))
        }

        fn delete_task(&self, _id: &str) -> Result<Value> {
            Ok(json!({ "deleted": true }))
        }

        fn list_tasks(&self, _status: Option<String>) -> Result<Value> {
            Ok(json!([{"id": "task-1"}]))
        }

        fn control_task(&self, request: TaskControlRequest) -> Result<Value> {
            Ok(json!({ "id": request.id, "action": request.action }))
        }

        fn get_progress(&self, request: TaskProgressRequest) -> Result<Value> {
            Ok(json!({
                "id": request.id,
                "event_limit": request.event_limit.unwrap_or(10),
                "status": "active"
            }))
        }

        fn send_message(&self, request: TaskMessageRequest) -> Result<Value> {
            Ok(json!({
                "id": request.id,
                "message": request.message,
                "source": request.source.unwrap_or_else(|| "user".to_string())
            }))
        }

        fn list_messages(&self, request: TaskMessageListRequest) -> Result<Value> {
            Ok(json!([{
                "id": "msg-1",
                "task_id": request.id,
                "limit": request.limit.unwrap_or(50)
            }]))
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

    #[tokio::test]
    async fn test_progress_operation() {
        let tool = TaskTool::new(Arc::new(MockStore));
        let output = tool
            .execute(json!({
                "operation": "progress",
                "id": "task-1",
                "event_limit": 5
            }))
            .await
            .unwrap();
        assert!(output.success);
    }
}
