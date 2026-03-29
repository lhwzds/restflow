//! Secrets management tool for AI agents.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use restflow_traits::store::SecretStore;

use crate::Result;
use crate::{Tool, ToolError, ToolOutput};

#[derive(Clone)]
pub struct SecretsTool {
    store: Arc<dyn SecretStore>,
    allow_write: bool,
    get_policy: SecretGetPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SecretGetPolicy {
    #[default]
    Open,
    MetadataOnly,
    Deny,
}

impl SecretsTool {
    pub fn new(store: Arc<dyn SecretStore>) -> Self {
        Self {
            store,
            allow_write: false,
            get_policy: SecretGetPolicy::Open,
        }
    }

    pub fn with_write(mut self, allow_write: bool) -> Self {
        self.allow_write = allow_write;
        self
    }

    pub fn with_get_policy(mut self, get_policy: SecretGetPolicy) -> Self {
        self.get_policy = get_policy;
        self
    }

    fn write_guard(&self) -> Result<()> {
        if self.allow_write {
            Ok(())
        } else {
            Err(crate::ToolError::Tool(
                "Write access to secrets is disabled. Available read-only operations: list, has. To modify secrets, the user must grant write permissions.".to_string(),
            ))
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum SecretsAction {
    List,
    Get {
        key: String,
    },
    Set {
        key: String,
        value: String,
        #[serde(default)]
        description: Option<String>,
    },
    Delete {
        key: String,
    },
    Has {
        key: String,
    },
}

#[async_trait]
impl Tool for SecretsTool {
    fn name(&self) -> &str {
        "manage_secrets"
    }

    fn description(&self) -> &str {
        "List, read, set, delete, and existence-check named secrets in secure storage."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["list", "get", "set", "delete", "has"],
                    "description": "Secret operation to perform"
                },
                "key": {
                    "type": "string",
                    "description": "Secret key (for get/set/delete/has)"
                },
                "value": {
                    "type": "string",
                    "description": "Secret value (for set)"
                },
                "description": {
                    "type": "string",
                    "description": "Optional description (for set)"
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let action: SecretsAction = serde_json::from_value(input)?;

        let output = match action {
            SecretsAction::List => {
                let result = self.store.list_secrets()?;
                ToolOutput::success(result)
            }
            SecretsAction::Get { key } => {
                let value = self.store.get_secret(&key)?;
                match self.get_policy {
                    SecretGetPolicy::Open => ToolOutput::success(json!({
                        "key": key,
                        "found": value.is_some(),
                        "value": value
                    })),
                    SecretGetPolicy::MetadataOnly => ToolOutput::success(json!({
                        "key": key,
                        "found": value.is_some(),
                        "value": Value::Null
                    })),
                    SecretGetPolicy::Deny => {
                        return Err(ToolError::Tool(
                            "Reading secret values is disabled by policy. Use list/has for non-sensitive checks."
                                .to_string(),
                        ));
                    }
                }
            }
            SecretsAction::Set {
                key,
                value,
                description,
            } => {
                self.write_guard()?;
                let existed = self.store.has_secret(&key)?;
                self.store.set_secret(&key, &value, description)?;
                ToolOutput::success(json!({
                    "key": key,
                    "updated": existed,
                    "created": !existed
                }))
            }
            SecretsAction::Delete { key } => {
                self.write_guard()?;
                let existed = self.store.has_secret(&key)?;
                if existed {
                    self.store.delete_secret(&key)?;
                }
                ToolOutput::success(json!({ "key": key, "deleted": existed }))
            }
            SecretsAction::Has { key } => {
                let exists = self.store.has_secret(&key)?;
                ToolOutput::success(json!({ "key": key, "exists": exists }))
            }
        };

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_traits::error::Result as TraitResult;

    #[derive(Clone)]
    struct MockSecretStore {
        secrets: Arc<parking_lot::RwLock<std::collections::HashMap<String, String>>>,
    }

    impl MockSecretStore {
        fn new() -> Self {
            Self {
                secrets: Arc::new(parking_lot::RwLock::new(std::collections::HashMap::new())),
            }
        }
    }

    impl SecretStore for MockSecretStore {
        fn list_secrets(&self) -> TraitResult<Value> {
            let map = self.secrets.read();
            let secrets: Vec<Value> = map
                .keys()
                .map(|k| json!({ "key": k, "value": "", "description": null }))
                .collect();
            Ok(json!({ "count": secrets.len(), "secrets": secrets }))
        }

        fn get_secret(&self, key: &str) -> TraitResult<Option<String>> {
            let map = self.secrets.read();
            Ok(map.get(key).cloned())
        }

        fn set_secret(
            &self,
            key: &str,
            value: &str,
            _description: Option<String>,
        ) -> TraitResult<()> {
            self.secrets
                .write()
                .insert(key.to_string(), value.to_string());
            Ok(())
        }

        fn delete_secret(&self, key: &str) -> TraitResult<()> {
            self.secrets.write().remove(key);
            Ok(())
        }

        fn has_secret(&self, key: &str) -> TraitResult<bool> {
            Ok(self.secrets.read().contains_key(key))
        }
    }

    fn setup_store() -> Arc<dyn SecretStore> {
        Arc::new(MockSecretStore::new())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_list_and_get_secret() {
        let store = setup_store();
        store
            .set_secret("TEST_KEY", "value", Some("desc".to_string()))
            .unwrap();

        let tool = SecretsTool::new(store).with_write(true);
        let output = tool
            .execute(json!({ "operation": "get", "key": "TEST_KEY" }))
            .await
            .unwrap();

        assert!(output.success);
        assert_eq!(output.result["value"], "value");

        let list_output = tool.execute(json!({ "operation": "list" })).await.unwrap();
        assert!(list_output.success);
        assert_eq!(list_output.result["count"], 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_write_guard() {
        let store = setup_store();
        let tool = SecretsTool::new(store);
        let result = tool
            .execute(json!({ "operation": "set", "key": "A", "value": "B" }))
            .await;
        let err = result.expect_err("expected write-guard error");
        assert!(
            err.to_string()
                .contains("Available read-only operations: list, has")
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_get_metadata_only_policy_redacts_value() {
        let store = setup_store();
        store
            .set_secret("TEST_KEY", "value", Some("desc".to_string()))
            .unwrap();

        let tool = SecretsTool::new(store).with_get_policy(SecretGetPolicy::MetadataOnly);
        let output = tool
            .execute(json!({ "operation": "get", "key": "TEST_KEY" }))
            .await
            .unwrap();

        assert!(output.success);
        assert_eq!(output.result["found"], true);
        assert_eq!(output.result["value"], Value::Null);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_get_deny_policy_blocks_read() {
        let store = setup_store();
        store
            .set_secret("TEST_KEY", "value", Some("desc".to_string()))
            .unwrap();

        let tool = SecretsTool::new(store).with_get_policy(SecretGetPolicy::Deny);
        let result = tool
            .execute(json!({ "operation": "get", "key": "TEST_KEY" }))
            .await;
        let err = result.expect_err("expected get policy deny error");
        assert!(
            err.to_string()
                .contains("Reading secret values is disabled by policy")
        );
    }
}
