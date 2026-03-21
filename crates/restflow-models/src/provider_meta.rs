use restflow_traits::ModelProvider;

use crate::LlmProvider;

/// Shared metadata for a canonical provider identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderMeta {
    pub provider: ModelProvider,
    pub runtime_provider: LlmProvider,
    pub api_key_env: Option<&'static str>,
}

impl ProviderMeta {
    pub fn canonical_name(self) -> &'static str {
        self.provider.canonical_str()
    }
}

pub const ALL_PROVIDER_META: &[ProviderMeta] = &[
    ProviderMeta {
        provider: ModelProvider::OpenAI,
        runtime_provider: LlmProvider::OpenAI,
        api_key_env: Some("OPENAI_API_KEY"),
    },
    ProviderMeta {
        provider: ModelProvider::Anthropic,
        runtime_provider: LlmProvider::Anthropic,
        api_key_env: Some("ANTHROPIC_API_KEY"),
    },
    ProviderMeta {
        provider: ModelProvider::ClaudeCode,
        runtime_provider: LlmProvider::Anthropic,
        api_key_env: None,
    },
    ProviderMeta {
        provider: ModelProvider::Codex,
        runtime_provider: LlmProvider::OpenAI,
        api_key_env: None,
    },
    ProviderMeta {
        provider: ModelProvider::DeepSeek,
        runtime_provider: LlmProvider::DeepSeek,
        api_key_env: Some("DEEPSEEK_API_KEY"),
    },
    ProviderMeta {
        provider: ModelProvider::Google,
        runtime_provider: LlmProvider::Google,
        api_key_env: Some("GEMINI_API_KEY"),
    },
    ProviderMeta {
        provider: ModelProvider::Groq,
        runtime_provider: LlmProvider::Groq,
        api_key_env: Some("GROQ_API_KEY"),
    },
    ProviderMeta {
        provider: ModelProvider::OpenRouter,
        runtime_provider: LlmProvider::OpenRouter,
        api_key_env: Some("OPENROUTER_API_KEY"),
    },
    ProviderMeta {
        provider: ModelProvider::XAI,
        runtime_provider: LlmProvider::XAI,
        api_key_env: Some("XAI_API_KEY"),
    },
    ProviderMeta {
        provider: ModelProvider::Qwen,
        runtime_provider: LlmProvider::Qwen,
        api_key_env: Some("DASHSCOPE_API_KEY"),
    },
    ProviderMeta {
        provider: ModelProvider::Zai,
        runtime_provider: LlmProvider::Zai,
        api_key_env: Some("ZAI_API_KEY"),
    },
    ProviderMeta {
        provider: ModelProvider::ZaiCodingPlan,
        runtime_provider: LlmProvider::ZaiCodingPlan,
        api_key_env: Some("ZAI_CODING_PLAN_API_KEY"),
    },
    ProviderMeta {
        provider: ModelProvider::Moonshot,
        runtime_provider: LlmProvider::Moonshot,
        api_key_env: Some("MOONSHOT_API_KEY"),
    },
    ProviderMeta {
        provider: ModelProvider::Doubao,
        runtime_provider: LlmProvider::Doubao,
        api_key_env: Some("ARK_API_KEY"),
    },
    ProviderMeta {
        provider: ModelProvider::Yi,
        runtime_provider: LlmProvider::Yi,
        api_key_env: Some("YI_API_KEY"),
    },
    ProviderMeta {
        provider: ModelProvider::SiliconFlow,
        runtime_provider: LlmProvider::SiliconFlow,
        api_key_env: Some("SILICONFLOW_API_KEY"),
    },
    ProviderMeta {
        provider: ModelProvider::MiniMax,
        runtime_provider: LlmProvider::MiniMax,
        api_key_env: Some("MINIMAX_API_KEY"),
    },
    ProviderMeta {
        provider: ModelProvider::MiniMaxCodingPlan,
        runtime_provider: LlmProvider::MiniMaxCodingPlan,
        api_key_env: Some("MINIMAX_CODING_PLAN_API_KEY"),
    },
];

pub fn provider_meta(provider: ModelProvider) -> &'static ProviderMeta {
    ALL_PROVIDER_META
        .iter()
        .find(|meta| meta.provider == provider)
        .unwrap_or_else(|| panic!("missing provider metadata for {provider:?}"))
}
