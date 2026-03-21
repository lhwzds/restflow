use crate::auth::AuthProvider;

use super::{ModelId, Provider};

#[derive(Debug, Clone, Copy)]
struct ProviderPolicy {
    default_model: ModelId,
    auth_providers: &'static [AuthProvider],
}

const AUTH_ANTHROPIC: &[AuthProvider] = &[AuthProvider::Anthropic];
const AUTH_CLAUDE_CODE: &[AuthProvider] = &[AuthProvider::ClaudeCode];
const AUTH_OPENAI: &[AuthProvider] = &[AuthProvider::OpenAI];
const AUTH_OPENAI_CODEX: &[AuthProvider] = &[AuthProvider::OpenAICodex];
const AUTH_GOOGLE: &[AuthProvider] = &[AuthProvider::Google];
const AUTH_OTHER: &[AuthProvider] = &[AuthProvider::Other];

const OPENAI_POLICY: ProviderPolicy = ProviderPolicy {
    default_model: ModelId::Gpt5,
    auth_providers: AUTH_OPENAI,
};

const ANTHROPIC_POLICY: ProviderPolicy = ProviderPolicy {
    default_model: ModelId::ClaudeOpus4_6,
    auth_providers: AUTH_ANTHROPIC,
};

const CLAUDE_CODE_POLICY: ProviderPolicy = ProviderPolicy {
    default_model: ModelId::ClaudeCodeOpus,
    auth_providers: AUTH_CLAUDE_CODE,
};

const CODEX_POLICY: ProviderPolicy = ProviderPolicy {
    default_model: ModelId::Gpt5_4Codex,
    auth_providers: AUTH_OPENAI_CODEX,
};

const DEEPSEEK_POLICY: ProviderPolicy = ProviderPolicy {
    default_model: ModelId::DeepseekChat,
    auth_providers: AUTH_OTHER,
};

const GOOGLE_POLICY: ProviderPolicy = ProviderPolicy {
    default_model: ModelId::Gemini25Pro,
    auth_providers: AUTH_GOOGLE,
};

const GROQ_POLICY: ProviderPolicy = ProviderPolicy {
    default_model: ModelId::GroqLlama4Maverick,
    auth_providers: AUTH_OTHER,
};

const OPENROUTER_POLICY: ProviderPolicy = ProviderPolicy {
    default_model: ModelId::OpenRouterAuto,
    auth_providers: AUTH_OTHER,
};

const XAI_POLICY: ProviderPolicy = ProviderPolicy {
    default_model: ModelId::Grok4,
    auth_providers: AUTH_OTHER,
};

const QWEN_POLICY: ProviderPolicy = ProviderPolicy {
    default_model: ModelId::Qwen3Max,
    auth_providers: AUTH_OTHER,
};

const ZAI_POLICY: ProviderPolicy = ProviderPolicy {
    default_model: ModelId::Glm5,
    auth_providers: AUTH_OTHER,
};

const ZAI_CODING_PLAN_POLICY: ProviderPolicy = ProviderPolicy {
    default_model: ModelId::Glm5CodingPlan,
    auth_providers: AUTH_OTHER,
};

const MOONSHOT_POLICY: ProviderPolicy = ProviderPolicy {
    default_model: ModelId::KimiK2_5,
    auth_providers: AUTH_OTHER,
};

const DOUBAO_POLICY: ProviderPolicy = ProviderPolicy {
    default_model: ModelId::DoubaoPro,
    auth_providers: AUTH_OTHER,
};

const YI_POLICY: ProviderPolicy = ProviderPolicy {
    default_model: ModelId::YiLightning,
    auth_providers: AUTH_OTHER,
};

const SILICONFLOW_POLICY: ProviderPolicy = ProviderPolicy {
    default_model: ModelId::SiliconFlowAuto,
    auth_providers: AUTH_OTHER,
};

const MINIMAX_POLICY: ProviderPolicy = ProviderPolicy {
    default_model: ModelId::MiniMaxM27,
    auth_providers: AUTH_OTHER,
};

const MINIMAX_CODING_PLAN_POLICY: ProviderPolicy = ProviderPolicy {
    default_model: ModelId::MiniMaxM25CodingPlan,
    auth_providers: AUTH_OTHER,
};

fn provider_policy(provider: Provider) -> &'static ProviderPolicy {
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
    provider_policy(provider).default_model
}

pub(crate) fn provider_auth_providers(provider: Provider) -> &'static [AuthProvider] {
    provider_policy(provider).auth_providers
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
