//! Agent CRUD tool for managing stored agents.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use super::traits::{Tool, ToolOutput};
use crate::error::{AiError, Result};

#[derive(Clone, Debug, Deserialize)]
pub struct AgentCreateRequest {
    pub name: String,
    pub agent: Value,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AgentUpdateRequest {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub agent: Option<Value>,
}

pub trait AgentStore: Send + Sync {
    fn list_agents(&self) -> Result<Value>;
    fn get_agent(&self, id: &str) -> Result<Value>;
    fn create_agent(&self, request: AgentCreateRequest) -> Result<Value>;
    fn update_agent(&self, request: AgentUpdateRequest) -> Result<Value>;
    fn delete_agent(&self, id: &str) -> Result<Value>;
}

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
            Err(AiError::Tool(
                "Write access to agents is disabled for this tool".to_string(),
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
            AgentAction::List => ToolOutput::success(self.store.list_agents()?),
            AgentAction::Show { id } => ToolOutput::success(self.store.get_agent(&id)?),
            AgentAction::Create { name, agent } => {
                self.write_guard()?;
                let request = AgentCreateRequest { name, agent };
                ToolOutput::success(self.store.create_agent(request)?)
            }
            AgentAction::Update { id, name, agent } => {
                self.write_guard()?;
                let request = AgentUpdateRequest { id, name, agent };
                ToolOutput::success(self.store.update_agent(request)?)
            }
            AgentAction::Delete { id } => {
                self.write_guard()?;
                ToolOutput::success(self.store.delete_agent(&id)?)
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
        assert!(result.is_err());
    }
}
