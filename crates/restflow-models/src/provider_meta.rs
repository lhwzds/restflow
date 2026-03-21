use restflow_traits::ModelProvider;

use crate::{LlmProvider, ModelId};

/// Shared metadata for a canonical provider identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderMeta {
    pub provider: ModelProvider,
    pub runtime_provider: LlmProvider,
    pub api_key_env: Option<&'static str>,
    pub api_key_env_aliases: &'static [&'static str],
    pub default_model_id: ModelId,
    pub models_dev_provider_ids: &'static [&'static str],
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
        api_key_env_aliases: &[],
        default_model_id: ModelId::Gpt5,
        models_dev_provider_ids: &["openai"],
    },
    ProviderMeta {
        provider: ModelProvider::Anthropic,
        runtime_provider: LlmProvider::Anthropic,
        api_key_env: Some("ANTHROPIC_API_KEY"),
        api_key_env_aliases: &[],
        default_model_id: ModelId::ClaudeOpus4_6,
        models_dev_provider_ids: &["anthropic"],
    },
    ProviderMeta {
        provider: ModelProvider::ClaudeCode,
        runtime_provider: LlmProvider::Anthropic,
        api_key_env: None,
        api_key_env_aliases: &[],
        default_model_id: ModelId::ClaudeCodeOpus,
        models_dev_provider_ids: &["claude-code", "anthropic"],
    },
    ProviderMeta {
        provider: ModelProvider::Codex,
        runtime_provider: LlmProvider::OpenAI,
        api_key_env: None,
        api_key_env_aliases: &[],
        default_model_id: ModelId::Gpt5_4Codex,
        models_dev_provider_ids: &["codex", "openai-codex", "openai"],
    },
    ProviderMeta {
        provider: ModelProvider::DeepSeek,
        runtime_provider: LlmProvider::DeepSeek,
        api_key_env: Some("DEEPSEEK_API_KEY"),
        api_key_env_aliases: &[],
        default_model_id: ModelId::DeepseekChat,
        models_dev_provider_ids: &["deepseek"],
    },
    ProviderMeta {
        provider: ModelProvider::Google,
        runtime_provider: LlmProvider::Google,
        api_key_env: Some("GEMINI_API_KEY"),
        api_key_env_aliases: &["GOOGLE_API_KEY"],
        default_model_id: ModelId::Gemini25Pro,
        models_dev_provider_ids: &["google"],
    },
    ProviderMeta {
        provider: ModelProvider::Groq,
        runtime_provider: LlmProvider::Groq,
        api_key_env: Some("GROQ_API_KEY"),
        api_key_env_aliases: &[],
        default_model_id: ModelId::GroqLlama4Maverick,
        models_dev_provider_ids: &["groq"],
    },
    ProviderMeta {
        provider: ModelProvider::OpenRouter,
        runtime_provider: LlmProvider::OpenRouter,
        api_key_env: Some("OPENROUTER_API_KEY"),
        api_key_env_aliases: &[],
        default_model_id: ModelId::OpenRouterAuto,
        models_dev_provider_ids: &["openrouter"],
    },
    ProviderMeta {
        provider: ModelProvider::XAI,
        runtime_provider: LlmProvider::XAI,
        api_key_env: Some("XAI_API_KEY"),
        api_key_env_aliases: &[],
        default_model_id: ModelId::Grok4,
        models_dev_provider_ids: &["xai"],
    },
    ProviderMeta {
        provider: ModelProvider::Qwen,
        runtime_provider: LlmProvider::Qwen,
        api_key_env: Some("DASHSCOPE_API_KEY"),
        api_key_env_aliases: &[],
        default_model_id: ModelId::Qwen3Max,
        models_dev_provider_ids: &["alibaba-cn", "alibaba"],
    },
    ProviderMeta {
        provider: ModelProvider::Zai,
        runtime_provider: LlmProvider::Zai,
        api_key_env: Some("ZAI_API_KEY"),
        api_key_env_aliases: &[],
        default_model_id: ModelId::Glm5,
        models_dev_provider_ids: &["zai", "zhipuai"],
    },
    ProviderMeta {
        provider: ModelProvider::ZaiCodingPlan,
        runtime_provider: LlmProvider::ZaiCodingPlan,
        api_key_env: Some("ZAI_CODING_PLAN_API_KEY"),
        api_key_env_aliases: &[],
        default_model_id: ModelId::Glm5CodingPlan,
        models_dev_provider_ids: &["zai-coding-plan", "zhipuai-coding-plan"],
    },
    ProviderMeta {
        provider: ModelProvider::Moonshot,
        runtime_provider: LlmProvider::Moonshot,
        api_key_env: Some("MOONSHOT_API_KEY"),
        api_key_env_aliases: &[],
        default_model_id: ModelId::KimiK2_5,
        models_dev_provider_ids: &["moonshotai", "moonshotai-cn", "kimi-for-coding"],
    },
    ProviderMeta {
        provider: ModelProvider::Doubao,
        runtime_provider: LlmProvider::Doubao,
        api_key_env: Some("ARK_API_KEY"),
        api_key_env_aliases: &[],
        default_model_id: ModelId::DoubaoPro,
        models_dev_provider_ids: &["doubao", "doubao-cn", "ark"],
    },
    ProviderMeta {
        provider: ModelProvider::Yi,
        runtime_provider: LlmProvider::Yi,
        api_key_env: Some("YI_API_KEY"),
        api_key_env_aliases: &[],
        default_model_id: ModelId::YiLightning,
        models_dev_provider_ids: &["yi"],
    },
    ProviderMeta {
        provider: ModelProvider::SiliconFlow,
        runtime_provider: LlmProvider::SiliconFlow,
        api_key_env: Some("SILICONFLOW_API_KEY"),
        api_key_env_aliases: &[],
        default_model_id: ModelId::SiliconFlowAuto,
        models_dev_provider_ids: &["siliconflow", "siliconflow-cn"],
    },
    ProviderMeta {
        provider: ModelProvider::MiniMax,
        runtime_provider: LlmProvider::MiniMax,
        api_key_env: Some("MINIMAX_API_KEY"),
        api_key_env_aliases: &[],
        default_model_id: ModelId::MiniMaxM27,
        models_dev_provider_ids: &["minimax", "minimax-cn"],
    },
    ProviderMeta {
        provider: ModelProvider::MiniMaxCodingPlan,
        runtime_provider: LlmProvider::MiniMaxCodingPlan,
        api_key_env: Some("MINIMAX_CODING_PLAN_API_KEY"),
        api_key_env_aliases: &[],
        default_model_id: ModelId::MiniMaxM25CodingPlan,
        models_dev_provider_ids: &["minimax-coding-plan", "minimax-cn-coding-plan"],
    },
];

pub fn provider_meta(provider: ModelProvider) -> &'static ProviderMeta {
    ALL_PROVIDER_META
        .iter()
        .find(|meta| meta.provider == provider)
        .unwrap_or_else(|| panic!("missing provider metadata for {provider:?}"))
}
