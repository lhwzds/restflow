//! Shared model/provider primitives for cross-crate normalization.

use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;

use crate::request::WireModelRef;
use crate::{ModelId, Provider, ValidationError};

/// Model metadata containing provider, temperature support, and display name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelMetadata {
    pub provider: Provider,
    pub supports_temperature: bool,
    pub name: &'static str,
}

/// Serializable model metadata for runtime clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ModelMetadataDTO {
    pub model: ModelId,
    pub provider: Provider,
    pub supports_temperature: bool,
    pub name: String,
}

/// Provider + model pair used by API and persistence layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ModelRef {
    pub provider: Provider,
    pub model: ModelId,
}

impl ModelRef {
    /// Build a consistent model reference from a model enum.
    pub fn from_model(model: ModelId) -> Self {
        Self {
            provider: model.provider(),
            model,
        }
    }

    /// Validate that provider and model provider metadata are consistent.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let expected_provider = self.model.provider();
        if self.provider != expected_provider {
            return Err(ValidationError::new(
                "model_ref",
                format!(
                    "provider '{}' does not match model provider '{}'",
                    self.provider.as_canonical_str(),
                    expected_provider.as_canonical_str()
                ),
            ));
        }
        Ok(())
    }

    /// Return canonical ID in `provider:model` format.
    pub fn canonical_id(&self) -> String {
        format!(
            "{}:{}",
            self.provider.as_canonical_str(),
            self.model.as_serialized_str()
        )
    }
}

impl TryFrom<WireModelRef> for ModelRef {
    type Error = ValidationError;

    fn try_from(value: WireModelRef) -> Result<Self, Self::Error> {
        let provider = Provider::from_canonical_str(&value.provider).ok_or_else(|| {
            ValidationError::new(
                "model_ref.provider",
                format!("unknown provider '{}'", value.provider),
            )
        })?;
        let model = ModelId::for_provider_and_model(provider, &value.model).ok_or_else(|| {
            ValidationError::new(
                "model_ref.model",
                format!("unknown model '{}'", value.model),
            )
        })?;

        let model_ref = Self { provider, model };
        model_ref.validate()?;
        Ok(model_ref)
    }
}

impl From<ModelRef> for WireModelRef {
    fn from(value: ModelRef) -> Self {
        Self {
            provider: value.provider.as_canonical_str().to_string(),
            model: value.model.as_serialized_str().to_string(),
        }
    }
}

macro_rules! define_model_provider {
    ($($variant:ident => { canonical: $canonical:literal, key: $key:literal, aliases: [$($alias:literal),* $(,)?] }),+ $(,)?) => {
        /// Canonical model provider identity shared by runtime and tooling layers.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Type)]
        pub enum ModelProvider {
            $(
                #[serde(rename = $canonical)]
                $variant,
            )+
        }

        impl ModelProvider {
            /// Return all canonical providers in a stable order.
            pub fn all() -> &'static [Self] {
                &[
                    $(Self::$variant,)+
                ]
            }

            /// Return canonical provider string used by config and API payloads.
            pub fn canonical_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $canonical,)+
                }
            }

            /// Parse user input/provider aliases into canonical provider identity.
            pub fn parse_alias(value: &str) -> Option<Self> {
                let normalized: String = value
                    .trim()
                    .chars()
                    .filter(|ch| ch.is_ascii_alphanumeric())
                    .collect::<String>()
                    .to_ascii_lowercase();

                match normalized.as_str() {
                    $(
                        $key => Some(Self::$variant),
                        $($alias => Some(Self::$variant),)*
                    )+
                    _ => None,
                }
            }
        }
    };
}

define_model_provider! {
    OpenAI => { canonical: "openai", key: "openai", aliases: ["gpt"] },
    Anthropic => { canonical: "anthropic", key: "anthropic", aliases: ["claude"] },
    ClaudeCode => { canonical: "claude-code", key: "claudecode", aliases: ["claudecodecli"] },
    Codex => { canonical: "codex", key: "codex", aliases: ["openaicodex", "openaicodexcli"] },
    DeepSeek => { canonical: "deepseek", key: "deepseek", aliases: [] },
    Google => { canonical: "google", key: "google", aliases: ["gemini"] },
    Groq => { canonical: "groq", key: "groq", aliases: [] },
    OpenRouter => { canonical: "openrouter", key: "openrouter", aliases: [] },
    XAI => { canonical: "xai", key: "xai", aliases: ["xaiapi", "grok"] },
    Qwen => { canonical: "qwen", key: "qwen", aliases: [] },
    Zai => { canonical: "zai", key: "zai", aliases: ["zhipu"] },
    ZaiCodingPlan => { canonical: "zai-coding-plan", key: "zaicodingplan", aliases: ["zaicoding", "zhipucodingplan"] },
    Moonshot => { canonical: "moonshot", key: "moonshot", aliases: ["kimi"] },
    Doubao => { canonical: "doubao", key: "doubao", aliases: ["ark"] },
    Yi => { canonical: "yi", key: "yi", aliases: [] },
    SiliconFlow => { canonical: "siliconflow", key: "siliconflow", aliases: [] },
    MiniMax => { canonical: "minimax", key: "minimax", aliases: [] },
    MiniMaxCodingPlan => { canonical: "minimax-coding-plan", key: "minimaxcodingplan", aliases: ["minimaxcoding"] },
}

impl<'de> Deserialize<'de> for ModelProvider {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse_alias(&raw)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown provider: {raw}")))
    }
}

#[cfg(test)]
mod tests {
    use super::ModelProvider;

    #[test]
    fn parse_alias_supports_common_shortcuts() {
        assert_eq!(
            ModelProvider::parse_alias("gpt"),
            Some(ModelProvider::OpenAI)
        );
        assert_eq!(
            ModelProvider::parse_alias("gemini"),
            Some(ModelProvider::Google)
        );
        assert_eq!(
            ModelProvider::parse_alias("claude-code"),
            Some(ModelProvider::ClaudeCode)
        );
        assert_eq!(
            ModelProvider::parse_alias("openai-codex"),
            Some(ModelProvider::Codex)
        );
        assert_eq!(
            ModelProvider::parse_alias("zai-coding"),
            Some(ModelProvider::ZaiCodingPlan)
        );
        assert_eq!(
            ModelProvider::parse_alias("minimax_coding"),
            Some(ModelProvider::MiniMaxCodingPlan)
        );
    }

    #[test]
    fn canonical_str_is_stable() {
        assert_eq!(ModelProvider::OpenAI.canonical_str(), "openai");
        assert_eq!(ModelProvider::ClaudeCode.canonical_str(), "claude-code");
        assert_eq!(ModelProvider::Codex.canonical_str(), "codex");
        assert_eq!(
            ModelProvider::ZaiCodingPlan.canonical_str(),
            "zai-coding-plan"
        );
        assert_eq!(
            ModelProvider::MiniMaxCodingPlan.canonical_str(),
            "minimax-coding-plan"
        );
    }

    #[test]
    fn deserialize_accepts_aliases() {
        let parsed: ModelProvider = serde_json::from_str("\"gpt\"").unwrap();
        assert_eq!(parsed, ModelProvider::OpenAI);

        let parsed: ModelProvider = serde_json::from_str("\"openai-codex\"").unwrap();
        assert_eq!(parsed, ModelProvider::Codex);
    }
}
