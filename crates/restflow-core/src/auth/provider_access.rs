use std::collections::HashMap;

use crate::models::{ModelId, Provider, provider_default_model};
use crate::storage::SecretStorage;
use restflow_models::LlmProvider;

use super::{AuthProfileManager, AuthProvider};

const PROFILE_PROVIDER_ORDER: &[(AuthProvider, Provider)] = &[
    (AuthProvider::OpenAICodex, Provider::Codex),
    (AuthProvider::ClaudeCode, Provider::ClaudeCode),
    (AuthProvider::Anthropic, Provider::Anthropic),
    (AuthProvider::OpenAI, Provider::OpenAI),
    (AuthProvider::Google, Provider::Google),
];

const SECRET_PROVIDER_ORDER: &[Provider] = &[
    Provider::MiniMaxCodingPlan,
    Provider::MiniMax,
    Provider::ZaiCodingPlan,
    Provider::Zai,
    Provider::Anthropic,
    Provider::OpenAI,
    Provider::Google,
    Provider::DeepSeek,
    Provider::Groq,
    Provider::OpenRouter,
    Provider::XAI,
    Provider::Qwen,
    Provider::Moonshot,
    Provider::Doubao,
    Provider::Yi,
    Provider::SiliconFlow,
];

pub(crate) fn secret_exists(storage: &SecretStorage, key: &str) -> bool {
    storage.get_non_empty(key).ok().flatten().is_some()
}

pub(crate) fn secret_or_env_exists(storage: &SecretStorage, key: &str) -> bool {
    if secret_exists(storage, key) {
        return true;
    }

    std::env::var(key)
        .ok()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

fn provider_has_secret<F>(provider: Provider, has_secret: &F) -> bool
where
    F: Fn(&str) -> bool,
{
    provider.api_key_env_candidates().any(has_secret)
}

pub(crate) async fn provider_available<F>(
    auth_manager: &AuthProfileManager,
    provider: Provider,
    has_secret: F,
) -> bool
where
    F: Fn(&str) -> bool,
{
    match provider {
        Provider::ClaudeCode => auth_manager
            .get_available_profile(AuthProvider::ClaudeCode)
            .await
            .is_some(),
        Provider::Codex => auth_manager
            .get_available_profile(AuthProvider::OpenAICodex)
            .await
            .is_some(),
        Provider::OpenAI => {
            provider_has_secret(provider, &has_secret)
                || auth_manager
                    .get_available_profile(AuthProvider::OpenAI)
                    .await
                    .is_some()
        }
        Provider::Anthropic => {
            provider_has_secret(provider, &has_secret)
                || auth_manager
                    .get_available_profile(AuthProvider::Anthropic)
                    .await
                    .is_some()
        }
        Provider::Google => {
            provider_has_secret(provider, &has_secret)
                || auth_manager
                    .get_available_profile(AuthProvider::Google)
                    .await
                    .is_some()
        }
        other => provider_has_secret(other, &has_secret),
    }
}

pub(crate) async fn resolve_model_from_credentials<F>(
    auth_manager: &AuthProfileManager,
    has_secret: F,
) -> Option<ModelId>
where
    F: Fn(&str) -> bool,
{
    for (auth_provider, provider) in PROFILE_PROVIDER_ORDER {
        if auth_manager
            .get_available_profile(*auth_provider)
            .await
            .is_some()
        {
            return Some(provider_default_model(*provider));
        }
    }

    for provider in SECRET_PROVIDER_ORDER {
        if provider_has_secret(*provider, &has_secret) {
            return Some(provider_default_model(*provider));
        }
    }

    None
}

fn resolve_provider_api_key(
    secret_storage: Option<&SecretStorage>,
    provider: Provider,
) -> Option<String> {
    for secret_name in provider.api_key_env_candidates() {
        if let Some(storage) = secret_storage
            && let Ok(Some(value)) = storage.get_secret(secret_name)
            && !value.trim().is_empty()
        {
            return Some(value);
        }

        if let Ok(value) = std::env::var(secret_name)
            && !value.trim().is_empty()
        {
            return Some(value);
        }
    }

    None
}

pub(crate) fn build_runtime_api_keys(
    secret_storage: Option<&SecretStorage>,
) -> HashMap<LlmProvider, String> {
    let mut keys = HashMap::new();
    for provider in Provider::all().iter().copied() {
        if let Some(value) = resolve_provider_api_key(secret_storage, provider) {
            keys.insert(provider.as_llm_provider(), value);
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use redb::Database;
    use tempfile::TempDir;
    use uuid::Uuid;

    use crate::auth::{Credential, CredentialSource, CredentialWriter};
    use crate::models::Provider;

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

    #[test]
    fn build_runtime_api_keys_accepts_google_legacy_secret_name() {
        let (secrets, _dir) = create_test_secrets();
        secrets
            .set_secret("GOOGLE_API_KEY", "legacy-google-key", None)
            .unwrap();

        let keys = build_runtime_api_keys(Some(&secrets));
        assert_eq!(
            keys.get(&LlmProvider::Google).map(String::as_str),
            Some("legacy-google-key")
        );
    }

    #[tokio::test]
    async fn provider_available_accepts_google_alias_secret() {
        let (secrets, _dir) = create_test_secrets();
        let manager = AuthProfileManager::new(secrets.clone());
        secrets
            .set_secret("GOOGLE_API_KEY", "legacy-google-key", None)
            .unwrap();

        let available = provider_available(&manager, Provider::Google, |key| {
            secret_exists(secrets.as_ref(), key)
        })
        .await;
        assert!(available);
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
