use crate::provider_policy::{
    profile_provider_resolution_order, provider_access_profiles, provider_allows_secret_env,
    provider_default_model, secret_provider_resolution_order,
};
use crate::storage::SecretStorage;
use types::{ModelId, Provider};

use super::AuthProfileManager;

pub fn secret_exists(storage: &SecretStorage, key: &str) -> bool {
    storage.get_non_empty(key).ok().flatten().is_some()
}

fn provider_has_secret<F>(provider: Provider, has_secret: &F) -> bool
where
    F: Fn(&str) -> bool,
{
    provider.api_key_env_candidates().any(has_secret)
}

pub async fn provider_available<F>(
    auth_manager: &AuthProfileManager,
    provider: Provider,
    has_secret: F,
) -> bool
where
    F: Fn(&str) -> bool,
{
    if provider_allows_secret_env(provider) && provider_has_secret(provider, &has_secret) {
        return true;
    }

    for auth_provider in provider_access_profiles(provider) {
        if auth_manager
            .get_available_profile(*auth_provider)
            .await
            .is_some()
        {
            return true;
        }
    }

    false
}

pub async fn resolve_model_from_credentials<F>(
    auth_manager: &AuthProfileManager,
    has_secret: F,
) -> Option<ModelId>
where
    F: Fn(&str) -> bool,
{
    for (auth_provider, provider) in profile_provider_resolution_order() {
        if auth_manager
            .get_available_profile(*auth_provider)
            .await
            .is_some()
        {
            return Some(provider_default_model(*provider));
        }
    }

    for provider in secret_provider_resolution_order() {
        if provider_has_secret(*provider, &has_secret) {
            return Some(provider_default_model(*provider));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::auth::AuthProvider;
    use redb::Database;
    use tempfile::TempDir;
    use uuid::Uuid;

    use crate::auth::{Credential, CredentialSource, CredentialWriter};
    use types::Provider;

    use super::*;

    fn create_test_secrets() -> (Arc<SecretStorage>, TempDir) {
        let dir = TempDir::new().unwrap();
        let db = Arc::new(Database::create(dir.path().join("test.db")).unwrap());
        let secrets = Arc::new(SecretStorage::new(db).unwrap());
        (secrets, dir)
    }

    fn create_test_profile(
        secrets: &Arc<SecretStorage>,
        name: &str,
        provider: AuthProvider,
    ) -> crate::auth::AuthProfile {
        let writer = CredentialWriter::new(secrets.clone());
        let profile_id = Uuid::new_v4().to_string();
        let credential = Credential::ApiKey {
            key: format!("test-key-{name}"),
            email: None,
        };
        let secure = writer.store_credential(&profile_id, &credential).unwrap();
        crate::auth::AuthProfile::new_with_id(
            profile_id,
            name,
            secure,
            CredentialSource::Manual,
            provider,
        )
    }

    #[tokio::test]
    async fn provider_available_accepts_google_secret() {
        let (secrets, _dir) = create_test_secrets();
        let manager = AuthProfileManager::new(secrets.clone());
        secrets
            .set_secret("GEMINI_API_KEY", "google-key", None)
            .unwrap();

        let available = provider_available(&manager, Provider::Google, |key| {
            secret_exists(secrets.as_ref(), key)
        })
        .await;
        assert!(available);
    }

    #[tokio::test]
    async fn provider_available_accepts_openai_profile_without_secret() {
        let (secrets, _dir) = create_test_secrets();
        let manager = AuthProfileManager::new(secrets.clone());
        manager
            .add_profile(create_test_profile(
                &secrets,
                "OpenAI",
                AuthProvider::OpenAI,
            ))
            .await
            .unwrap();

        let available = provider_available(&manager, Provider::OpenAI, |key| {
            secret_exists(secrets.as_ref(), key)
        })
        .await;
        assert!(available);
    }

    #[tokio::test]
    async fn provider_available_requires_dedicated_codex_profile() {
        let (secrets, _dir) = create_test_secrets();
        let manager = AuthProfileManager::new(secrets.clone());
        manager
            .add_profile(create_test_profile(
                &secrets,
                "OpenAI",
                AuthProvider::OpenAI,
            ))
            .await
            .unwrap();
        secrets
            .set_secret("OPENAI_API_KEY", "openai-key", None)
            .unwrap();

        let available = provider_available(&manager, Provider::Codex, |key| {
            secret_exists(secrets.as_ref(), key)
        })
        .await;
        assert!(!available);
    }

    #[tokio::test]
    async fn resolve_model_from_credentials_prefers_dedicated_auth_profiles() {
        let (secrets, _dir) = create_test_secrets();
        let manager = AuthProfileManager::new(secrets.clone());
        manager
            .add_profile(create_test_profile(
                &secrets,
                "Codex",
                AuthProvider::OpenAICodex,
            ))
            .await
            .unwrap();
        secrets
            .set_secret("OPENAI_API_KEY", "openai-key", None)
            .unwrap();

        let model =
            resolve_model_from_credentials(&manager, |key| secret_exists(secrets.as_ref(), key))
                .await;
        assert_eq!(model, Some(ModelId::Gpt5_4Codex));
    }
}
