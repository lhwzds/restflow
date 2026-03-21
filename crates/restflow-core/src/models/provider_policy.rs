use restflow_models::provider_meta;

use crate::auth::AuthProvider;

use super::{ModelId, Provider};

#[derive(Debug, Clone, Copy)]
struct ProviderAuthPolicy {
    auth_providers: &'static [AuthProvider],
}

const AUTH_ANTHROPIC: &[AuthProvider] = &[AuthProvider::Anthropic];
const AUTH_CLAUDE_CODE: &[AuthProvider] = &[AuthProvider::ClaudeCode];
const AUTH_OPENAI: &[AuthProvider] = &[AuthProvider::OpenAI];
const AUTH_OPENAI_CODEX: &[AuthProvider] = &[AuthProvider::OpenAICodex];
const AUTH_GOOGLE: &[AuthProvider] = &[AuthProvider::Google];
const AUTH_OTHER: &[AuthProvider] = &[AuthProvider::Other];

const ALL_PROVIDER_AUTH_POLICIES: &[(Provider, ProviderAuthPolicy)] = &[
    (
        Provider::OpenAI,
        ProviderAuthPolicy {
            auth_providers: AUTH_OPENAI,
        },
    ),
    (
        Provider::Anthropic,
        ProviderAuthPolicy {
            auth_providers: AUTH_ANTHROPIC,
        },
    ),
    (
        Provider::ClaudeCode,
        ProviderAuthPolicy {
            auth_providers: AUTH_CLAUDE_CODE,
        },
    ),
    (
        Provider::Codex,
        ProviderAuthPolicy {
            auth_providers: AUTH_OPENAI_CODEX,
        },
    ),
    (
        Provider::DeepSeek,
        ProviderAuthPolicy {
            auth_providers: AUTH_OTHER,
        },
    ),
    (
        Provider::Google,
        ProviderAuthPolicy {
            auth_providers: AUTH_GOOGLE,
        },
    ),
    (
        Provider::Groq,
        ProviderAuthPolicy {
            auth_providers: AUTH_OTHER,
        },
    ),
    (
        Provider::OpenRouter,
        ProviderAuthPolicy {
            auth_providers: AUTH_OTHER,
        },
    ),
    (
        Provider::XAI,
        ProviderAuthPolicy {
            auth_providers: AUTH_OTHER,
        },
    ),
    (
        Provider::Qwen,
        ProviderAuthPolicy {
            auth_providers: AUTH_OTHER,
        },
    ),
    (
        Provider::Zai,
        ProviderAuthPolicy {
            auth_providers: AUTH_OTHER,
        },
    ),
    (
        Provider::ZaiCodingPlan,
        ProviderAuthPolicy {
            auth_providers: AUTH_OTHER,
        },
    ),
    (
        Provider::Moonshot,
        ProviderAuthPolicy {
            auth_providers: AUTH_OTHER,
        },
    ),
    (
        Provider::Doubao,
        ProviderAuthPolicy {
            auth_providers: AUTH_OTHER,
        },
    ),
    (
        Provider::Yi,
        ProviderAuthPolicy {
            auth_providers: AUTH_OTHER,
        },
    ),
    (
        Provider::SiliconFlow,
        ProviderAuthPolicy {
            auth_providers: AUTH_OTHER,
        },
    ),
    (
        Provider::MiniMax,
        ProviderAuthPolicy {
            auth_providers: AUTH_OTHER,
        },
    ),
    (
        Provider::MiniMaxCodingPlan,
        ProviderAuthPolicy {
            auth_providers: AUTH_OTHER,
        },
    ),
];

fn provider_auth_policy(provider: Provider) -> &'static ProviderAuthPolicy {
    ALL_PROVIDER_AUTH_POLICIES
        .iter()
        .find_map(|(candidate, policy)| (*candidate == provider).then_some(policy))
        .unwrap_or_else(|| panic!("missing auth policy for {}", provider.as_canonical_str()))
}

pub(crate) fn provider_default_model(provider: Provider) -> ModelId {
    provider_meta(provider.as_model_provider()).default_model_id
}

pub(crate) fn provider_auth_providers(provider: Provider) -> &'static [AuthProvider] {
    provider_auth_policy(provider).auth_providers
}

#[cfg(test)]
mod tests {
    use super::{ALL_PROVIDER_AUTH_POLICIES, provider_auth_providers, provider_default_model};
    use crate::auth::AuthProvider;
    use crate::models::{ModelId, Provider};

    #[test]
    fn provider_default_model_uses_runtime_defaults() {
        assert_eq!(
            provider_default_model(Provider::Anthropic),
            ModelId::ClaudeOpus4_6
        );
        assert_eq!(
            provider_default_model(Provider::MiniMax),
            ModelId::MiniMaxM27
        );
    }

    #[test]
    fn provider_auth_providers_match_expected_preferences() {
        assert_eq!(
            provider_auth_providers(Provider::OpenAI),
            &[AuthProvider::OpenAI]
        );
        assert_eq!(
            provider_auth_providers(Provider::Codex),
            &[AuthProvider::OpenAICodex]
        );
        assert_eq!(
            provider_auth_providers(Provider::ZaiCodingPlan),
            &[AuthProvider::Other]
        );
    }

    #[test]
    fn provider_auth_policy_table_stays_in_sync() {
        assert_eq!(Provider::all().len(), ALL_PROVIDER_AUTH_POLICIES.len());
    }
}
