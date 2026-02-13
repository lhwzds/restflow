//! Background agent management tool.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

use super::traits::{Tool, ToolOutput};
use crate::error::{AiError, Result};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackgroundAgentCreateRequest {
    pub name: String,
    pub agent_id: String,
    #[serde(default)]
    pub schedule: Option<Value>,
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub input_template: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub durability_mode: Option<String>,
    #[serde(default)]
    pub memory: Option<Value>,
    #[serde(default)]
    pub memory_scope: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackgroundAgentUpdateRequest {
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
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub durability_mode: Option<String>,
    #[serde(default)]
    pub memory: Option<Value>,
    #[serde(default)]
    pub memory_scope: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackgroundAgentControlRequest {
    pub id: String,
    pub action: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackgroundAgentProgressRequest {
    pub id: String,
    #[serde(default)]
    pub event_limit: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackgroundAgentMessageRequest {
    pub id: String,
    pub message: String,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackgroundAgentMessageListRequest {
    pub id: String,
    #[serde(default)]
    pub limit: Option<usize>,
}

pub trait BackgroundAgentStore: Send + Sync {
    fn create_background_agent(&self, request: BackgroundAgentCreateRequest) -> Result<Value>;
    fn update_background_agent(&self, request: BackgroundAgentUpdateRequest) -> Result<Value>;
    fn delete_background_agent(&self, id: &str) -> Result<Value>;
    fn list_background_agents(&self, status: Option<String>) -> Result<Value>;
    fn control_background_agent(&self, request: BackgroundAgentControlRequest) -> Result<Value>;
    fn get_background_agent_progress(
        &self,
        request: BackgroundAgentProgressRequest,
    ) -> Result<Value>;
    fn send_background_agent_message(
        &self,
        request: BackgroundAgentMessageRequest,
    ) -> Result<Value>;
    fn list_background_agent_messages(
        &self,
        request: BackgroundAgentMessageListRequest,
    ) -> Result<Value>;
}

#[derive(Clone)]
pub struct BackgroundAgentTool {
    store: Arc<dyn BackgroundAgentStore>,
    allow_write: bool,
}

impl BackgroundAgentTool {
    pub fn new(store: Arc<dyn BackgroundAgentStore>) -> Self {
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
                "Write access to background agents is disabled. Available read-only operations: list, get, progress. To modify background agents, the user must grant write permissions.".to_string(),
            ))
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum BackgroundAgentAction {
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
        timeout_secs: Option<u64>,
        #[serde(default)]
        durability_mode: Option<String>,
        #[serde(default)]
        memory: Option<Value>,
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
        timeout_secs: Option<u64>,
        #[serde(default)]
        durability_mode: Option<String>,
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
impl Tool for BackgroundAgentTool {
    fn name(&self) -> &str {
        "manage_background_agents"
    }

    fn description(&self) -> &str {
        "Manage background agents with explicit operations: create, update, delete, list, control, progress, send_message, list_messages, pause, resume, cancel, and run."
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
                    "description": "Background agent operation to perform"
                },
                "id": {
                    "type": "string"
                },
                "name": {
                    "type": "string",
                    "description": "Background agent name (for create/update)"
                },
                "agent_id": {
                    "type": "string",
                    "description": "Agent ID (for create/update)"
                },
                "description": {
                    "type": "string",
                    "description": "Background agent description (for update)"
                },
                "schedule": {
                    "type": "object",
                    "description": "Background agent schedule object (for create/update)"
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
                    "description": "Memory configuration payload (for create/update)"
                },
                "timeout_secs": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Optional per-task timeout in seconds for API execution mode (for create/update)"
                },
                "durability_mode": {
                    "type": "string",
                    "enum": ["sync", "async", "exit"],
                    "description": "Checkpoint durability mode (for create/update)"
                },
                "input": {
                    "type": "string",
                    "description": "Optional input for the background agent (for create/update)"
                },
                "input_template": {
                    "type": "string",
                    "description": "Optional runtime template for background agent input (for create/update)"
                },
                "memory_scope": {
                    "type": "string",
                    "enum": ["shared_agent", "per_background_agent"],
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
        let action: BackgroundAgentAction = match serde_json::from_value(input) {
            Ok(action) => action,
            Err(e) => {
                return Ok(ToolOutput::error(format!(
                    "Invalid input: {e}. Required: operation (create|list|get|update|delete|progress)."
                )));
            }
        };

        let output = match action {
            BackgroundAgentAction::List { status } => {
                let result = self
                    .store
                    .list_background_agents(status)
                    .map_err(|e| AiError::Tool(format!("Failed to list background agent: {e}.")))?;
                ToolOutput::success(result)
            }
            BackgroundAgentAction::Create {
                name,
                agent_id,
                schedule,
                input,
                input_template,
                timeout_secs,
                durability_mode,
                memory,
                memory_scope,
            } => {
                self.write_guard()?;
                let result = self
                    .store
                    .create_background_agent(BackgroundAgentCreateRequest {
                        name,
                        agent_id,
                        schedule,
                        input,
                        input_template,
                        timeout_secs,
                        durability_mode,
                        memory,
                        memory_scope,
                    })
                    .map_err(|e| {
                        AiError::Tool(format!("Failed to create background agent: {e}."))
                    })?;
                ToolOutput::success(result)
            }
            BackgroundAgentAction::Update {
                id,
                name,
                description,
                agent_id,
                input,
                input_template,
                schedule,
                notification,
                execution_mode,
                timeout_secs,
                durability_mode,
                memory,
                memory_scope,
            } => {
                self.write_guard()?;
                let result = self
                    .store
                    .update_background_agent(BackgroundAgentUpdateRequest {
                        id,
                        name,
                        description,
                        agent_id,
                        input,
                        input_template,
                        schedule,
                        notification,
                        execution_mode,
                        timeout_secs,
                        durability_mode,
                        memory,
                        memory_scope,
                    })
                    .map_err(|e| {
                        AiError::Tool(format!("Failed to update background agent: {e}."))
                    })?;
                ToolOutput::success(result)
            }
            BackgroundAgentAction::Delete { id } => {
                self.write_guard()?;
                ToolOutput::success(self.store.delete_background_agent(&id).map_err(|e| {
                    AiError::Tool(format!("Failed to delete background agent: {e}."))
                })?)
            }
            BackgroundAgentAction::Pause { id } => {
                self.write_guard()?;
                ToolOutput::success(
                    self.store
                        .control_background_agent(BackgroundAgentControlRequest {
                            id,
                            action: "pause".to_string(),
                        })
                        .map_err(|e| {
                            AiError::Tool(format!("Failed to pause background agent: {e}."))
                        })?,
                )
            }
            BackgroundAgentAction::Resume { id } => {
                self.write_guard()?;
                ToolOutput::success(
                    self.store
                        .control_background_agent(BackgroundAgentControlRequest {
                            id,
                            action: "resume".to_string(),
                        })
                        .map_err(|e| {
                            AiError::Tool(format!("Failed to resume background agent: {e}."))
                        })?,
                )
            }
            BackgroundAgentAction::Cancel { id } => {
                self.write_guard()?;
                ToolOutput::success(self.store.delete_background_agent(&id).map_err(|e| {
                    AiError::Tool(format!("Failed to cancel background agent: {e}."))
                })?)
            }
            BackgroundAgentAction::Run { id } => {
                self.write_guard()?;
                ToolOutput::success(
                    self.store
                        .control_background_agent(BackgroundAgentControlRequest {
                            id,
                            action: "run_now".to_string(),
                        })
                        .map_err(|e| {
                            AiError::Tool(format!("Failed to run background agent: {e}."))
                        })?,
                )
            }
            BackgroundAgentAction::Control { id, action } => {
                self.write_guard()?;
                ToolOutput::success(
                    self.store
                        .control_background_agent(BackgroundAgentControlRequest { id, action })
                        .map_err(|e| {
                            AiError::Tool(format!("Failed to control background agent: {e}."))
                        })?,
                )
            }
            BackgroundAgentAction::Progress { id, event_limit } => ToolOutput::success(
                self.store
                    .get_background_agent_progress(BackgroundAgentProgressRequest {
                        id,
                        event_limit,
                    })
                    .map_err(|e| AiError::Tool(format!("Failed to get background agent: {e}.")))?,
            ),
            BackgroundAgentAction::SendMessage {
                id,
                message,
                source,
            } => {
                self.write_guard()?;
                ToolOutput::success(
                    self.store
                        .send_background_agent_message(BackgroundAgentMessageRequest {
                            id,
                            message,
                            source,
                        })
                        .map_err(|e| {
                            AiError::Tool(format!("Failed to send message background agent: {e}."))
                        })?,
                )
            }
            BackgroundAgentAction::ListMessages { id, limit } => ToolOutput::success(
                self.store
                    .list_background_agent_messages(BackgroundAgentMessageListRequest { id, limit })
                    .map_err(|e| {
                        AiError::Tool(format!("Failed to list messages background agent: {e}."))
                    })?,
            ),
        };

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockStore;
    struct FailingListStore;

    impl BackgroundAgentStore for MockStore {
        fn create_background_agent(&self, _request: BackgroundAgentCreateRequest) -> Result<Value> {
            Ok(json!({ "id": "task-1" }))
        }

        fn update_background_agent(&self, _request: BackgroundAgentUpdateRequest) -> Result<Value> {
            Ok(json!({ "id": "task-1", "updated": true }))
        }

        fn delete_background_agent(&self, _id: &str) -> Result<Value> {
            Ok(json!({ "deleted": true }))
        }

        fn list_background_agents(&self, _status: Option<String>) -> Result<Value> {
            Ok(json!([{"id": "task-1"}]))
        }

        fn control_background_agent(
            &self,
            request: BackgroundAgentControlRequest,
        ) -> Result<Value> {
            Ok(json!({ "id": request.id, "action": request.action }))
        }

        fn get_background_agent_progress(
            &self,
            request: BackgroundAgentProgressRequest,
        ) -> Result<Value> {
            Ok(json!({
                "id": request.id,
                "event_limit": request.event_limit.unwrap_or(10),
                "status": "active"
            }))
        }

        fn send_background_agent_message(
            &self,
            request: BackgroundAgentMessageRequest,
        ) -> Result<Value> {
            Ok(json!({
                "id": request.id,
                "message": request.message,
                "source": request.source.unwrap_or_else(|| "user".to_string())
            }))
        }

        fn list_background_agent_messages(
            &self,
            request: BackgroundAgentMessageListRequest,
        ) -> Result<Value> {
            Ok(json!([{
                "id": "msg-1",
                "task_id": request.id,
                "limit": request.limit.unwrap_or(50)
            }]))
        }
    }

    impl BackgroundAgentStore for FailingListStore {
        fn create_background_agent(&self, _request: BackgroundAgentCreateRequest) -> Result<Value> {
            Ok(json!({ "id": "task-1" }))
        }

        fn update_background_agent(&self, _request: BackgroundAgentUpdateRequest) -> Result<Value> {
            Ok(json!({ "id": "task-1", "updated": true }))
        }

        fn delete_background_agent(&self, _id: &str) -> Result<Value> {
            Ok(json!({ "deleted": true }))
        }

        fn list_background_agents(&self, _status: Option<String>) -> Result<Value> {
            Err(AiError::Tool("store offline".to_string()))
        }

        fn control_background_agent(
            &self,
            request: BackgroundAgentControlRequest,
        ) -> Result<Value> {
            Ok(json!({ "id": request.id, "action": request.action }))
        }

        fn get_background_agent_progress(
            &self,
            request: BackgroundAgentProgressRequest,
        ) -> Result<Value> {
            Ok(json!({
                "id": request.id,
                "event_limit": request.event_limit.unwrap_or(10),
                "status": "active"
            }))
        }

        fn send_background_agent_message(
            &self,
            request: BackgroundAgentMessageRequest,
        ) -> Result<Value> {
            Ok(json!({
                "id": request.id,
                "message": request.message,
                "source": request.source.unwrap_or_else(|| "user".to_string())
            }))
        }

        fn list_background_agent_messages(
            &self,
            request: BackgroundAgentMessageListRequest,
        ) -> Result<Value> {
            Ok(json!([{
                "id": "msg-1",
                "task_id": request.id,
                "limit": request.limit.unwrap_or(50)
            }]))
        }
    }

    #[tokio::test]
    async fn test_list_tasks() {
        let tool = BackgroundAgentTool::new(Arc::new(MockStore));
        let output = tool.execute(json!({ "operation": "list" })).await.unwrap();
        assert!(output.success);
    }

    #[tokio::test]
    async fn test_write_guard() {
        let tool = BackgroundAgentTool::new(Arc::new(MockStore));
        let result = tool
            .execute(json!({
                "operation": "create",
                "name": "A",
                "agent_id": "agent-1"
            }))
            .await;
        let err = result.expect_err("expected write-guard error");
        assert!(
            err.to_string()
                .contains("Available read-only operations: list, get, progress")
        );
    }

    #[tokio::test]
    async fn test_invalid_input_message() {
        let tool = BackgroundAgentTool::new(Arc::new(MockStore));
        let output = tool
            .execute(json!({
                "id": "task-1"
            }))
            .await
            .expect("tool should return error output");
        assert!(!output.success);
        assert!(
            output
                .error
                .expect("expected error")
                .contains("Required: operation (create|list|get|update|delete|progress)")
        );
    }

    #[tokio::test]
    async fn test_list_store_error_is_wrapped() {
        let tool = BackgroundAgentTool::new(Arc::new(FailingListStore));
        let result = tool.execute(json!({ "operation": "list" })).await;
        let err = result.expect_err("expected wrapped store error");
        let err_text = err.to_string();
        assert!(err_text.contains("Failed to list background agent"));
        assert!(err_text.contains("store offline"));
    }

    #[tokio::test]
    async fn test_progress_operation() {
        let tool = BackgroundAgentTool::new(Arc::new(MockStore));
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
