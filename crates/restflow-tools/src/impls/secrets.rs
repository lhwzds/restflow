//! Secrets management tool for AI agents.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use restflow_storage::{Secret, SecretStorage};

use crate::tool::{Tool, ToolOutput};
use restflow_ai::error::AiError;
use crate::error::Result;

#[derive(Clone)]
pub struct SecretsTool {
    storage: Arc<SecretStorage>,
    allow_write: bool,
}

impl SecretsTool {
    pub fn new(storage: Arc<SecretStorage>) -> Self {
        Self {
            storage,
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
            Err(crate::error::ToolError::Tool(
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
                let secrets: Vec<Secret> = self
                    .storage
                    .list_secrets()
                    .map_err(|e| AiError::Tool(format!("Failed to list secret: {e}")))?;
                ToolOutput::success(json!({ "count": secrets.len(), "secrets": secrets }))
            }
            SecretsAction::Get { key } => {
                let value = self
                    .storage
                    .get_secret(&key)
                    .map_err(|e| AiError::Tool(format!("Failed to get secret: {e}")))?;
                ToolOutput::success(json!({
                    "key": key,
                    "found": value.is_some(),
                    "value": value
                }))
            }
            SecretsAction::Set {
                key,
                value,
                description,
            } => {
                self.write_guard()?;
                let existed = self
                    .storage
                    .has_secret(&key)
                    .map_err(|e| AiError::Tool(format!("Failed to set secret: {e}")))?;
                self.storage
                    .set_secret(&key, &value, description)
                    .map_err(|e| AiError::Tool(format!("Failed to set secret: {e}")))?;
                ToolOutput::success(json!({
                    "key": key,
                    "updated": existed,
                    "created": !existed
                }))
            }
            SecretsAction::Delete { key } => {
                self.write_guard()?;
                let existed = self
                    .storage
                    .has_secret(&key)
                    .map_err(|e| AiError::Tool(format!("Failed to delete secret: {e}")))?;
                if existed {
                    self.storage
                        .delete_secret(&key)
                        .map_err(|e| AiError::Tool(format!("Failed to delete secret: {e}")))?;
                }
                ToolOutput::success(json!({ "key": key, "deleted": existed }))
            }
            SecretsAction::Has { key } => {
                let exists = self
                    .storage
                    .has_secret(&key)
                    .map_err(|e| AiError::Tool(format!("Failed to check secret: {e}")))?;
                ToolOutput::success(json!({ "key": key, "exists": exists }))
            }
        };

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_storage::SecretStorageConfig;
    use std::sync::Mutex;
    use tempfile::tempdir;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn setup_storage() -> (Arc<SecretStorage>, tempfile::TempDir) {
        let _guard = ENV_LOCK.lock().unwrap();
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(redb::Database::create(db_path).unwrap());

        let state_dir = temp_dir.path().join("state");
        std::fs::create_dir_all(&state_dir).unwrap();
        // SAFETY: env var modified under ENV_LOCK and callers use
        // #[tokio::test(flavor = "current_thread")] so no worker threads race.
        unsafe {
            std::env::set_var("RESTFLOW_DIR", &state_dir);
        }

        let storage = SecretStorage::with_config(
            db,
            SecretStorageConfig {
                allow_insecure_file_permissions: true,
            },
        )
        .unwrap();

        // Keep temp_dir alive so RESTFLOW_DIR remains valid for storage usage
        (Arc::new(storage), temp_dir)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_list_and_get_secret() {
        let (storage, _temp_dir) = setup_storage();
        storage
            .set_secret("TEST_KEY", "value", Some("desc".to_string()))
            .unwrap();

        let tool = SecretsTool::new(storage.clone()).with_write(true);
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
        let (storage, _temp_dir) = setup_storage();
        let tool = SecretsTool::new(storage);
        let result = tool
            .execute(json!({ "operation": "set", "key": "A", "value": "B" }))
            .await;
        let err = result.expect_err("expected write-guard error");
        assert!(
            err.to_string()
                .contains("Available read-only operations: list, has")
        );
    }
}
