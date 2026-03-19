//! Shared model/provider primitives for cross-crate normalization.

/// Canonical model provider identity shared by runtime and tooling layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelProvider {
    OpenAI,
    Anthropic,
    ClaudeCode,
    Codex,
    DeepSeek,
    Google,
    Groq,
    OpenRouter,
    XAI,
    Qwen,
    Zai,
    ZaiCodingPlan,
    Moonshot,
    Doubao,
    Yi,
    SiliconFlow,
    MiniMax,
    MiniMaxCodingPlan,
}

impl ModelProvider {
    /// Return canonical provider string used by config and API payloads.
    pub fn canonical_str(self) -> &'static str {
        match self {
            Self::OpenAI => "openai",
            Self::Anthropic => "anthropic",
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
            Self::DeepSeek => "deepseek",
            Self::Google => "google",
            Self::Groq => "groq",
            Self::OpenRouter => "openrouter",
            Self::XAI => "xai",
            Self::Qwen => "qwen",
            Self::Zai => "zai",
            Self::ZaiCodingPlan => "zai-coding-plan",
            Self::Moonshot => "moonshot",
            Self::Doubao => "doubao",
            Self::Yi => "yi",
            Self::SiliconFlow => "siliconflow",
            Self::MiniMax => "minimax",
            Self::MiniMaxCodingPlan => "minimax-coding-plan",
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
            "openai" | "gpt" => Some(Self::OpenAI),
            "anthropic" | "claude" => Some(Self::Anthropic),
            "claudecode" | "claudecodecli" => Some(Self::ClaudeCode),
            "codex" | "openaicodex" | "openaicodexcli" => Some(Self::Codex),
            "deepseek" => Some(Self::DeepSeek),
            "google" | "gemini" => Some(Self::Google),
            "groq" => Some(Self::Groq),
            "openrouter" => Some(Self::OpenRouter),
            "xai" | "xaiapi" | "grok" => Some(Self::XAI),
            "qwen" => Some(Self::Qwen),
            "zai" | "zhipu" => Some(Self::Zai),
            "zaicodingplan" | "zaicoding" | "zhipucodingplan" => Some(Self::ZaiCodingPlan),
            "moonshot" | "kimi" => Some(Self::Moonshot),
            "doubao" | "ark" => Some(Self::Doubao),
            "yi" => Some(Self::Yi),
            "siliconflow" => Some(Self::SiliconFlow),
            "minimax" => Some(Self::MiniMax),
            "minimaxcodingplan" | "minimaxcoding" => Some(Self::MiniMaxCodingPlan),
            _ => None,
        }
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
}
