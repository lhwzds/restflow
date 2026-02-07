//! Authentication profile management tool.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use super::traits::{Tool, ToolOutput};
use crate::error::{AiError, Result};

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CredentialInput {
    ApiKey {
        key: String,
        #[serde(default)]
        email: Option<String>,
    },
    Token {
        token: String,
        #[serde(default)]
        expires_at: Option<String>,
        #[serde(default)]
        email: Option<String>,
    },
    OAuth {
        access_token: String,
        #[serde(default)]
        refresh_token: Option<String>,
        #[serde(default)]
        expires_at: Option<String>,
        #[serde(default)]
        email: Option<String>,
    },
}

#[derive(Clone, Debug, Deserialize)]
pub struct AuthProfileCreateRequest {
    pub name: String,
    pub provider: String,
    #[serde(default)]
    pub source: Option<String>,
    pub credential: CredentialInput,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AuthProfileTestRequest {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
}

pub trait AuthProfileStore: Send + Sync {
    fn list_profiles(&self) -> Result<Value>;
    fn discover_profiles(&self) -> Result<Value>;
    fn add_profile(&self, request: AuthProfileCreateRequest) -> Result<Value>;
    fn remove_profile(&self, id: &str) -> Result<Value>;
    fn test_profile(&self, request: AuthProfileTestRequest) -> Result<Value>;
}

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
            Err(AiError::Tool(
                "Write access to auth profiles is disabled for this tool".to_string(),
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

        let output = match action {
            AuthProfileAction::List => ToolOutput::success(self.store.list_profiles()?),
            AuthProfileAction::Discover => ToolOutput::success(self.store.discover_profiles()?),
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
                ToolOutput::success(self.store.add_profile(request)?)
            }
            AuthProfileAction::Remove { id } => {
                self.write_guard()?;
                ToolOutput::success(self.store.remove_profile(&id)?)
            }
            AuthProfileAction::Test { id, provider } => {
                let request = AuthProfileTestRequest { id, provider };
                ToolOutput::success(self.store.test_profile(request)?)
            }
        };

        Ok(output)
    }
}

fn parse_rfc3339(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| AiError::Tool(format!("Invalid timestamp: {}", e)))
}

impl CredentialInput {
    pub fn expires_at(&self) -> Result<Option<DateTime<Utc>>> {
        match self {
            CredentialInput::Token { expires_at, .. }
            | CredentialInput::OAuth { expires_at, .. } => {
                if let Some(value) = expires_at {
                    Ok(Some(parse_rfc3339(value)?))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
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
        assert!(result.is_err());
    }
}
