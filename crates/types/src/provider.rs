use crate::{ClientKind, LlmProvider, ModelId, ModelProvider, catalog};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use specta::Type;

/// Shared metadata for a canonical provider identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderMeta {
    pub provider: ModelProvider,
    pub runtime_provider: LlmProvider,
    pub api_key_env: Option<&'static str>,
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
        default_model_id: ModelId::Gpt5_4,
        models_dev_provider_ids: &["openai"],
    },
    ProviderMeta {
        provider: ModelProvider::Anthropic,
        runtime_provider: LlmProvider::Anthropic,
        api_key_env: Some("ANTHROPIC_API_KEY"),
        default_model_id: ModelId::ClaudeOpus4_6,
        models_dev_provider_ids: &["anthropic"],
    },
    ProviderMeta {
        provider: ModelProvider::ClaudeCode,
        runtime_provider: LlmProvider::Anthropic,
        api_key_env: None,
        default_model_id: ModelId::ClaudeCodeOpus,
        models_dev_provider_ids: &["claude-code", "anthropic"],
    },
    ProviderMeta {
        provider: ModelProvider::Codex,
        runtime_provider: LlmProvider::OpenAI,
        api_key_env: None,
        default_model_id: ModelId::Gpt5_4Codex,
        models_dev_provider_ids: &["codex", "openai-codex", "openai"],
    },
    ProviderMeta {
        provider: ModelProvider::DeepSeek,
        runtime_provider: LlmProvider::DeepSeek,
        api_key_env: Some("DEEPSEEK_API_KEY"),
        default_model_id: ModelId::DeepseekChat,
        models_dev_provider_ids: &["deepseek"],
    },
    ProviderMeta {
        provider: ModelProvider::Google,
        runtime_provider: LlmProvider::Google,
        api_key_env: Some("GEMINI_API_KEY"),
        default_model_id: ModelId::Gemini25Pro,
        models_dev_provider_ids: &["google"],
    },
    ProviderMeta {
        provider: ModelProvider::Groq,
        runtime_provider: LlmProvider::Groq,
        api_key_env: Some("GROQ_API_KEY"),
        default_model_id: ModelId::GroqLlama4Maverick,
        models_dev_provider_ids: &["groq"],
    },
    ProviderMeta {
        provider: ModelProvider::OpenRouter,
        runtime_provider: LlmProvider::OpenRouter,
        api_key_env: Some("OPENROUTER_API_KEY"),
        default_model_id: ModelId::OpenRouterAuto,
        models_dev_provider_ids: &["openrouter"],
    },
    ProviderMeta {
        provider: ModelProvider::XAI,
        runtime_provider: LlmProvider::XAI,
        api_key_env: Some("XAI_API_KEY"),
        default_model_id: ModelId::Grok4,
        models_dev_provider_ids: &["xai"],
    },
    ProviderMeta {
        provider: ModelProvider::Qwen,
        runtime_provider: LlmProvider::Qwen,
        api_key_env: Some("DASHSCOPE_API_KEY"),
        default_model_id: ModelId::Qwen3Max,
        models_dev_provider_ids: &["alibaba-cn", "alibaba"],
    },
    ProviderMeta {
        provider: ModelProvider::Zai,
        runtime_provider: LlmProvider::Zai,
        api_key_env: Some("ZAI_API_KEY"),
        default_model_id: ModelId::Glm5,
        models_dev_provider_ids: &["zai", "zhipuai"],
    },
    ProviderMeta {
        provider: ModelProvider::ZaiCodingPlan,
        runtime_provider: LlmProvider::ZaiCodingPlan,
        api_key_env: Some("ZAI_CODING_PLAN_API_KEY"),
        default_model_id: ModelId::Glm5_1CodingPlan,
        models_dev_provider_ids: &["zai-coding-plan", "zhipuai-coding-plan"],
    },
    ProviderMeta {
        provider: ModelProvider::Moonshot,
        runtime_provider: LlmProvider::Moonshot,
        api_key_env: Some("MOONSHOT_API_KEY"),
        default_model_id: ModelId::KimiK2_5,
        models_dev_provider_ids: &["moonshotai", "moonshotai-cn", "kimi-for-coding"],
    },
    ProviderMeta {
        provider: ModelProvider::Doubao,
        runtime_provider: LlmProvider::Doubao,
        api_key_env: Some("ARK_API_KEY"),
        default_model_id: ModelId::DoubaoPro,
        models_dev_provider_ids: &["doubao", "doubao-cn", "ark"],
    },
    ProviderMeta {
        provider: ModelProvider::Yi,
        runtime_provider: LlmProvider::Yi,
        api_key_env: Some("YI_API_KEY"),
        default_model_id: ModelId::YiLightning,
        models_dev_provider_ids: &["yi"],
    },
    ProviderMeta {
        provider: ModelProvider::SiliconFlow,
        runtime_provider: LlmProvider::SiliconFlow,
        api_key_env: Some("SILICONFLOW_API_KEY"),
        default_model_id: ModelId::SiliconFlowAuto,
        models_dev_provider_ids: &["siliconflow", "siliconflow-cn"],
    },
    ProviderMeta {
        provider: ModelProvider::MiniMax,
        runtime_provider: LlmProvider::MiniMax,
        api_key_env: Some("MINIMAX_API_KEY"),
        default_model_id: ModelId::MiniMaxM27,
        models_dev_provider_ids: &["minimax", "minimax-cn"],
    },
    ProviderMeta {
        provider: ModelProvider::MiniMaxCodingPlan,
        runtime_provider: LlmProvider::MiniMaxCodingPlan,
        api_key_env: Some("MINIMAX_CODING_PLAN_API_KEY"),
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

/// API-facing provider wrapper backed by the shared canonical provider identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Type)]
#[repr(transparent)]
#[specta(transparent)]
pub struct Provider(ModelProvider);

#[allow(non_upper_case_globals)]
impl Provider {
    pub const OpenAI: Self = Self(ModelProvider::OpenAI);
    pub const Anthropic: Self = Self(ModelProvider::Anthropic);
    pub const ClaudeCode: Self = Self(ModelProvider::ClaudeCode);
    pub const Codex: Self = Self(ModelProvider::Codex);
    pub const DeepSeek: Self = Self(ModelProvider::DeepSeek);
    pub const Google: Self = Self(ModelProvider::Google);
    pub const Groq: Self = Self(ModelProvider::Groq);
    pub const OpenRouter: Self = Self(ModelProvider::OpenRouter);
    pub const XAI: Self = Self(ModelProvider::XAI);
    pub const Qwen: Self = Self(ModelProvider::Qwen);
    pub const Zai: Self = Self(ModelProvider::Zai);
    pub const ZaiCodingPlan: Self = Self(ModelProvider::ZaiCodingPlan);
    pub const Moonshot: Self = Self(ModelProvider::Moonshot);
    pub const Doubao: Self = Self(ModelProvider::Doubao);
    pub const Yi: Self = Self(ModelProvider::Yi);
    pub const SiliconFlow: Self = Self(ModelProvider::SiliconFlow);
    pub const MiniMax: Self = Self(ModelProvider::MiniMax);
    pub const MiniMaxCodingPlan: Self = Self(ModelProvider::MiniMaxCodingPlan);

    pub fn all() -> &'static [Provider] {
        &ALL_PROVIDERS
    }

    /// Convert to shared provider identity used by cross-crate parsers.
    pub const fn as_model_provider(self) -> ModelProvider {
        self.0
    }

    /// Convert from shared provider identity.
    pub const fn from_model_provider(provider: ModelProvider) -> Self {
        Self(provider)
    }

    pub fn api_key_env(self) -> Option<&'static str> {
        provider_meta(self.0).api_key_env
    }

    pub fn api_key_env_candidates(self) -> impl Iterator<Item = &'static str> {
        provider_meta(self.0).api_key_env.into_iter()
    }

    /// Convert Provider to LLM provider used by runtime factory.
    pub fn as_llm_provider(self) -> LlmProvider {
        provider_meta(self.0).runtime_provider
    }

    /// Get the canonical provider identifier for use in canonical model IDs.
    /// Returns lowercase provider name (e.g., "openai", "anthropic").
    pub fn as_canonical_str(self) -> &'static str {
        provider_meta(self.0).canonical_name()
    }

    /// Parse a canonical provider string back to Provider.
    /// Returns None if the string is not recognized.
    pub fn from_canonical_str(s: &str) -> Option<Self> {
        ModelProvider::parse_alias(s).map(Self)
    }

    /// Get the best available model for this provider.
    pub fn flagship_model(self) -> ModelId {
        catalog::provider_catalog(self)
            .map(|catalog| catalog.flagship)
            .unwrap_or_else(|| panic!("missing provider catalog for {}", self.as_canonical_str()))
    }
}

impl From<ModelProvider> for Provider {
    fn from(value: ModelProvider) -> Self {
        Self(value)
    }
}

impl From<Provider> for ModelProvider {
    fn from(value: Provider) -> Self {
        value.0
    }
}

impl Serialize for Provider {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_canonical_str())
    }
}

impl<'de> Deserialize<'de> for Provider {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::from_canonical_str(&raw)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown provider: {raw}")))
    }
}

const ALL_PROVIDERS: [Provider; 18] = [
    Provider::OpenAI,
    Provider::Anthropic,
    Provider::ClaudeCode,
    Provider::Codex,
    Provider::DeepSeek,
    Provider::Google,
    Provider::Groq,
    Provider::OpenRouter,
    Provider::XAI,
    Provider::Qwen,
    Provider::Zai,
    Provider::ZaiCodingPlan,
    Provider::Moonshot,
    Provider::Doubao,
    Provider::Yi,
    Provider::SiliconFlow,
    Provider::MiniMax,
    Provider::MiniMaxCodingPlan,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderSelector {
    Provider(Provider),
    ClientKind(ClientKind),
}

impl ProviderSelector {
    pub fn label(self) -> &'static str {
        match self {
            Self::Provider(Provider::ClaudeCode) | Self::ClientKind(ClientKind::ClaudeCodeCli) => {
                "claude-code"
            }
            Self::Provider(Provider::Codex) | Self::ClientKind(ClientKind::CodexCli) => {
                "openai-codex"
            }
            Self::Provider(provider) => provider.as_canonical_str(),
            Self::ClientKind(ClientKind::Http) => "http",
            Self::ClientKind(ClientKind::OpenCodeCli) => "opencode-cli",
            Self::ClientKind(ClientKind::GeminiCli) => "gemini-cli",
        }
    }

    pub fn matches_model(self, model: ModelId) -> bool {
        match self {
            Self::Provider(provider) => model.provider() == provider,
            Self::ClientKind(client_kind) => model.client_kind() == client_kind,
        }
    }

    pub fn runtime_provider(self) -> Option<LlmProvider> {
        match self {
            Self::Provider(provider) => Some(provider.as_llm_provider()),
            Self::ClientKind(ClientKind::CodexCli | ClientKind::OpenCodeCli) => {
                Some(LlmProvider::OpenAI)
            }
            Self::ClientKind(ClientKind::GeminiCli) => Some(LlmProvider::Google),
            Self::ClientKind(ClientKind::ClaudeCodeCli) => Some(LlmProvider::Anthropic),
            Self::ClientKind(ClientKind::Http) => None,
        }
    }
}

pub fn parse_provider_selector(value: &str) -> Option<ProviderSelector> {
    let normalized = normalize_identifier(value);
    let special = match normalized.as_str() {
        "claude-code" | "claudecode" => Some(ProviderSelector::Provider(Provider::ClaudeCode)),
        "codex" | "codex-cli" | "codexcli" | "openai-codex" | "openaicodex" => {
            Some(ProviderSelector::Provider(Provider::Codex))
        }
        "opencode" | "opencode-cli" | "opencodecli" => {
            Some(ProviderSelector::ClientKind(ClientKind::OpenCodeCli))
        }
        "gemini-cli" | "geminicli" => Some(ProviderSelector::ClientKind(ClientKind::GeminiCli)),
        _ => None,
    };
    special.or_else(|| {
        ModelProvider::parse_alias(value)
            .map(Provider::from_model_provider)
            .map(ProviderSelector::Provider)
    })
}

pub fn split_provider_qualified_model(value: &str) -> Option<(ProviderSelector, &str)> {
    for separator in [':', '/'] {
        let Some((provider_raw, model_raw)) = value.split_once(separator) else {
            continue;
        };
        let model_raw = model_raw.trim();
        if model_raw.is_empty() {
            continue;
        }
        if let Some(provider) = parse_provider_selector(provider_raw) {
            return Some((provider, model_raw));
        }
    }

    None
}

pub fn parse_model_reference(value: &str) -> Option<ModelId> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    parse_plain_model_reference(trimmed).or_else(|| {
        let (selector, model_raw) = split_provider_qualified_model(trimmed)?;
        match selector {
            ProviderSelector::Provider(provider) => {
                ModelId::for_provider_and_model(provider, model_raw)
            }
            ProviderSelector::ClientKind(client_kind) => parse_plain_model_reference(model_raw)
                .filter(|model| model.client_kind() == client_kind),
        }
    })
}

pub fn resolve_available_model_name(requested: &str, available: &[String]) -> Option<String> {
    let trimmed = requested.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(exact) = find_case_insensitive_match(available, trimmed) {
        return Some(exact);
    }

    if let Some(model) = parse_model_reference(trimmed)
        && let Some(resolved) = find_available_model_for_id(available, model)
    {
        return Some(resolved);
    }

    let normalized = normalize_identifier(trimmed);
    if normalized.is_empty() {
        return None;
    }

    let normalized_matches = available
        .iter()
        .filter(|candidate| normalize_identifier(candidate) == normalized)
        .collect::<Vec<_>>();
    if normalized_matches.len() == 1 {
        return Some(normalized_matches[0].clone());
    }

    let prefix_matches = available
        .iter()
        .filter(|candidate| normalize_identifier(candidate).starts_with(&normalized))
        .collect::<Vec<_>>();
    if prefix_matches.is_empty() {
        return None;
    }

    let mut sorted_matches = prefix_matches.into_iter().cloned().collect::<Vec<_>>();
    sorted_matches.sort();
    sorted_matches.into_iter().next()
}

fn parse_plain_model_reference(value: &str) -> Option<ModelId> {
    ModelId::from_api_name(value)
        .or_else(|| ModelId::from_canonical_id(value))
        .or_else(|| ModelId::from_serialized_str(value))
}

fn find_available_model_for_id(available: &[String], model: ModelId) -> Option<String> {
    find_case_insensitive_match(available, model.as_serialized_str())
        .or_else(|| find_case_insensitive_match(available, model.as_str()))
        .or_else(|| {
            available.iter().find_map(|candidate| {
                (parse_plain_model_reference(candidate) == Some(model)).then(|| candidate.clone())
            })
        })
}

fn find_case_insensitive_match(available: &[String], requested: &str) -> Option<String> {
    available
        .iter()
        .find(|candidate| candidate.eq_ignore_ascii_case(requested))
        .cloned()
}

fn normalize_identifier(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut previous_dash = false;

    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
            previous_dash = false;
            continue;
        }
        if !previous_dash {
            normalized.push('-');
            previous_dash = true;
        }
    }

    normalized.trim_matches('-').to_string()
}
