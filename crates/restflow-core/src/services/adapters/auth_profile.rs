//! AuthProfileStore adapter backed by SecretStorage.

use crate::storage::SecretStorage;
use restflow_ai::tools::{AuthProfileCreateRequest, AuthProfileStore, AuthProfileTestRequest};
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
            .list_secrets()?;

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
            )?;

        Ok(json!({
            "id": key_name,
            "name": request.name,
            "provider": request.provider,
            "created": true
        }))
    }

    fn remove_profile(&self, id: &str) -> restflow_tools::Result<Value> {
        self.storage
            .delete_secret(id)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_ai::tools::{AuthProfileStore, CredentialInput};
    use std::sync::{Arc, Mutex, OnceLock};
    use tempfile::tempdir;

    /// Guard to serialize tests that modify RESTFLOW_DIR env var.
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn setup() -> (AuthProfileStorageAdapter, tempfile::TempDir, std::sync::MutexGuard<'static, ()>) {
        let guard = env_lock();
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(redb::Database::create(db_path).unwrap());

        let state_dir = temp_dir.path().join("state");
        std::fs::create_dir_all(&state_dir).unwrap();
        let prev_dir = std::env::var_os("RESTFLOW_DIR");
        let prev_key = std::env::var_os("RESTFLOW_MASTER_KEY");
        unsafe {
            std::env::set_var("RESTFLOW_DIR", &state_dir);
            std::env::remove_var("RESTFLOW_MASTER_KEY");
        }

        let storage = SecretStorage::with_config(
            db,
            restflow_storage::SecretStorageConfig {
                allow_insecure_file_permissions: true,
            },
        )
        .unwrap();

        // Restore env vars immediately; SecretStorage caches master key at init time
        unsafe {
            match prev_dir {
                Some(v) => std::env::set_var("RESTFLOW_DIR", v),
                None => std::env::remove_var("RESTFLOW_DIR"),
            }
            match prev_key {
                Some(v) => std::env::set_var("RESTFLOW_MASTER_KEY", v),
                None => std::env::remove_var("RESTFLOW_MASTER_KEY"),
            }
        }

        (AuthProfileStorageAdapter::new(storage), temp_dir, guard)
    }

    #[test]
    fn test_add_and_list_profile() {
        let (adapter, _dir, _guard) = setup();
        let request = AuthProfileCreateRequest {
            name: "OpenAI Key".to_string(),
            provider: "openai".to_string(),
            source: None,
            credential: CredentialInput::ApiKey {
                key: "sk-test-key".to_string(),
                email: None,
            },
        };
        let result = adapter.add_profile(request).unwrap();
        assert_eq!(result["created"], true);
        assert_eq!(result["id"], "OPENAI_API_KEY");

        let list = adapter.list_profiles().unwrap();
        let profiles = list.as_array().unwrap();
        assert!(profiles.iter().any(|p| p["id"] == "OPENAI_API_KEY"));
    }

    #[test]
    fn test_remove_profile() {
        let (adapter, _dir, _guard) = setup();
        let request = AuthProfileCreateRequest {
            name: "Remove Me".to_string(),
            provider: "github".to_string(),
            source: None,
            credential: CredentialInput::Token {
                token: "ghp_test".to_string(),
                expires_at: None,
                email: None,
            },
        };
        adapter.add_profile(request).unwrap();
        let result = adapter.remove_profile("GITHUB_API_KEY").unwrap();
        assert_eq!(result["removed"], true);
    }

    #[test]
    fn test_test_profile_by_provider() {
        let (adapter, _dir, _guard) = setup();
        let request = AuthProfileCreateRequest {
            name: "Test".to_string(),
            provider: "anthropic".to_string(),
            source: None,
            credential: CredentialInput::ApiKey {
                key: "sk-ant-test".to_string(),
                email: None,
            },
        };
        adapter.add_profile(request).unwrap();

        let result = adapter
            .test_profile(AuthProfileTestRequest {
                id: None,
                provider: Some("anthropic".to_string()),
            })
            .unwrap();
        assert_eq!(result["available"], true);
    }

    #[test]
    fn test_test_profile_unavailable() {
        let (adapter, _dir, _guard) = setup();
        let result = adapter
            .test_profile(AuthProfileTestRequest {
                id: Some("NONEXISTENT_KEY".to_string()),
                provider: None,
            })
            .unwrap();
        assert_eq!(result["available"], false);
    }

    #[test]
    fn test_discover_profiles() {
        let (adapter, _dir, _guard) = setup();
        let result = adapter.discover_profiles().unwrap();
        assert!(result.get("total").is_some());
        assert!(result.get("profiles").is_some());
    }
}
