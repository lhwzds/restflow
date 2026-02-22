//! Authentication profile management tool.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::{Tool, ToolOutput};
use restflow_ai::error::AiError;
use restflow_traits::store::{AuthProfileStore, CredentialInput, AuthProfileCreateRequest, AuthProfileTestRequest};
use crate::Result;

#[derive(Clone)]
pub struct AuthProfileTool {
    store: Arc<dyn AuthProfileStore>,
    allow_write: bool,
}

impl AuthProfileTool {
    pub fn new(store: Arc<dyn AuthProfileStore>) -> Self {
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
                "Write access to auth profiles is disabled. Available read-only operations: list, get. To modify auth profiles, the user must grant write permissions.".to_string(),
            ))
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum AuthProfileAction {
    List,
    Discover,
    Add {
        name: String,
        provider: String,
        #[serde(default)]
        source: Option<String>,
        credential: CredentialInput,
    },
    Remove {
        id: String,
    },
    Test {
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        provider: Option<String>,
    },
}

#[async_trait]
impl Tool for AuthProfileTool {
    fn name(&self) -> &str {
        "manage_auth_profiles"
    }

    fn description(&self) -> &str {
        "Discover, create, test, list, and remove authentication profiles for model providers."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["list", "discover", "add", "remove", "test"],
                    "description": "Auth profile operation to perform"
                },
                "id": {
                    "type": "string",
                    "description": "Profile ID (for remove/test)"
                },
                "provider": {
                    "type": "string",
                    "description": "Provider name (for add/test)"
                },
                "source": {
                    "type": "string",
                    "description": "Credential source (for add)"
                },
                "name": {
                    "type": "string",
                    "description": "Profile name (for add)"
                },
                "credential": {
                    "type": "object",
                    "description": "Credential payload (for add)"
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let action: AuthProfileAction = serde_json::from_value(input)?;

        let output =
            match action {
                AuthProfileAction::List => ToolOutput::success(
                    self.store
                        .list_profiles()
                        .map_err(|e| AiError::Tool(format!("Failed to list auth profile: {e}")))?,
                ),
                AuthProfileAction::Discover => {
                    ToolOutput::success(self.store.discover_profiles().map_err(|e| {
                        AiError::Tool(format!("Failed to discover auth profile: {e}"))
                    })?)
                }
                AuthProfileAction::Add {
                    name,
                    provider,
                    source,
                    credential,
                } => {
                    self.write_guard()?;
                    let request = AuthProfileCreateRequest {
                        name,
                        provider,
                        source,
                        credential,
                    };
                    ToolOutput::success(
                        self.store.add_profile(request).map_err(|e| {
                            AiError::Tool(format!("Failed to add auth profile: {e}"))
                        })?,
                    )
                }
                AuthProfileAction::Remove { id } => {
                    self.write_guard()?;
                    ToolOutput::success(self.store.remove_profile(&id).map_err(|e| {
                        AiError::Tool(format!("Failed to remove auth profile: {e}"))
                    })?)
                }
                AuthProfileAction::Test { id, provider } => {
                    let request = AuthProfileTestRequest { id, provider };
                    ToolOutput::success(
                        self.store.test_profile(request).map_err(|e| {
                            AiError::Tool(format!("Failed to test auth profile: {e}"))
                        })?,
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

    impl AuthProfileStore for MockStore {
        fn list_profiles(&self) -> Result<Value> {
            Ok(json!([{"id": "profile"}]))
        }

        fn discover_profiles(&self) -> Result<Value> {
            Ok(json!({"total": 1}))
        }

        fn add_profile(&self, _request: AuthProfileCreateRequest) -> Result<Value> {
            Ok(json!({"id": "profile"}))
        }

        fn remove_profile(&self, _id: &str) -> Result<Value> {
            Ok(json!({"removed": true}))
        }

        fn test_profile(&self, _request: AuthProfileTestRequest) -> Result<Value> {
            Ok(json!({"available": true}))
        }
    }

    #[tokio::test]
    async fn test_list_profiles() {
        let tool = AuthProfileTool::new(Arc::new(MockStore));
        let output = tool.execute(json!({"operation": "list"})).await.unwrap();
        assert!(output.success);
    }

    #[tokio::test]
    async fn test_add_requires_write() {
        let tool = AuthProfileTool::new(Arc::new(MockStore));
        let result = tool
            .execute(json!({
                "operation": "add",
                "name": "Profile",
                "provider": "anthropic",
                "credential": {"type": "api_key", "key": "secret"}
            }))
            .await;
        let err = result.expect_err("expected write-guard error");
        assert!(
            err.to_string()
                .contains("Available read-only operations: list, get")
        );
    }
}
