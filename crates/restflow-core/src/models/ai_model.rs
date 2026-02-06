use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// AI model provider
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    OpenAI,
    Anthropic,
    DeepSeek,
}

/// Model metadata containing provider, temperature support, and display name
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelMetadata {
    pub provider: Provider,
    pub supports_temperature: bool,
    pub name: &'static str,
}

/// Serializable model metadata for transferring to frontend
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ModelMetadataDTO {
    pub model: AIModel,
    pub provider: Provider,
    pub supports_temperature: bool,
    pub name: String,
}

/// AI model enum - Single Source of Truth for all supported models
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum AIModel {
    // OpenAI GPT-5 series (no temperature support)
    #[serde(rename = "gpt-5")]
    Gpt5,
    #[serde(rename = "gpt-5-mini")]
    Gpt5Mini,
    #[serde(rename = "gpt-5-nano")]
    Gpt5Nano,
    #[serde(rename = "gpt-5-pro")]
    Gpt5Pro,

    // OpenAI O-series (no temperature support)
    #[serde(rename = "o4-mini")]
    O4Mini,
    #[serde(rename = "o3")]
    O3,
    #[serde(rename = "o3-mini")]
    O3Mini,

    // Anthropic Claude series (latest models only, for direct API)
    #[serde(rename = "claude-opus-4-1")]
    ClaudeOpus4_1,
    #[serde(rename = "claude-sonnet-4-5")]
    ClaudeSonnet4_5,
    #[serde(rename = "claude-haiku-4-5")]
    ClaudeHaiku4_5,

    // Claude Code CLI aliases (for use with claude CLI tool)
    #[serde(rename = "claude-code-opus")]
    ClaudeCodeOpus,
    #[serde(rename = "claude-code-sonnet")]
    ClaudeCodeSonnet,
    #[serde(rename = "claude-code-haiku")]
    ClaudeCodeHaiku,

    // DeepSeek series
    #[serde(rename = "deepseek-chat")]
    DeepseekChat,
    #[serde(rename = "deepseek-reasoner")]
    DeepseekReasoner,

    // Codex CLI (OpenAI)
    #[serde(rename = "codex-cli")]
    CodexCli,

    // OpenCode CLI (multi-provider)
    #[serde(rename = "opencode-cli")]
    OpenCodeCli,
}

impl AIModel {
    /// Get comprehensive metadata for this model
    pub fn metadata(&self) -> ModelMetadata {
        match self {
            // GPT-5 series (no temperature support)
            Self::Gpt5 => ModelMetadata {
                provider: Provider::OpenAI,
                supports_temperature: false,
                name: "GPT-5",
            },
            Self::Gpt5Mini => ModelMetadata {
                provider: Provider::OpenAI,
                supports_temperature: false,
                name: "GPT-5 Mini",
            },
            Self::Gpt5Nano => ModelMetadata {
                provider: Provider::OpenAI,
                supports_temperature: false,
                name: "GPT-5 Nano",
            },
            Self::Gpt5Pro => ModelMetadata {
                provider: Provider::OpenAI,
                supports_temperature: false,
                name: "GPT-5 Pro",
            },

            // O-series (no temperature support)
            Self::O4Mini => ModelMetadata {
                provider: Provider::OpenAI,
                supports_temperature: false,
                name: "O4 Mini",
            },
            Self::O3 => ModelMetadata {
                provider: Provider::OpenAI,
                supports_temperature: false,
                name: "O3",
            },
            Self::O3Mini => ModelMetadata {
                provider: Provider::OpenAI,
                supports_temperature: false,
                name: "O3 Mini",
            },

            // Claude series
            Self::ClaudeOpus4_1 => ModelMetadata {
                provider: Provider::Anthropic,
                supports_temperature: true,
                name: "Claude Opus 4.1",
            },
            Self::ClaudeSonnet4_5 => ModelMetadata {
                provider: Provider::Anthropic,
                supports_temperature: true,
                name: "Claude Sonnet 4.5",
            },
            Self::ClaudeHaiku4_5 => ModelMetadata {
                provider: Provider::Anthropic,
                supports_temperature: true,
                name: "Claude Haiku 4.5",
            },

            // Claude Code CLI aliases
            Self::ClaudeCodeOpus => ModelMetadata {
                provider: Provider::Anthropic,
                supports_temperature: true,
                name: "Claude Code Opus",
            },
            Self::ClaudeCodeSonnet => ModelMetadata {
                provider: Provider::Anthropic,
                supports_temperature: true,
                name: "Claude Code Sonnet",
            },
            Self::ClaudeCodeHaiku => ModelMetadata {
                provider: Provider::Anthropic,
                supports_temperature: true,
                name: "Claude Code Haiku",
            },

            // DeepSeek series
            Self::DeepseekChat => ModelMetadata {
                provider: Provider::DeepSeek,
                supports_temperature: true,
                name: "DeepSeek Chat",
            },
            Self::DeepseekReasoner => ModelMetadata {
                provider: Provider::DeepSeek,
                supports_temperature: true,
                name: "DeepSeek Reasoner",
            },

            // Codex CLI
            Self::CodexCli => ModelMetadata {
                provider: Provider::OpenAI,
                supports_temperature: false,
                name: "Codex CLI",
            },

            // OpenCode CLI
            Self::OpenCodeCli => ModelMetadata {
                provider: Provider::OpenAI,
                supports_temperature: false,
                name: "OpenCode CLI",
            },
        }
    }

    /// Get the provider for this model
    pub fn provider(&self) -> Provider {
        self.metadata().provider
    }

    /// Check if this model supports temperature parameter
    pub fn supports_temperature(&self) -> bool {
        self.metadata().supports_temperature
    }

    /// Get the string representation used for API calls
    pub fn as_str(&self) -> &'static str {
        match self {
            // GPT-5 series
            Self::Gpt5 => "gpt-5",
            Self::Gpt5Mini => "gpt-5-mini",
            Self::Gpt5Nano => "gpt-5-nano",
            Self::Gpt5Pro => "gpt-5-pro",

            // O-series
            Self::O4Mini => "o4-mini",
            Self::O3 => "o3",
            Self::O3Mini => "o3-mini",

            // Claude series (direct API)
            Self::ClaudeOpus4_1 => "claude-opus-4-1",
            Self::ClaudeSonnet4_5 => "claude-sonnet-4-5",
            Self::ClaudeHaiku4_5 => "claude-haiku-4-5",

            // Claude Code CLI (aliases for claude CLI tool)
            Self::ClaudeCodeOpus => "opus",
            Self::ClaudeCodeSonnet => "sonnet",
            Self::ClaudeCodeHaiku => "haiku",

            // DeepSeek series
            Self::DeepseekChat => "deepseek-chat",
            Self::DeepseekReasoner => "deepseek-reasoner",

            // Codex CLI
            Self::CodexCli => "gpt-5.3-codex",

            // OpenCode CLI
            Self::OpenCodeCli => "opencode",
        }
    }

    /// Convert an API model name into an AIModel.
    pub fn from_api_name(name: &str) -> Option<Self> {
        if let Some(model) = Self::all().iter().find(|m| m.as_str() == name).copied() {
            return Some(model);
        }

        match name {
            "claude-sonnet-4-5-20250514" | "claude-sonnet-4-20250514" => {
                Some(Self::ClaudeSonnet4_5)
            }
            "claude-opus-4-1-20250805" | "claude-opus-4-1-20250514" => {
                Some(Self::ClaudeOpus4_1)
            }
            "claude-haiku-4-5-20250514" | "claude-haiku-4-20250514" => {
                Some(Self::ClaudeHaiku4_5)
            }
            _ => {
                if name.starts_with("claude-sonnet-4") {
                    Some(Self::ClaudeSonnet4_5)
                } else if name.starts_with("claude-opus-4-1") || name.starts_with("claude-opus-4") {
                    Some(Self::ClaudeOpus4_1)
                } else if name.starts_with("claude-haiku-4") {
                    Some(Self::ClaudeHaiku4_5)
                } else {
                    None
                }
            }
        }
    }

    /// Get the display name for UI
    pub fn display_name(&self) -> &'static str {
        self.metadata().name
    }

    /// Get the serialized string representation (serde rename)
    pub fn as_serialized_str(&self) -> &'static str {
        match self {
            // GPT-5 series
            Self::Gpt5 => "gpt-5",
            Self::Gpt5Mini => "gpt-5-mini",
            Self::Gpt5Nano => "gpt-5-nano",
            Self::Gpt5Pro => "gpt-5-pro",

            // O-series
            Self::O4Mini => "o4-mini",
            Self::O3 => "o3",
            Self::O3Mini => "o3-mini",

            // Claude series (direct API)
            Self::ClaudeOpus4_1 => "claude-opus-4-1",
            Self::ClaudeSonnet4_5 => "claude-sonnet-4-5",
            Self::ClaudeHaiku4_5 => "claude-haiku-4-5",

            // Claude Code CLI aliases
            Self::ClaudeCodeOpus => "claude-code-opus",
            Self::ClaudeCodeSonnet => "claude-code-sonnet",
            Self::ClaudeCodeHaiku => "claude-code-haiku",

            // Codex CLI
            Self::CodexCli => "codex-cli",

            // OpenCode CLI
            Self::OpenCodeCli => "opencode-cli",

            // DeepSeek series
            Self::DeepseekChat => "deepseek-chat",
            Self::DeepseekReasoner => "deepseek-reasoner",
        }
    }

    /// Check if this model uses the Codex CLI
    pub fn is_codex_cli(&self) -> bool {
        matches!(self, Self::CodexCli)
    }

    /// Check if this model uses the Claude Code CLI
    pub fn is_claude_code(&self) -> bool {
        matches!(
            self,
            Self::ClaudeCodeOpus | Self::ClaudeCodeSonnet | Self::ClaudeCodeHaiku
        )
    }

    /// Check if this model uses the OpenCode CLI
    pub fn is_opencode_cli(&self) -> bool {
        matches!(self, Self::OpenCodeCli)
    }

    /// Get all available models as a slice
    pub fn all() -> &'static [AIModel] {
        &[
            // OpenAI
            Self::Gpt5,
            Self::Gpt5Mini,
            Self::Gpt5Nano,
            Self::Gpt5Pro,
            Self::O4Mini,
            Self::O3,
            Self::O3Mini,
            // Anthropic (direct API)
            Self::ClaudeOpus4_1,
            Self::ClaudeSonnet4_5,
            Self::ClaudeHaiku4_5,
            // Anthropic (Claude Code CLI)
            Self::ClaudeCodeOpus,
            Self::ClaudeCodeSonnet,
            Self::ClaudeCodeHaiku,
            // DeepSeek
            Self::DeepseekChat,
            Self::DeepseekReasoner,
            // Codex CLI
            Self::CodexCli,
            // OpenCode CLI
            Self::OpenCodeCli,
        ]
    }

    /// Convert metadata to serializable DTO for frontend
    pub fn to_metadata_dto(&self) -> ModelMetadataDTO {
        let metadata = self.metadata();
        ModelMetadataDTO {
            model: *self,
            provider: metadata.provider,
            supports_temperature: metadata.supports_temperature,
            name: metadata.name.to_string(),
        }
    }

    /// Get all models with their metadata as DTOs
    pub fn all_with_metadata() -> Vec<ModelMetadataDTO> {
        Self::all()
            .iter()
            .map(|model| model.to_metadata_dto())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider() {
        assert_eq!(AIModel::Gpt5.provider(), Provider::OpenAI);
        assert_eq!(AIModel::ClaudeSonnet4_5.provider(), Provider::Anthropic);
        assert_eq!(AIModel::DeepseekChat.provider(), Provider::DeepSeek);
    }

    #[test]
    fn test_supports_temperature() {
        // Models that don't support temperature
        assert!(!AIModel::Gpt5.supports_temperature());
        assert!(!AIModel::Gpt5Mini.supports_temperature());
        assert!(!AIModel::O3.supports_temperature());
        assert!(!AIModel::O4Mini.supports_temperature());
        assert!(!AIModel::CodexCli.supports_temperature());
        assert!(!AIModel::OpenCodeCli.supports_temperature());

        // Models that support temperature
        assert!(AIModel::ClaudeSonnet4_5.supports_temperature());
        assert!(AIModel::ClaudeHaiku4_5.supports_temperature());
        assert!(AIModel::DeepseekChat.supports_temperature());
    }

    #[test]
    fn test_is_codex_cli() {
        assert!(AIModel::CodexCli.is_codex_cli());
        assert!(!AIModel::Gpt5.is_codex_cli());
    }

    #[test]
    fn test_is_opencode_cli() {
        assert!(AIModel::OpenCodeCli.is_opencode_cli());
        assert!(!AIModel::Gpt5.is_opencode_cli());
    }

    #[test]
    fn test_as_str() {
        assert_eq!(AIModel::Gpt5.as_str(), "gpt-5");
        assert_eq!(AIModel::O3.as_str(), "o3");
        assert_eq!(AIModel::ClaudeSonnet4_5.as_str(), "claude-sonnet-4-5");
        assert_eq!(AIModel::ClaudeHaiku4_5.as_str(), "claude-haiku-4-5");
        assert_eq!(AIModel::CodexCli.as_str(), "gpt-5.3-codex");
        assert_eq!(AIModel::OpenCodeCli.as_str(), "opencode");
        assert_eq!(AIModel::DeepseekChat.as_str(), "deepseek-chat");
    }

    #[test]
    fn test_from_api_name() {
        assert_eq!(
            AIModel::from_api_name("claude-sonnet-4-5-20250514"),
            Some(AIModel::ClaudeSonnet4_5)
        );
        assert_eq!(
            AIModel::from_api_name("claude-sonnet-4-20250514"),
            Some(AIModel::ClaudeSonnet4_5)
        );
        assert_eq!(AIModel::from_api_name("nonexistent"), None);
    }

    #[test]
    fn test_display_name() {
        assert_eq!(AIModel::Gpt5.display_name(), "GPT-5");
        assert_eq!(AIModel::ClaudeSonnet4_5.display_name(), "Claude Sonnet 4.5");
        assert_eq!(AIModel::ClaudeHaiku4_5.display_name(), "Claude Haiku 4.5");
        assert_eq!(AIModel::CodexCli.display_name(), "Codex CLI");
        assert_eq!(AIModel::OpenCodeCli.display_name(), "OpenCode CLI");
        assert_eq!(AIModel::DeepseekChat.display_name(), "DeepSeek Chat");
    }

    #[test]
    fn test_all_models() {
        let models = AIModel::all();
        assert_eq!(models.len(), 17);
        assert!(models.contains(&AIModel::Gpt5));
        assert!(models.contains(&AIModel::O3));
        assert!(models.contains(&AIModel::ClaudeOpus4_1));
        assert!(models.contains(&AIModel::ClaudeSonnet4_5));
        assert!(models.contains(&AIModel::ClaudeHaiku4_5));
        assert!(models.contains(&AIModel::CodexCli));
        assert!(models.contains(&AIModel::OpenCodeCli));
        assert!(models.contains(&AIModel::DeepseekChat));
    }

    #[test]
    fn test_metadata() {
        // Test metadata for GPT-5 (no temperature)
        let metadata = AIModel::Gpt5.metadata();
        assert_eq!(metadata.provider, Provider::OpenAI);
        assert!(!metadata.supports_temperature);
        assert_eq!(metadata.name, "GPT-5");

        // Test metadata for Claude Sonnet 4.5 (with temperature)
        let metadata = AIModel::ClaudeSonnet4_5.metadata();
        assert_eq!(metadata.provider, Provider::Anthropic);
        assert!(metadata.supports_temperature);
        assert_eq!(metadata.name, "Claude Sonnet 4.5");

        // Test metadata for DeepSeek Chat
        let metadata = AIModel::DeepseekChat.metadata();
        assert_eq!(metadata.provider, Provider::DeepSeek);
        assert!(metadata.supports_temperature);
        assert_eq!(metadata.name, "DeepSeek Chat");
    }
}
