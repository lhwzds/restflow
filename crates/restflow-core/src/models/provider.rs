use restflow_models::{LlmProvider, provider_meta};
use restflow_traits::ModelProvider;
use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;
use ts_rs::TS;

use super::{catalog, model_id::ModelId};

macro_rules! define_provider_enum {
    ($($variant:ident => $rename:literal),+ $(,)?) => {
        /// AI model provider
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, TS, Type)]
        #[ts(export)]
        pub enum Provider {
            $(
                #[serde(rename = $rename)]
                #[ts(rename = $rename)]
                $variant,
            )+
        }

        impl Provider {
            pub fn all() -> &'static [Provider] {
                &[
                    $(Self::$variant,)+
                ]
            }

            /// Convert to shared provider identity used by cross-crate parsers.
            pub fn as_model_provider(&self) -> ModelProvider {
                match *self {
                    $(Self::$variant => ModelProvider::$variant,)+
                }
            }

            /// Convert from shared provider identity.
            pub fn from_model_provider(provider: ModelProvider) -> Self {
                match provider {
                    $(ModelProvider::$variant => Self::$variant,)+
                }
            }
        }
    };
}

define_provider_enum! {
    OpenAI => "openai",
    Anthropic => "anthropic",
    ClaudeCode => "claude-code",
    Codex => "codex",
    DeepSeek => "deepseek",
    Google => "google",
    Groq => "groq",
    OpenRouter => "openrouter",
    XAI => "xai",
    Qwen => "qwen",
    Zai => "zai",
    ZaiCodingPlan => "zai-coding-plan",
    Moonshot => "moonshot",
    Doubao => "doubao",
    Yi => "yi",
    SiliconFlow => "siliconflow",
    MiniMax => "minimax",
    MiniMaxCodingPlan => "minimax-coding-plan",
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

impl Provider {
    pub fn api_key_env(&self) -> Option<&'static str> {
        provider_meta(self.as_model_provider()).api_key_env
    }

    /// Convert Provider to LLM provider used by runtime factory.
    pub fn as_llm_provider(&self) -> LlmProvider {
        provider_meta(self.as_model_provider()).runtime_provider
    }

    /// Get the canonical provider identifier for use in canonical model IDs.
    /// Returns lowercase provider name (e.g., "openai", "anthropic").
    pub fn as_canonical_str(&self) -> &'static str {
        provider_meta(self.as_model_provider()).canonical_name()
    }

    /// Parse a canonical provider string back to Provider.
    /// Returns None if the string is not recognized.
    pub fn from_canonical_str(s: &str) -> Option<Self> {
        ModelProvider::parse_alias(s).map(Self::from_model_provider)
    }

    /// Get the best available model for this provider.
    pub fn flagship_model(&self) -> ModelId {
        catalog::provider_catalog(*self)
            .map(|catalog| catalog.flagship)
            .unwrap_or_else(|| panic!("missing provider catalog for {}", self.as_canonical_str()))
    }
}
