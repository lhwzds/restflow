use restflow_models::{LlmProvider, provider_meta};
use restflow_traits::ModelProvider;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use specta::Type;
use ts_rs::TS;

use super::{catalog, model_id::ModelId};

/// API-facing provider wrapper backed by the shared canonical provider identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, TS, Type)]
#[repr(transparent)]
#[ts(export, as = "ModelProvider")]
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
