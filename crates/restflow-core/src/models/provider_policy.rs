use restflow_models::provider_meta;

use crate::auth::AuthProvider;

use super::{ModelId, Provider};

const AUTH_ANTHROPIC: &[AuthProvider] = &[AuthProvider::Anthropic];
const AUTH_CLAUDE_CODE: &[AuthProvider] = &[AuthProvider::ClaudeCode];
const AUTH_OPENAI: &[AuthProvider] = &[AuthProvider::OpenAI];
const AUTH_OPENAI_CODEX: &[AuthProvider] = &[AuthProvider::OpenAICodex];
const AUTH_GOOGLE: &[AuthProvider] = &[AuthProvider::Google];
const AUTH_OTHER: &[AuthProvider] = &[AuthProvider::Other];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProviderAccessPolicy {
    auth_profiles: &'static [AuthProvider],
    allow_secret_env: bool,
}

const ALL_PROVIDER_AUTH_POLICIES: &[(Provider, &[AuthProvider])] = &[
    (Provider::OpenAI, AUTH_OPENAI),
    (Provider::Anthropic, AUTH_ANTHROPIC),
    (Provider::ClaudeCode, AUTH_CLAUDE_CODE),
    (Provider::Codex, AUTH_OPENAI_CODEX),
    (Provider::DeepSeek, AUTH_OTHER),
    (Provider::Google, AUTH_GOOGLE),
    (Provider::Groq, AUTH_OTHER),
    (Provider::OpenRouter, AUTH_OTHER),
    (Provider::XAI, AUTH_OTHER),
    (Provider::Qwen, AUTH_OTHER),
    (Provider::Zai, AUTH_OTHER),
    (Provider::ZaiCodingPlan, AUTH_OTHER),
    (Provider::Moonshot, AUTH_OTHER),
    (Provider::Doubao, AUTH_OTHER),
    (Provider::Yi, AUTH_OTHER),
    (Provider::SiliconFlow, AUTH_OTHER),
    (Provider::MiniMax, AUTH_OTHER),
    (Provider::MiniMaxCodingPlan, AUTH_OTHER),
];

const DISPLAY_PROVIDER_ORDER: &[Provider] = &[
    Provider::OpenAI,
    Provider::MiniMaxCodingPlan,
    Provider::ZaiCodingPlan,
    Provider::ClaudeCode,
    Provider::Codex,
    Provider::Anthropic,
    Provider::Google,
    Provider::DeepSeek,
    Provider::Groq,
    Provider::OpenRouter,
    Provider::XAI,
    Provider::Qwen,
    Provider::Zai,
    Provider::Moonshot,
    Provider::Doubao,
    Provider::Yi,
    Provider::SiliconFlow,
    Provider::MiniMax,
];

const PROFILE_PROVIDER_RESOLUTION_ORDER: &[(AuthProvider, Provider)] = &[
    (AuthProvider::OpenAICodex, Provider::Codex),
    (AuthProvider::ClaudeCode, Provider::ClaudeCode),
    (AuthProvider::Anthropic, Provider::Anthropic),
    (AuthProvider::OpenAI, Provider::OpenAI),
    (AuthProvider::Google, Provider::Google),
];

const SECRET_PROVIDER_RESOLUTION_ORDER: &[Provider] = &[
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

const ACCESS_SECRET_ONLY: ProviderAccessPolicy = ProviderAccessPolicy {
    auth_profiles: &[],
    allow_secret_env: true,
};
const ACCESS_OPENAI: ProviderAccessPolicy = ProviderAccessPolicy {
    auth_profiles: AUTH_OPENAI,
    allow_secret_env: true,
};
const ACCESS_ANTHROPIC: ProviderAccessPolicy = ProviderAccessPolicy {
    auth_profiles: AUTH_ANTHROPIC,
    allow_secret_env: true,
};
const ACCESS_GOOGLE: ProviderAccessPolicy = ProviderAccessPolicy {
    auth_profiles: AUTH_GOOGLE,
    allow_secret_env: true,
};
const ACCESS_CLAUDE_CODE: ProviderAccessPolicy = ProviderAccessPolicy {
    auth_profiles: AUTH_CLAUDE_CODE,
    allow_secret_env: false,
};
const ACCESS_OPENAI_CODEX: ProviderAccessPolicy = ProviderAccessPolicy {
    auth_profiles: AUTH_OPENAI_CODEX,
    allow_secret_env: false,
};

const ALL_PROVIDER_ACCESS_POLICIES: &[(Provider, ProviderAccessPolicy)] = &[
    (Provider::OpenAI, ACCESS_OPENAI),
    (Provider::Anthropic, ACCESS_ANTHROPIC),
    (Provider::ClaudeCode, ACCESS_CLAUDE_CODE),
    (Provider::Codex, ACCESS_OPENAI_CODEX),
    (Provider::DeepSeek, ACCESS_SECRET_ONLY),
    (Provider::Google, ACCESS_GOOGLE),
    (Provider::Groq, ACCESS_SECRET_ONLY),
    (Provider::OpenRouter, ACCESS_SECRET_ONLY),
    (Provider::XAI, ACCESS_SECRET_ONLY),
    (Provider::Qwen, ACCESS_SECRET_ONLY),
    (Provider::Zai, ACCESS_SECRET_ONLY),
    (Provider::ZaiCodingPlan, ACCESS_SECRET_ONLY),
    (Provider::Moonshot, ACCESS_SECRET_ONLY),
    (Provider::Doubao, ACCESS_SECRET_ONLY),
    (Provider::Yi, ACCESS_SECRET_ONLY),
    (Provider::SiliconFlow, ACCESS_SECRET_ONLY),
    (Provider::MiniMax, ACCESS_SECRET_ONLY),
    (Provider::MiniMaxCodingPlan, ACCESS_SECRET_ONLY),
];

fn provider_auth_policy(provider: Provider) -> &'static [AuthProvider] {
    ALL_PROVIDER_AUTH_POLICIES
        .iter()
        .find_map(|(candidate, policy)| (*candidate == provider).then_some(policy))
        .unwrap_or_else(|| panic!("missing auth policy for {}", provider.as_canonical_str()))
}

fn provider_access_policy(provider: Provider) -> ProviderAccessPolicy {
    ALL_PROVIDER_ACCESS_POLICIES
        .iter()
        .find_map(|(candidate, policy)| (*candidate == provider).then_some(*policy))
        .unwrap_or_else(|| panic!("missing access policy for {}", provider.as_canonical_str()))
}

pub(crate) fn provider_default_model(provider: Provider) -> ModelId {
    provider_meta(provider.as_model_provider()).default_model_id
}

pub(crate) fn provider_auth_providers(provider: Provider) -> &'static [AuthProvider] {
    provider_auth_policy(provider)
}

pub(crate) fn provider_access_profiles(provider: Provider) -> &'static [AuthProvider] {
    provider_access_policy(provider).auth_profiles
}

pub(crate) fn provider_allows_secret_env(provider: Provider) -> bool {
    provider_access_policy(provider).allow_secret_env
}

pub(crate) fn provider_display_order(provider: Provider) -> usize {
    DISPLAY_PROVIDER_ORDER
        .iter()
        .position(|candidate| *candidate == provider)
        .unwrap_or(usize::MAX)
}

pub(crate) fn profile_provider_resolution_order() -> &'static [(AuthProvider, Provider)] {
    PROFILE_PROVIDER_RESOLUTION_ORDER
}

pub(crate) fn secret_provider_resolution_order() -> &'static [Provider] {
    SECRET_PROVIDER_RESOLUTION_ORDER
}

#[cfg(test)]
mod tests {
    use super::{
        ALL_PROVIDER_ACCESS_POLICIES, ALL_PROVIDER_AUTH_POLICIES, DISPLAY_PROVIDER_ORDER,
        profile_provider_resolution_order, provider_access_profiles, provider_allows_secret_env,
        provider_auth_providers, provider_default_model, provider_display_order,
        secret_provider_resolution_order,
    };
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

    #[test]
    fn provider_access_policy_table_stays_in_sync() {
        assert_eq!(Provider::all().len(), ALL_PROVIDER_ACCESS_POLICIES.len());
        assert_eq!(
            provider_access_profiles(Provider::Codex),
            &[AuthProvider::OpenAICodex]
        );
        assert!(!provider_allows_secret_env(Provider::Codex));
        assert!(provider_allows_secret_env(Provider::OpenAI));
    }

    #[test]
    fn provider_display_order_places_coding_first() {
        assert!(
            provider_display_order(Provider::OpenAI) < provider_display_order(Provider::Anthropic)
        );
        assert!(
            provider_display_order(Provider::MiniMaxCodingPlan)
                < provider_display_order(Provider::DeepSeek)
        );
        assert_eq!(Provider::all().len(), DISPLAY_PROVIDER_ORDER.len());
    }

    #[test]
    fn provider_resolution_orders_match_expected_prefixes() {
        assert_eq!(
            profile_provider_resolution_order()[0],
            (AuthProvider::OpenAICodex, Provider::Codex)
        );
        assert_eq!(
            secret_provider_resolution_order()[0],
            Provider::MiniMaxCodingPlan
        );
    }
}
