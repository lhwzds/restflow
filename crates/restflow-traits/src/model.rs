//! Shared model/provider primitives for cross-crate normalization.

use serde::{Deserialize, Deserializer, Serialize};

macro_rules! define_model_provider {
    ($($variant:ident => { canonical: $canonical:literal, key: $key:literal, aliases: [$($alias:literal),* $(,)?] }),+ $(,)?) => {
        /// Canonical model provider identity shared by runtime and tooling layers.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
        #[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
        #[cfg_attr(feature = "specta", derive(specta::Type))]
        pub enum ModelProvider {
            $(
                #[serde(rename = $canonical)]
                #[cfg_attr(feature = "ts", ts(rename = $canonical))]
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
