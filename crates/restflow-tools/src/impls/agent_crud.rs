//! Agent CRUD tool for managing stored agents.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::{Tool, ToolError, ToolOutput};
use restflow_traits::store::{AgentStore, AgentCreateRequest, AgentUpdateRequest};
use crate::Result;

#[derive(Clone)]
pub struct AgentCrudTool {
    store: Arc<dyn AgentStore>,
    allow_write: bool,
}

impl AgentCrudTool {
    pub fn new(store: Arc<dyn AgentStore>) -> Self {
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
            Err(crate::ToolError::Tool(
                "Write access to agents is disabled. Available read-only operations: list, get. To modify agents, the user must grant write permissions.".to_string(),
            ))
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum AgentAction {
    List,
    Show {
        id: String,
    },
    Create {
        name: String,
        agent: Value,
    },
    Update {
        id: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        agent: Option<Value>,
    },
    Delete {
        id: String,
    },
}

#[async_trait]
impl Tool for AgentCrudTool {
    fn name(&self) -> &str {
        "manage_agents"
    }

    fn description(&self) -> &str {
        "Create, read, update, list, and delete agent definitions and configuration."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["list", "show", "create", "update", "delete"],
                    "description": "Agent operation to perform"
                },
                "id": {
                    "type": "string",
                    "description": "Agent ID (for show/update/delete)"
                },
                "name": {
                    "type": "string",
                    "description": "Agent name (for create/update)"
                },
                "agent": {
                    "type": "object",
                    "description": "Agent configuration (for create/update)"
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let action: AgentAction = serde_json::from_value(input)?;

        let output = match action {
            AgentAction::List => ToolOutput::success(
                self.store
                    .list_agents()
                    .map_err(|e| ToolError::Tool(format!("Failed to list agent: {e}")))?,
            ),
            AgentAction::Show { id } => ToolOutput::success(
                self.store
                    .get_agent(&id)
                    .map_err(|e| ToolError::Tool(format!("Failed to get agent: {e}")))?,
            ),
            AgentAction::Create { name, agent } => {
                self.write_guard()?;
                let request = AgentCreateRequest { name, agent };
                ToolOutput::success(self.store.create_agent(request).map_err(|e| {
                    let message = e.to_string();
                    if message.contains("\"type\":\"validation_error\"") {
                        ToolError::Tool(message)
                    } else {
                        ToolError::Tool(format!("Failed to create agent: {e}"))
                    }
                })?)
            }
            AgentAction::Update { id, name, agent } => {
                self.write_guard()?;
                let request = AgentUpdateRequest { id, name, agent };
                ToolOutput::success(self.store.update_agent(request).map_err(|e| {
                    let message = e.to_string();
                    if message.contains("\"type\":\"validation_error\"") {
                        ToolError::Tool(message)
                    } else {
                        ToolError::Tool(format!("Failed to update agent: {e}"))
                    }
                })?)
            }
            AgentAction::Delete { id } => {
                self.write_guard()?;
                ToolOutput::success(
                    self.store
                        .delete_agent(&id)
                        .map_err(|e| ToolError::Tool(format!("Failed to delete agent: {e}")))?,
                )
            }
        };

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockStore;

    impl AgentStore for MockStore {
        fn list_agents(&self) -> Result<Value> {
            Ok(json!([{"id": "agent-1"}]))
        }

        fn get_agent(&self, _id: &str) -> Result<Value> {
            Ok(json!({"id": "agent-1"}))
        }

        fn create_agent(&self, _request: AgentCreateRequest) -> Result<Value> {
            Ok(json!({"id": "agent-1"}))
        }

        fn update_agent(&self, _request: AgentUpdateRequest) -> Result<Value> {
            Ok(json!({"id": "agent-1"}))
        }

        fn delete_agent(&self, _id: &str) -> Result<Value> {
            Ok(json!({"deleted": true}))
        }
    }

    #[tokio::test]
    async fn test_list_agents() {
        let tool = AgentCrudTool::new(Arc::new(MockStore));
        let output = tool.execute(json!({"operation": "list"})).await.unwrap();
        assert!(output.success);
    }

    #[tokio::test]
    async fn test_create_requires_write() {
        let tool = AgentCrudTool::new(Arc::new(MockStore));
        let result = tool
            .execute(json!({"operation": "create", "name": "Agent", "agent": {}}))
            .await;
        let err = result.expect_err("expected write-guard error");
        assert!(
            err.to_string()
                .contains("Available read-only operations: list, get")
        );
    }

    struct ValidationStore;

    impl AgentStore for ValidationStore {
        fn list_agents(&self) -> Result<Value> {
            Ok(json!([]))
        }

        fn get_agent(&self, _id: &str) -> Result<Value> {
            Ok(json!({}))
        }

        fn create_agent(&self, _request: AgentCreateRequest) -> Result<Value> {
            Err(crate::ToolError::Tool(
                "{\"type\":\"validation_error\",\"errors\":[{\"field\":\"temperature\",\"message\":\"invalid\"}]}".to_string(),
            ))
        }

        fn update_agent(&self, _request: AgentUpdateRequest) -> Result<Value> {
            Err(crate::ToolError::Tool(
                "{\"type\":\"validation_error\",\"errors\":[{\"field\":\"tools\",\"message\":\"unknown\"}]}".to_string(),
            ))
        }

        fn delete_agent(&self, _id: &str) -> Result<Value> {
            Ok(json!({}))
        }
    }

    #[tokio::test]
    async fn test_create_propagates_validation_payload_without_wrapping() {
        let tool = AgentCrudTool::new(Arc::new(ValidationStore)).with_write(true);
        let err = tool
            .execute(json!({"operation": "create", "name": "Agent", "agent": {}}))
            .await
            .expect_err("expected validation error");
        assert!(err.to_string().contains("\"type\":\"validation_error\""));
    }
}
