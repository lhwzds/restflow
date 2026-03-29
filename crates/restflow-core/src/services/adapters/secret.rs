use restflow_storage::SecretStorage;
use restflow_traits::store::SecretStore;
use serde_json::{Value, json};
use std::sync::Arc;

pub struct SecretStoreAdapter {
    storage: Arc<SecretStorage>,
}

impl SecretStoreAdapter {
    pub fn new(storage: Arc<SecretStorage>) -> Self {
        Self { storage }
    }
}

impl SecretStore for SecretStoreAdapter {
    fn list_secrets(&self) -> restflow_traits::error::Result<Value> {
        let secrets = self.storage.list_secrets().map_err(|e| {
            restflow_traits::ToolError::Tool(format!("Failed to list secrets: {e}"))
        })?;
        Ok(json!({ "count": secrets.len(), "secrets": secrets }))
    }

    fn get_secret(&self, key: &str) -> restflow_traits::error::Result<Option<String>> {
        self.storage
            .get_secret(key)
            .map_err(|e| restflow_traits::ToolError::Tool(format!("Failed to get secret: {e}")))
    }

    fn set_secret(
        &self,
        key: &str,
        value: &str,
        description: Option<String>,
    ) -> restflow_traits::error::Result<()> {
        self.storage
            .set_secret(key, value, description)
            .map_err(|e| restflow_traits::ToolError::Tool(format!("Failed to set secret: {e}")))
    }

    fn delete_secret(&self, key: &str) -> restflow_traits::error::Result<()> {
        self.storage
            .delete_secret(key)
            .map_err(|e| restflow_traits::ToolError::Tool(format!("Failed to delete secret: {e}")))
    }

    fn has_secret(&self, key: &str) -> restflow_traits::error::Result<bool> {
        self.storage
            .has_secret(key)
            .map_err(|e| restflow_traits::ToolError::Tool(format!("Failed to check secret: {e}")))
    }
}
