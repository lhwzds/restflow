//! AuthProfileStore adapter backed by SecretStorage.

use crate::storage::SecretStorage;
use restflow_ai::tools::{AuthProfileCreateRequest, AuthProfileStore, AuthProfileTestRequest};
use restflow_tools::ToolError;
use serde_json::{Value, json};

#[derive(Clone)]
pub struct AuthProfileStorageAdapter {
    storage: SecretStorage,
}

impl AuthProfileStorageAdapter {
    pub fn new(storage: SecretStorage) -> Self {
        Self { storage }
    }
}

impl AuthProfileStore for AuthProfileStorageAdapter {
    fn list_profiles(&self) -> restflow_tools::Result<Value> {
        let secrets = self
            .storage
            .list_secrets()
            .map_err(|e| ToolError::Tool(e.to_string()))?;

        let profiles: Vec<Value> = secrets
            .iter()
            .filter(|s| s.key.ends_with("_API_KEY") || s.key.ends_with("_TOKEN"))
            .map(|s| {
                let has_value = self.storage.has_secret(&s.key).unwrap_or(false);
                json!({
                    "id": s.key,
                    "name": s.key,
                    "has_credential": has_value,
                    "description": s.description
                })
            })
            .collect();

        Ok(json!(profiles))
    }

    fn discover_profiles(&self) -> restflow_tools::Result<Value> {
        let known_vars = [
            "ANTHROPIC_API_KEY",
            "OPENAI_API_KEY",
            "GEMINI_API_KEY",
            "GROQ_API_KEY",
            "DEEPSEEK_API_KEY",
            "OPENROUTER_API_KEY",
            "XAI_API_KEY",
            "GITHUB_TOKEN",
        ];

        let discovered: Vec<Value> = known_vars
            .iter()
            .filter_map(|var| {
                std::env::var(var).ok().map(|_| {
                    json!({
                        "env_var": var,
                        "available": true
                    })
                })
            })
            .collect();

        Ok(json!({
            "total": discovered.len(),
            "profiles": discovered
        }))
    }

    fn add_profile(&self, request: AuthProfileCreateRequest) -> restflow_tools::Result<Value> {
        let key_name = format!(
            "{}_API_KEY",
            request.provider.to_uppercase().replace('-', "_")
        );
        let secret_value = match &request.credential {
            restflow_ai::tools::CredentialInput::ApiKey { key, .. } => key.clone(),
            restflow_ai::tools::CredentialInput::Token { token, .. } => token.clone(),
            restflow_ai::tools::CredentialInput::OAuth { access_token, .. } => access_token.clone(),
        };
        self.storage
            .set_secret(
                &key_name,
                &secret_value,
                Some(format!("Auth profile: {}", request.name)),
            )
            .map_err(|e| ToolError::Tool(e.to_string()))?;

        Ok(json!({
            "id": key_name,
            "name": request.name,
            "provider": request.provider,
            "created": true
        }))
    }

    fn remove_profile(&self, id: &str) -> restflow_tools::Result<Value> {
        self.storage
            .delete_secret(id)
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        Ok(json!({ "id": id, "removed": true }))
    }

    fn test_profile(&self, request: AuthProfileTestRequest) -> restflow_tools::Result<Value> {
        if let Some(id) = &request.id {
            let available = self
                .storage
                .get_secret(id)
                .ok()
                .flatten()
                .map(|v| !v.is_empty())
                .unwrap_or(false);
            Ok(json!({
                "id": id,
                "available": available
            }))
        } else if let Some(provider) = &request.provider {
            let key_name = format!("{}_API_KEY", provider.to_uppercase().replace('-', "_"));
            let available = self
                .storage
                .get_secret(&key_name)
                .ok()
                .flatten()
                .map(|v| !v.is_empty())
                .unwrap_or(false);
            Ok(json!({
                "provider": provider,
                "key_name": key_name,
                "available": available
            }))
        } else {
            Ok(json!({ "available": false, "reason": "No id or provider specified" }))
        }
    }
}
