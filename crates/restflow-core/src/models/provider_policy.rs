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

const OPENAI_POLICY: ProviderAuthPolicy = ProviderAuthPolicy {
    auth_providers: AUTH_OPENAI,
};

const ANTHROPIC_POLICY: ProviderAuthPolicy = ProviderAuthPolicy {
    auth_providers: AUTH_ANTHROPIC,
};

const CLAUDE_CODE_POLICY: ProviderAuthPolicy = ProviderAuthPolicy {
    auth_providers: AUTH_CLAUDE_CODE,
};

const CODEX_POLICY: ProviderAuthPolicy = ProviderAuthPolicy {
    auth_providers: AUTH_OPENAI_CODEX,
};

const DEEPSEEK_POLICY: ProviderAuthPolicy = ProviderAuthPolicy {
    auth_providers: AUTH_OTHER,
};

const GOOGLE_POLICY: ProviderAuthPolicy = ProviderAuthPolicy {
    auth_providers: AUTH_GOOGLE,
};

const GROQ_POLICY: ProviderAuthPolicy = ProviderAuthPolicy {
    auth_providers: AUTH_OTHER,
};

const OPENROUTER_POLICY: ProviderAuthPolicy = ProviderAuthPolicy {
    auth_providers: AUTH_OTHER,
};

const XAI_POLICY: ProviderAuthPolicy = ProviderAuthPolicy {
    auth_providers: AUTH_OTHER,
};

const QWEN_POLICY: ProviderAuthPolicy = ProviderAuthPolicy {
    auth_providers: AUTH_OTHER,
};

const ZAI_POLICY: ProviderAuthPolicy = ProviderAuthPolicy {
    auth_providers: AUTH_OTHER,
};

const ZAI_CODING_PLAN_POLICY: ProviderAuthPolicy = ProviderAuthPolicy {
    auth_providers: AUTH_OTHER,
};

const MOONSHOT_POLICY: ProviderAuthPolicy = ProviderAuthPolicy {
    auth_providers: AUTH_OTHER,
};

const DOUBAO_POLICY: ProviderAuthPolicy = ProviderAuthPolicy {
    auth_providers: AUTH_OTHER,
};

const YI_POLICY: ProviderAuthPolicy = ProviderAuthPolicy {
    auth_providers: AUTH_OTHER,
};

const SILICONFLOW_POLICY: ProviderAuthPolicy = ProviderAuthPolicy {
    auth_providers: AUTH_OTHER,
};

const MINIMAX_POLICY: ProviderAuthPolicy = ProviderAuthPolicy {
    auth_providers: AUTH_OTHER,
};

const MINIMAX_CODING_PLAN_POLICY: ProviderAuthPolicy = ProviderAuthPolicy {
    auth_providers: AUTH_OTHER,
};

fn provider_auth_policy(provider: Provider) -> &'static ProviderAuthPolicy {
    match provider {
        Provider::OpenAI => &OPENAI_POLICY,
        Provider::Anthropic => &ANTHROPIC_POLICY,
        Provider::ClaudeCode => &CLAUDE_CODE_POLICY,
        Provider::Codex => &CODEX_POLICY,
        Provider::DeepSeek => &DEEPSEEK_POLICY,
        Provider::Google => &GOOGLE_POLICY,
        Provider::Groq => &GROQ_POLICY,
        Provider::OpenRouter => &OPENROUTER_POLICY,
        Provider::XAI => &XAI_POLICY,
        Provider::Qwen => &QWEN_POLICY,
        Provider::Zai => &ZAI_POLICY,
        Provider::ZaiCodingPlan => &ZAI_CODING_PLAN_POLICY,
        Provider::Moonshot => &MOONSHOT_POLICY,
        Provider::Doubao => &DOUBAO_POLICY,
        Provider::Yi => &YI_POLICY,
        Provider::SiliconFlow => &SILICONFLOW_POLICY,
        Provider::MiniMax => &MINIMAX_POLICY,
        Provider::MiniMaxCodingPlan => &MINIMAX_CODING_PLAN_POLICY,
    }
}

pub(crate) fn provider_default_model(provider: Provider) -> ModelId {
    let serialized = provider_meta(provider.as_model_provider()).default_model_id;
    ModelId::from_serialized_str(serialized).unwrap_or_else(|| {
        panic!(
            "missing default model '{}' for provider {}",
            serialized,
            provider.as_canonical_str()
        )
    })
}

pub(crate) fn provider_auth_providers(provider: Provider) -> &'static [AuthProvider] {
    provider_auth_policy(provider).auth_providers
}

#[cfg(test)]
mod tests {
    use super::{provider_auth_providers, provider_default_model};
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
}
