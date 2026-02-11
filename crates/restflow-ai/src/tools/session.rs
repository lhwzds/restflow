//! Chat session management tool.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use super::traits::{Tool, ToolOutput};
use crate::error::{AiError, Result};

#[derive(Clone, Debug, Deserialize)]
pub struct SessionCreateRequest {
    pub agent_id: String,
    pub model: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub skill_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SessionSearchQuery {
    pub query: String,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub skill_id: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SessionListFilter {
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub skill_id: Option<String>,
    #[serde(default)]
    pub include_messages: Option<bool>,
}

pub trait SessionStore: Send + Sync {
    fn list_sessions(&self, filter: SessionListFilter) -> Result<Value>;
    fn get_session(&self, id: &str) -> Result<Value>;
    fn create_session(&self, request: SessionCreateRequest) -> Result<Value>;
    fn delete_session(&self, id: &str) -> Result<Value>;
    fn search_sessions(&self, query: SessionSearchQuery) -> Result<Value>;
}

#[derive(Clone)]
pub struct SessionTool {
    store: Arc<dyn SessionStore>,
    allow_write: bool,
}

impl SessionTool {
    pub fn new(store: Arc<dyn SessionStore>) -> Self {
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
                "Write access to sessions is disabled. Available read-only operations: list, get, history. To modify sessions, the user must grant write permissions.".to_string(),
            ))
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum SessionAction {
    List {
        #[serde(default)]
        agent_id: Option<String>,
        #[serde(default)]
        skill_id: Option<String>,
        #[serde(default)]
        include_messages: Option<bool>,
    },
    Get {
        id: String,
    },
    Create {
        agent_id: String,
        model: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        skill_id: Option<String>,
    },
    Delete {
        id: String,
    },
    Search {
        query: String,
        #[serde(default)]
        agent_id: Option<String>,
        #[serde(default)]
        skill_id: Option<String>,
        #[serde(default)]
        limit: Option<u32>,
    },
}

#[async_trait]
impl Tool for SessionTool {
    fn name(&self) -> &str {
        "manage_sessions"
    }

    fn description(&self) -> &str {
        "Create, list, fetch, search, and delete chat sessions."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["list", "get", "create", "delete", "search"],
                    "description": "Session operation to perform"
                },
                "id": {
                    "type": "string",
                    "description": "Session ID (for get/delete)"
                },
                "agent_id": {
                    "type": "string",
                    "description": "Agent ID filter (for list/search) or agent ID for create"
                },
                "skill_id": {
                    "type": "string",
                    "description": "Optional skill ID filter (for list/search/create)"
                },
                "include_messages": {
                    "type": "boolean",
                    "description": "Include full messages in list results",
                    "default": false
                },
                "model": {
                    "type": "string",
                    "description": "Model name (for create)"
                },
                "name": {
                    "type": "string",
                    "description": "Optional session name (for create)"
                },
                "query": {
                    "type": "string",
                    "description": "Search query (for search)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max results (for search)",
                    "minimum": 1
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let action: SessionAction = serde_json::from_value(input)?;

        let output = match action {
            SessionAction::List {
                agent_id,
                skill_id,
                include_messages,
            } => {
                let filter = SessionListFilter {
                    agent_id,
                    skill_id,
                    include_messages,
                };
                ToolOutput::success(
                    self.store
                        .list_sessions(filter)
                        .map_err(|e| AiError::Tool(format!("Failed to list session: {e}")))?,
                )
            }
            SessionAction::Get { id } => ToolOutput::success(
                self.store
                    .get_session(&id)
                    .map_err(|e| AiError::Tool(format!("Failed to get session: {e}")))?,
            ),
            SessionAction::Create {
                agent_id,
                model,
                name,
                skill_id,
            } => {
                self.write_guard()?;
                let request = SessionCreateRequest {
                    agent_id,
                    model,
                    name,
                    skill_id,
                };
                ToolOutput::success(
                    self.store
                        .create_session(request)
                        .map_err(|e| AiError::Tool(format!("Failed to create session: {e}")))?,
                )
            }
            SessionAction::Delete { id } => {
                self.write_guard()?;
                ToolOutput::success(
                    self.store
                        .delete_session(&id)
                        .map_err(|e| AiError::Tool(format!("Failed to delete session: {e}")))?,
                )
            }
            SessionAction::Search {
                query,
                agent_id,
                skill_id,
                limit,
            } => {
                let request = SessionSearchQuery {
                    query,
                    agent_id,
                    skill_id,
                    limit,
                };
                ToolOutput::success(
                    self.store
                        .search_sessions(request)
                        .map_err(|e| AiError::Tool(format!("Failed to search session: {e}")))?,
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

    impl SessionStore for MockStore {
        fn list_sessions(&self, _filter: SessionListFilter) -> Result<Value> {
            Ok(json!([{"id": "session-1"}]))
        }

        fn get_session(&self, _id: &str) -> Result<Value> {
            Ok(json!({"id": "session-1"}))
        }

        fn create_session(&self, _request: SessionCreateRequest) -> Result<Value> {
            Ok(json!({"id": "session-1"}))
        }

        fn delete_session(&self, _id: &str) -> Result<Value> {
            Ok(json!({"deleted": true}))
        }

        fn search_sessions(&self, _query: SessionSearchQuery) -> Result<Value> {
            Ok(json!([{"id": "session-1"}]))
        }
    }

    #[tokio::test]
    async fn test_list_sessions() {
        let tool = SessionTool::new(Arc::new(MockStore));
        let output = tool.execute(json!({"operation": "list"})).await.unwrap();
        assert!(output.success);
    }

    #[tokio::test]
    async fn test_create_requires_write() {
        let tool = SessionTool::new(Arc::new(MockStore));
        let result = tool
            .execute(json!({"operation": "create", "agent_id": "agent", "model": "gpt"}))
            .await;
        let err = result.err().expect("expected write-guard error");
        assert!(
            err.to_string()
                .contains("Available read-only operations: list, get, history")
        );
    }
}
