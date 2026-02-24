use restflow_ai::llm::{LlmProvider, ModelSpec};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// AI model provider
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    OpenAI,
    Anthropic,
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

impl Provider {
    pub fn all() -> &'static [Provider] {
        &[
            Self::OpenAI,
            Self::Anthropic,
            Self::DeepSeek,
            Self::Google,
            Self::Groq,
            Self::OpenRouter,
            Self::XAI,
            Self::Qwen,
            Self::Zai,
            Self::ZaiCodingPlan,
            Self::Moonshot,
            Self::Doubao,
            Self::Yi,
            Self::SiliconFlow,
            Self::MiniMax,
            Self::MiniMaxCodingPlan,
        ]
    }

    pub fn api_key_env(&self) -> &'static str {
        match self {
            Self::OpenAI => "OPENAI_API_KEY",
            Self::Anthropic => "ANTHROPIC_API_KEY",
            Self::DeepSeek => "DEEPSEEK_API_KEY",
            Self::Google => "GEMINI_API_KEY",
            Self::Groq => "GROQ_API_KEY",
            Self::OpenRouter => "OPENROUTER_API_KEY",
            Self::XAI => "XAI_API_KEY",
            Self::Qwen => "DASHSCOPE_API_KEY",
            Self::Zai => "ZAI_API_KEY",
            Self::ZaiCodingPlan => "ZAI_CODING_PLAN_API_KEY",
            Self::Moonshot => "MOONSHOT_API_KEY",
            Self::Doubao => "ARK_API_KEY",
            Self::Yi => "YI_API_KEY",
            Self::SiliconFlow => "SILICONFLOW_API_KEY",
            Self::MiniMax => "MINIMAX_API_KEY",
            Self::MiniMaxCodingPlan => "MINIMAX_CODING_PLAN_API_KEY",
        }
    }

    /// Convert Provider to LLM provider used by runtime factory.
    pub fn as_llm_provider(&self) -> LlmProvider {
        match self {
            Self::OpenAI => LlmProvider::OpenAI,
            Self::Anthropic => LlmProvider::Anthropic,
            Self::DeepSeek => LlmProvider::DeepSeek,
            Self::Google => LlmProvider::Google,
            Self::Groq => LlmProvider::Groq,
            Self::OpenRouter => LlmProvider::OpenRouter,
            Self::XAI => LlmProvider::XAI,
            Self::Qwen => LlmProvider::Qwen,
            Self::Zai => LlmProvider::Zai,
            Self::ZaiCodingPlan => LlmProvider::ZaiCodingPlan,
            Self::Moonshot => LlmProvider::Moonshot,
            Self::Doubao => LlmProvider::Doubao,
            Self::Yi => LlmProvider::Yi,
            Self::SiliconFlow => LlmProvider::SiliconFlow,
            Self::MiniMax => LlmProvider::MiniMax,
            Self::MiniMaxCodingPlan => LlmProvider::MiniMaxCodingPlan,
        }
    }

    /// Get the canonical provider identifier for use in canonical model IDs.
    /// Returns lowercase provider name (e.g., "openai", "anthropic").
    pub fn as_canonical_str(&self) -> &'static str {
        match self {
            Self::OpenAI => "openai",
            Self::Anthropic => "anthropic",
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

    /// Parse a canonical provider string back to Provider.
    /// Returns None if the string is not recognized.
    pub fn from_canonical_str(s: &str) -> Option<Self> {
        match s {
            "openai" => Some(Self::OpenAI),
            "anthropic" => Some(Self::Anthropic),
            "deepseek" => Some(Self::DeepSeek),
            "google" => Some(Self::Google),
            "groq" => Some(Self::Groq),
            "openrouter" => Some(Self::OpenRouter),
            "xai" => Some(Self::XAI),
            "qwen" => Some(Self::Qwen),
            "zai" => Some(Self::Zai),
            "zai-coding-plan" => Some(Self::ZaiCodingPlan),
            "moonshot" => Some(Self::Moonshot),
            "doubao" => Some(Self::Doubao),
            "yi" => Some(Self::Yi),
            "siliconflow" => Some(Self::SiliconFlow),
            "minimax" => Some(Self::MiniMax),
            "minimax-coding-plan" => Some(Self::MiniMaxCodingPlan),
            _ => None,
        }
    }

    /// Get the best available model for this provider.
    pub fn flagship_model(&self) -> AIModel {
        match self {
            Self::OpenAI => AIModel::Gpt5,
            Self::Anthropic => AIModel::ClaudeSonnet4_5,
            Self::DeepSeek => AIModel::DeepseekChat,
            Self::Google => AIModel::Gemini3Pro,
            Self::Groq => AIModel::GroqLlama4Maverick,
            Self::OpenRouter => AIModel::OrClaudeOpus4_6,
            Self::XAI => AIModel::Grok4,
            Self::Qwen => AIModel::Qwen3Max,
            Self::Zai => AIModel::Glm5,
            Self::ZaiCodingPlan => AIModel::Glm5CodingPlan,
            Self::Moonshot => AIModel::KimiK2_5,
            Self::Doubao => AIModel::DoubaoPro,
            Self::Yi => AIModel::YiLightning,
            Self::SiliconFlow => AIModel::SiliconFlowAuto,
            Self::MiniMax => AIModel::MiniMaxM25,
            Self::MiniMaxCodingPlan => AIModel::MiniMaxM25CodingPlan,
        }
    }
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
    #[serde(rename = "gpt-5-1")]
    Gpt5_1,
    #[serde(rename = "gpt-5-2")]
    Gpt5_2,

    // Anthropic Claude series (latest models only, for direct API)
    #[serde(rename = "claude-opus-4-6")]
    ClaudeOpus4_6,
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

    // Google Gemini (OpenAI-compatible endpoint)
    #[serde(rename = "gemini-2-5-pro")]
    Gemini25Pro,
    #[serde(rename = "gemini-2-5-flash")]
    Gemini25Flash,
    #[serde(rename = "gemini-3-pro")]
    Gemini3Pro,
    #[serde(rename = "gemini-3-flash")]
    Gemini3Flash,

    // Groq
    #[serde(rename = "groq-llama4-scout")]
    GroqLlama4Scout,
    #[serde(rename = "groq-llama4-maverick")]
    GroqLlama4Maverick,

    // X.AI
    #[serde(rename = "grok-4")]
    Grok4,
    #[serde(rename = "grok-3-mini")]
    Grok3Mini,

    // OpenRouter
    #[serde(rename = "openrouter")]
    OpenRouterAuto,

    // OpenRouter flagship models (via openrouter.ai/api/v1)
    #[serde(rename = "or-claude-opus-4-6")]
    OrClaudeOpus4_6,
    #[serde(rename = "or-gpt-5")]
    OrGpt5,
    #[serde(rename = "or-gemini-3-pro")]
    OrGemini3Pro,
    #[serde(rename = "or-deepseek-v3-2")]
    OrDeepseekV3_2,
    #[serde(rename = "or-grok-4")]
    OrGrok4,
    #[serde(rename = "or-llama-4-maverick")]
    OrLlama4Maverick,
    #[serde(rename = "or-qwen3-coder")]
    OrQwen3Coder,
    #[serde(rename = "or-devstral-2")]
    OrDevstral2,
    #[serde(rename = "or-glm-4-7")]
    OrGlm4_7,
    #[serde(rename = "or-kimi-k2-5")]
    OrKimiK2_5,
    #[serde(rename = "or-minimax-m2-1")]
    OrMinimaxM2_1,

    // Qwen
    #[serde(rename = "qwen3-max")]
    Qwen3Max,
    #[serde(rename = "qwen3-plus")]
    Qwen3Plus,

    // Zai
    #[serde(rename = "glm-5")]
    Glm5,
    #[serde(rename = "glm-5-code")]
    Glm5Code,
    #[serde(rename = "glm-4-7")]
    Glm4_7,
    #[serde(rename = "zai-coding-plan-glm-5")]
    Glm5CodingPlan,
    #[serde(rename = "zai-coding-plan-glm-5-code")]
    Glm5CodeCodingPlan,
    #[serde(rename = "zai-coding-plan-glm-4-7")]
    Glm4_7CodingPlan,

    // Moonshot
    #[serde(rename = "kimi-k2-5")]
    KimiK2_5,

    // Doubao
    #[serde(rename = "doubao-pro")]
    DoubaoPro,

    // Yi
    #[serde(rename = "yi-lightning")]
    YiLightning,

    // SiliconFlow
    #[serde(rename = "siliconflow")]
    SiliconFlowAuto,

    // MiniMax (Anthropic-compatible API)
    #[serde(rename = "minimax-m2-1")]
    MiniMaxM21,
    #[serde(rename = "minimax-m2-5")]
    MiniMaxM25,
    #[serde(rename = "minimax-coding-plan-m2-1")]
    MiniMaxM21CodingPlan,
    #[serde(rename = "minimax-coding-plan-m2-5")]
    MiniMaxM25CodingPlan,

    // Codex CLI (OpenAI)
    #[serde(rename = "gpt-5-codex")]
    Gpt5Codex,
    #[serde(rename = "gpt-5.1-codex")]
    Gpt5_1Codex,
    #[serde(rename = "gpt-5.2-codex")]
    Gpt5_2Codex,
    #[serde(rename = "gpt-5.3-codex")]
    CodexCli,

    // OpenCode CLI (multi-provider)
    #[serde(rename = "opencode-cli")]
    OpenCodeCli,

    // Gemini CLI (Google)
    #[serde(rename = "gemini-cli")]
    GeminiCli,
}

impl AIModel {
    /// Convert AIModel to ModelSpec used by runtime LLM factory.
    pub fn as_model_spec(&self) -> ModelSpec {
        let provider = self.provider().as_llm_provider();
        if self.is_opencode_cli() {
            ModelSpec::opencode(self.as_serialized_str(), self.as_str())
        } else if self.is_codex_cli() {
            ModelSpec::codex(self.as_serialized_str(), self.as_str())
        } else if self.is_gemini_cli() {
            ModelSpec::gemini_cli(self.as_serialized_str(), self.as_str())
        } else if matches!(self.provider(), Provider::ZaiCodingPlan)
            || matches!(self, Self::Glm5Code)
        {
            ModelSpec::new(self.as_serialized_str(), provider, self.as_str())
                .with_base_url("https://api.z.ai/api/coding/paas/v4")
        } else {
            ModelSpec::new(self.as_serialized_str(), provider, self.as_str())
        }
    }

    /// Build the shared model catalog for dynamic model switching.
    pub fn build_model_specs() -> Vec<ModelSpec> {
        let mut specs = Vec::new();
        for model in Self::all() {
            specs.push(model.as_model_spec());

            // Claude Code aliases are matched by `as_str()` at runtime as well.
            if model.is_claude_code() {
                let provider = model.provider().as_llm_provider();
                specs.push(ModelSpec::new(model.as_str(), provider, model.as_str()));
            }
        }

        specs
    }

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
            Self::Gpt5_1 => ModelMetadata {
                provider: Provider::OpenAI,
                supports_temperature: false,
                name: "GPT-5.1",
            },
            Self::Gpt5_2 => ModelMetadata {
                provider: Provider::OpenAI,
                supports_temperature: false,
                name: "GPT-5.2",
            },

            // Claude series
            Self::ClaudeOpus4_6 => ModelMetadata {
                provider: Provider::Anthropic,
                supports_temperature: true,
                name: "Claude Opus 4.6",
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

            // Google Gemini
            Self::Gemini25Pro => ModelMetadata {
                provider: Provider::Google,
                supports_temperature: true,
                name: "Gemini 2.5 Pro",
            },
            Self::Gemini25Flash => ModelMetadata {
                provider: Provider::Google,
                supports_temperature: true,
                name: "Gemini 2.5 Flash",
            },
            Self::Gemini3Pro => ModelMetadata {
                provider: Provider::Google,
                supports_temperature: true,
                name: "Gemini 3 Pro Preview",
            },
            Self::Gemini3Flash => ModelMetadata {
                provider: Provider::Google,
                supports_temperature: true,
                name: "Gemini 3 Flash Preview",
            },

            // Groq
            Self::GroqLlama4Scout => ModelMetadata {
                provider: Provider::Groq,
                supports_temperature: true,
                name: "Llama 4 Scout",
            },
            Self::GroqLlama4Maverick => ModelMetadata {
                provider: Provider::Groq,
                supports_temperature: true,
                name: "Llama 4 Maverick",
            },

            // X.AI
            Self::Grok4 => ModelMetadata {
                provider: Provider::XAI,
                supports_temperature: true,
                name: "Grok 4",
            },
            Self::Grok3Mini => ModelMetadata {
                provider: Provider::XAI,
                supports_temperature: true,
                name: "Grok 3 Mini",
            },

            // OpenRouter
            Self::OpenRouterAuto => ModelMetadata {
                provider: Provider::OpenRouter,
                supports_temperature: true,
                name: "OpenRouter Auto",
            },
            Self::OrClaudeOpus4_6 => ModelMetadata {
                provider: Provider::OpenRouter,
                supports_temperature: true,
                name: "OR Claude Opus 4.6",
            },
            Self::OrGpt5 => ModelMetadata {
                provider: Provider::OpenRouter,
                supports_temperature: false,
                name: "OR GPT-5",
            },
            Self::OrGemini3Pro => ModelMetadata {
                provider: Provider::OpenRouter,
                supports_temperature: true,
                name: "OR Gemini 3 Pro",
            },
            Self::OrDeepseekV3_2 => ModelMetadata {
                provider: Provider::OpenRouter,
                supports_temperature: true,
                name: "OR DeepSeek V3.2",
            },
            Self::OrGrok4 => ModelMetadata {
                provider: Provider::OpenRouter,
                supports_temperature: true,
                name: "OR Grok 4",
            },
            Self::OrLlama4Maverick => ModelMetadata {
                provider: Provider::OpenRouter,
                supports_temperature: true,
                name: "OR Llama 4 Maverick",
            },
            Self::OrQwen3Coder => ModelMetadata {
                provider: Provider::OpenRouter,
                supports_temperature: true,
                name: "OR Qwen3 Coder",
            },
            Self::OrDevstral2 => ModelMetadata {
                provider: Provider::OpenRouter,
                supports_temperature: true,
                name: "OR Devstral 2",
            },
            Self::OrGlm4_7 => ModelMetadata {
                provider: Provider::OpenRouter,
                supports_temperature: true,
                name: "OR GLM-4.7",
            },
            Self::OrKimiK2_5 => ModelMetadata {
                provider: Provider::OpenRouter,
                supports_temperature: true,
                name: "OR Kimi K2.5",
            },
            Self::OrMinimaxM2_1 => ModelMetadata {
                provider: Provider::OpenRouter,
                supports_temperature: true,
                name: "OR MiniMax M2.1",
            },

            // Qwen
            Self::Qwen3Max => ModelMetadata {
                provider: Provider::Qwen,
                supports_temperature: true,
                name: "Qwen3 Max",
            },
            Self::Qwen3Plus => ModelMetadata {
                provider: Provider::Qwen,
                supports_temperature: true,
                name: "Qwen3 Plus",
            },

            // Zai
            Self::Glm5 => ModelMetadata {
                provider: Provider::Zai,
                supports_temperature: true,
                name: "GLM-5",
            },
            Self::Glm5Code => ModelMetadata {
                provider: Provider::Zai,
                supports_temperature: true,
                name: "GLM-5 Code",
            },
            Self::Glm4_7 => ModelMetadata {
                provider: Provider::Zai,
                supports_temperature: true,
                name: "GLM-4.7",
            },
            Self::Glm5CodingPlan => ModelMetadata {
                provider: Provider::ZaiCodingPlan,
                supports_temperature: true,
                name: "GLM-5 (Coding Plan)",
            },
            Self::Glm5CodeCodingPlan => ModelMetadata {
                provider: Provider::ZaiCodingPlan,
                supports_temperature: true,
                name: "GLM-5 Code (Coding Plan)",
            },
            Self::Glm4_7CodingPlan => ModelMetadata {
                provider: Provider::ZaiCodingPlan,
                supports_temperature: true,
                name: "GLM-4.7 (Coding Plan)",
            },

            // Moonshot
            Self::KimiK2_5 => ModelMetadata {
                provider: Provider::Moonshot,
                supports_temperature: true,
                name: "Kimi K2.5",
            },

            // Doubao
            Self::DoubaoPro => ModelMetadata {
                provider: Provider::Doubao,
                supports_temperature: true,
                name: "Doubao Pro",
            },

            // Yi
            Self::YiLightning => ModelMetadata {
                provider: Provider::Yi,
                supports_temperature: true,
                name: "Yi Lightning",
            },

            // SiliconFlow
            Self::SiliconFlowAuto => ModelMetadata {
                provider: Provider::SiliconFlow,
                supports_temperature: true,
                name: "SiliconFlow Auto",
            },

            // MiniMax
            Self::MiniMaxM21 => ModelMetadata {
                provider: Provider::MiniMax,
                supports_temperature: true,
                name: "MiniMax M2.1",
            },
            Self::MiniMaxM25 => ModelMetadata {
                provider: Provider::MiniMax,
                supports_temperature: true,
                name: "MiniMax M2.5",
            },
            Self::MiniMaxM21CodingPlan => ModelMetadata {
                provider: Provider::MiniMaxCodingPlan,
                supports_temperature: true,
                name: "MiniMax M2.1 (Coding Plan)",
            },
            Self::MiniMaxM25CodingPlan => ModelMetadata {
                provider: Provider::MiniMaxCodingPlan,
                supports_temperature: true,
                name: "MiniMax M2.5 (Coding Plan)",
            },

            // Codex CLI
            Self::Gpt5Codex => ModelMetadata {
                provider: Provider::OpenAI,
                supports_temperature: false,
                name: "Codex GPT-5",
            },
            Self::Gpt5_1Codex => ModelMetadata {
                provider: Provider::OpenAI,
                supports_temperature: false,
                name: "Codex GPT-5.1",
            },
            Self::Gpt5_2Codex => ModelMetadata {
                provider: Provider::OpenAI,
                supports_temperature: false,
                name: "Codex GPT-5.2",
            },
            Self::CodexCli => ModelMetadata {
                provider: Provider::OpenAI,
                supports_temperature: false,
                name: "Codex GPT-5.3",
            },

            // OpenCode CLI
            Self::OpenCodeCli => ModelMetadata {
                provider: Provider::OpenAI,
                supports_temperature: false,
                name: "OpenCode CLI",
            },

            // Gemini CLI
            Self::GeminiCli => ModelMetadata {
                provider: Provider::Google,
                supports_temperature: false,
                name: "Gemini CLI",
            },
        }
    }

    /// Get the provider for this model
    pub fn provider(&self) -> Provider {
        self.metadata().provider
    }

    /// Get the canonical model identity in "provider:model" format.
    /// This is the single source of truth for model identification across
    /// routing, events, pricing lookup, and logs.
    ///
    /// Format: lowercase provider:model (e.g., "openai:gpt-5", "anthropic:claude-opus-4-6")
    pub fn canonical_id(&self) -> String {
        format!(
            "{}:{}",
            self.provider().as_canonical_str(),
            self.as_serialized_str()
        )
    }

    /// Parse a canonical model ID back to AIModel.
    /// Accepts both "provider:model" format and legacy model-only strings.
    ///
    /// Returns None if the model string is not recognized.
    pub fn from_canonical_id(canonical_id: &str) -> Option<Self> {
        let normalized = canonical_id.trim().to_lowercase();

        // Try "provider:model" format first
        if let Some((provider_str, model_str)) = normalized.split_once(':')
            && let Some(provider) = Provider::from_canonical_str(provider_str)
        {
            // Search for matching model (case-insensitive comparison)
            for model in Self::all() {
                if model.provider() == provider {
                    let serialized = model.as_serialized_str().to_lowercase();
                    if serialized == model_str || model.as_str() == model_str {
                        return Some(*model);
                    }
                }
            }
        }

        // Fallback: try model-only lookup (legacy support, case-insensitive)
        for model in Self::all() {
            let serialized = model.as_serialized_str().to_lowercase();
            if serialized == normalized || model.as_str() == normalized {
                return Some(*model);
            }
        }

        None
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
            Self::Gpt5_1 => "gpt-5.1",
            Self::Gpt5_2 => "gpt-5.2",

            // Claude series (direct API)
            Self::ClaudeOpus4_6 => "claude-opus-4-6",
            Self::ClaudeSonnet4_5 => "claude-sonnet-4-5",
            Self::ClaudeHaiku4_5 => "claude-haiku-4-5",

            // Claude Code CLI (aliases for claude CLI tool)
            Self::ClaudeCodeOpus => "opus",
            Self::ClaudeCodeSonnet => "sonnet",
            Self::ClaudeCodeHaiku => "haiku",

            // DeepSeek series
            Self::DeepseekChat => "deepseek-chat",
            Self::DeepseekReasoner => "deepseek-reasoner",

            // Google Gemini
            Self::Gemini25Pro => "gemini-2.5-pro",
            Self::Gemini25Flash => "gemini-2.5-flash",
            Self::Gemini3Pro => "gemini-3-pro-preview",
            Self::Gemini3Flash => "gemini-3-flash-preview",

            // Groq
            Self::GroqLlama4Scout => "meta-llama/llama-4-scout-17b-16e-instruct",
            Self::GroqLlama4Maverick => "meta-llama/llama-4-maverick-17b-128e-instruct",

            // X.AI
            Self::Grok4 => "grok-4",
            Self::Grok3Mini => "grok-3-mini",

            // OpenRouter
            Self::OpenRouterAuto => "openrouter/auto",
            Self::OrClaudeOpus4_6 => "anthropic/claude-opus-4.6",
            Self::OrGpt5 => "openai/gpt-5",
            Self::OrGemini3Pro => "google/gemini-3-pro-preview",
            Self::OrDeepseekV3_2 => "deepseek/deepseek-v3.2",
            Self::OrGrok4 => "x-ai/grok-4",
            Self::OrLlama4Maverick => "meta-llama/llama-4-maverick",
            Self::OrQwen3Coder => "qwen/qwen3-coder",
            Self::OrDevstral2 => "mistralai/devstral-2-2512",
            Self::OrGlm4_7 => "z-ai/glm-4.7",
            Self::OrKimiK2_5 => "moonshotai/kimi-k2.5",
            Self::OrMinimaxM2_1 => "minimax/minimax-m2.1",

            // Qwen
            Self::Qwen3Max => "qwen3-max",
            Self::Qwen3Plus => "qwen3-plus",

            // Zai
            Self::Glm5 => "glm-5",
            Self::Glm5Code => "glm-5",
            Self::Glm4_7 => "glm-4.7",
            Self::Glm5CodingPlan => "glm-5",
            Self::Glm5CodeCodingPlan => "glm-5",
            Self::Glm4_7CodingPlan => "glm-4.7",

            // Moonshot
            Self::KimiK2_5 => "kimi-k2.5",

            // Doubao
            Self::DoubaoPro => "doubao-pro-256k",

            // Yi
            Self::YiLightning => "yi-lightning",

            // SiliconFlow
            Self::SiliconFlowAuto => "deepseek-ai/DeepSeek-V3",

            // Codex CLI
            Self::Gpt5Codex => "gpt-5-codex",
            Self::Gpt5_1Codex => "gpt-5.1-codex",
            Self::Gpt5_2Codex => "gpt-5.2-codex",
            Self::CodexCli => "gpt-5.3-codex",

            // OpenCode CLI
            Self::OpenCodeCli => "opencode",

            // Gemini CLI
            Self::GeminiCli => "gemini-2.5-pro",

            // MiniMax
            Self::MiniMaxM21 => "MiniMax-M2.1",
            Self::MiniMaxM25 => "MiniMax-M2.5",
            Self::MiniMaxM21CodingPlan => "MiniMax-M2.1",
            Self::MiniMaxM25CodingPlan => "MiniMax-M2.5",
        }
    }

    /// Convert an API model name into an AIModel.
    pub fn from_api_name(name: &str) -> Option<Self> {
        let normalized = name.trim();
        if normalized.is_empty() {
            return None;
        }

        if let Some(model) = Self::all()
            .iter()
            .find(|m| {
                m.as_str().eq_ignore_ascii_case(normalized)
                    || m.as_serialized_str().eq_ignore_ascii_case(normalized)
            })
            .copied()
        {
            return Some(model);
        }

        match normalized.to_ascii_lowercase().as_str() {
            "glm-5-code" => Some(Self::Glm5Code),
            "zai-coding-plan-glm-5" => Some(Self::Glm5CodingPlan),
            "zai-coding-plan-glm-5-code" => Some(Self::Glm5CodeCodingPlan),
            "zai-coding-plan-glm-4-7" => Some(Self::Glm4_7CodingPlan),
            "minimax-m2-1" | "minimax-m2.1" => Some(Self::MiniMaxM21),
            "minimax-m2-5" | "minimax-m2.5" => Some(Self::MiniMaxM25),
            "minimax-coding-plan-m2-1" | "minimax-coding-plan-m2.1" => {
                Some(Self::MiniMaxM21CodingPlan)
            }
            "minimax-coding-plan-m2-5" | "minimax-coding-plan-m2.5" => {
                Some(Self::MiniMaxM25CodingPlan)
            }
            "claude-sonnet-4-5-20250514" | "claude-sonnet-4-20250514" => {
                Some(Self::ClaudeSonnet4_5)
            }
            "claude-opus-4-6-20260205" | "claude-opus-4-6-20250514" => Some(Self::ClaudeOpus4_6),
            "claude-haiku-4-5-20250514" | "claude-haiku-4-20250514" => Some(Self::ClaudeHaiku4_5),
            _ => {
                if normalized.starts_with("claude-sonnet-4") {
                    Some(Self::ClaudeSonnet4_5)
                } else if normalized.starts_with("claude-opus-4-6")
                    || normalized.starts_with("claude-opus-4")
                {
                    Some(Self::ClaudeOpus4_6)
                } else if normalized.starts_with("claude-haiku-4") {
                    Some(Self::ClaudeHaiku4_5)
                } else {
                    None
                }
            }
        }
    }

    /// Resolve a concrete model for a specific provider/model pair.
    pub fn for_provider_and_model(provider: Provider, model: &str) -> Option<Self> {
        let normalized = model.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            return None;
        }

        let canonical = match normalized.as_str() {
            "glm5" => "glm-5",
            "glm5-code" => "glm-5-code",
            "glm-4.7" => "glm-4-7",
            "minimax-m2.1" => "minimax-m2-1",
            "minimax-m2.5" => "minimax-m2-5",
            value => value,
        };

        match provider {
            Provider::MiniMax => match canonical {
                "minimax-m2-1" => Some(Self::MiniMaxM21),
                "minimax-m2-5" => Some(Self::MiniMaxM25),
                _ => None,
            },
            Provider::MiniMaxCodingPlan => match canonical {
                "minimax-m2-1" => Some(Self::MiniMaxM21CodingPlan),
                "minimax-m2-5" => Some(Self::MiniMaxM25CodingPlan),
                _ => None,
            },
            Provider::Zai => match canonical {
                "glm-5" => Some(Self::Glm5),
                "glm-5-code" => Some(Self::Glm5Code),
                "glm-4-7" => Some(Self::Glm4_7),
                _ => None,
            },
            Provider::ZaiCodingPlan => match canonical {
                "glm-5" => Some(Self::Glm5CodingPlan),
                "glm-5-code" => Some(Self::Glm5CodeCodingPlan),
                "glm-4-7" => Some(Self::Glm4_7CodingPlan),
                _ => None,
            },
            _ => {
                let parsed = Self::from_api_name(canonical)?;
                if parsed.provider() == provider {
                    Some(parsed)
                } else {
                    None
                }
            }
        }
    }

    /// Remap this model into another provider when a provider-specific counterpart exists.
    pub fn remap_provider(&self, provider: Provider) -> Option<Self> {
        if self.provider() == provider {
            return Some(*self);
        }

        let canonical = match self {
            Self::MiniMaxM21 | Self::MiniMaxM21CodingPlan => "minimax-m2-1",
            Self::MiniMaxM25 | Self::MiniMaxM25CodingPlan => "minimax-m2-5",
            Self::Glm5 | Self::Glm5CodingPlan => "glm-5",
            Self::Glm5Code | Self::Glm5CodeCodingPlan => "glm-5-code",
            Self::Glm4_7 | Self::Glm4_7CodingPlan => "glm-4-7",
            _ => return None,
        };

        Self::for_provider_and_model(provider, canonical)
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
            Self::Gpt5_1 => "gpt-5-1",
            Self::Gpt5_2 => "gpt-5-2",

            // Claude series (direct API)
            Self::ClaudeOpus4_6 => "claude-opus-4-6",
            Self::ClaudeSonnet4_5 => "claude-sonnet-4-5",
            Self::ClaudeHaiku4_5 => "claude-haiku-4-5",

            // Claude Code CLI aliases
            Self::ClaudeCodeOpus => "claude-code-opus",
            Self::ClaudeCodeSonnet => "claude-code-sonnet",
            Self::ClaudeCodeHaiku => "claude-code-haiku",

            // DeepSeek series
            Self::DeepseekChat => "deepseek-chat",
            Self::DeepseekReasoner => "deepseek-reasoner",

            // Google Gemini
            Self::Gemini25Pro => "gemini-2-5-pro",
            Self::Gemini25Flash => "gemini-2-5-flash",
            Self::Gemini3Pro => "gemini-3-pro",
            Self::Gemini3Flash => "gemini-3-flash",

            // Groq
            Self::GroqLlama4Scout => "groq-llama4-scout",
            Self::GroqLlama4Maverick => "groq-llama4-maverick",

            // X.AI
            Self::Grok4 => "grok-4",
            Self::Grok3Mini => "grok-3-mini",

            // OpenRouter
            Self::OpenRouterAuto => "openrouter",
            Self::OrClaudeOpus4_6 => "or-claude-opus-4-6",
            Self::OrGpt5 => "or-gpt-5",
            Self::OrGemini3Pro => "or-gemini-3-pro",
            Self::OrDeepseekV3_2 => "or-deepseek-v3-2",
            Self::OrGrok4 => "or-grok-4",
            Self::OrLlama4Maverick => "or-llama-4-maverick",
            Self::OrQwen3Coder => "or-qwen3-coder",
            Self::OrDevstral2 => "or-devstral-2",
            Self::OrGlm4_7 => "or-glm-4-7",
            Self::OrKimiK2_5 => "or-kimi-k2-5",
            Self::OrMinimaxM2_1 => "or-minimax-m2-1",

            // Qwen
            Self::Qwen3Max => "qwen3-max",
            Self::Qwen3Plus => "qwen3-plus",

            // Zai
            Self::Glm5 => "glm-5",
            Self::Glm5Code => "glm-5-code",
            Self::Glm4_7 => "glm-4-7",
            Self::Glm5CodingPlan => "zai-coding-plan-glm-5",
            Self::Glm5CodeCodingPlan => "zai-coding-plan-glm-5-code",
            Self::Glm4_7CodingPlan => "zai-coding-plan-glm-4-7",

            // Moonshot
            Self::KimiK2_5 => "kimi-k2-5",

            // Doubao
            Self::DoubaoPro => "doubao-pro",

            // Yi
            Self::YiLightning => "yi-lightning",

            // SiliconFlow
            Self::SiliconFlowAuto => "siliconflow",

            // MiniMax
            Self::MiniMaxM21 => "minimax-m2-1",
            Self::MiniMaxM25 => "minimax-m2-5",
            Self::MiniMaxM21CodingPlan => "minimax-coding-plan-m2-1",
            Self::MiniMaxM25CodingPlan => "minimax-coding-plan-m2-5",

            // Codex CLI
            Self::Gpt5Codex => "gpt-5-codex",
            Self::Gpt5_1Codex => "gpt-5.1-codex",
            Self::Gpt5_2Codex => "gpt-5.2-codex",
            Self::CodexCli => "gpt-5.3-codex",

            // OpenCode CLI
            Self::OpenCodeCli => "opencode-cli",

            // Gemini CLI
            Self::GeminiCli => "gemini-cli",
        }
    }

    /// Check if this model uses the Codex CLI
    pub fn is_codex_cli(&self) -> bool {
        matches!(
            self,
            Self::Gpt5Codex | Self::Gpt5_1Codex | Self::Gpt5_2Codex | Self::CodexCli
        )
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

    /// Check if this model uses the Gemini CLI
    pub fn is_gemini_cli(&self) -> bool {
        matches!(self, Self::GeminiCli)
    }

    /// Check if this model is any CLI-based model (manages its own auth)
    pub fn is_cli_model(&self) -> bool {
        self.is_codex_cli()
            || self.is_opencode_cli()
            || self.is_gemini_cli()
            || self.is_claude_code()
    }

    /// Get a same-provider fallback model (cheaper tier).
    /// Returns None if this is already the cheapest or no fallback exists.
    pub fn same_provider_fallback(&self) -> Option<Self> {
        match self {
            // Anthropic: Opus -> Sonnet -> Haiku
            Self::ClaudeOpus4_6 => Some(Self::ClaudeSonnet4_5),
            Self::ClaudeSonnet4_5 => Some(Self::ClaudeHaiku4_5),
            // OpenAI: Pro -> Gpt5 -> Mini -> Nano
            Self::Gpt5Pro => Some(Self::Gpt5),
            Self::Gpt5 => Some(Self::Gpt5Mini),
            Self::Gpt5Mini => Some(Self::Gpt5Nano),
            // DeepSeek: Reasoner -> Chat
            Self::DeepseekReasoner => Some(Self::DeepseekChat),
            // Gemini: Pro -> Flash (both generations)
            Self::Gemini3Pro => Some(Self::Gemini3Flash),
            Self::Gemini25Pro => Some(Self::Gemini25Flash),
            // GLM: 5 -> 5Code -> 4.7
            Self::Glm5 => Some(Self::Glm5Code),
            Self::Glm5Code => Some(Self::Glm4_7),
            Self::Glm5CodingPlan => Some(Self::Glm5CodeCodingPlan),
            Self::Glm5CodeCodingPlan => Some(Self::Glm4_7CodingPlan),
            // X.AI: Grok4 -> Grok3Mini
            Self::Grok4 => Some(Self::Grok3Mini),
            _ => None,
        }
    }

    /// Get the OpenRouter equivalent of this model (if one exists).
    pub fn openrouter_equivalent(&self) -> Option<Self> {
        match self {
            Self::ClaudeOpus4_6 | Self::ClaudeSonnet4_5 => Some(Self::OrClaudeOpus4_6),
            Self::Gpt5 | Self::Gpt5Mini | Self::Gpt5Pro => Some(Self::OrGpt5),
            Self::Gemini3Pro | Self::Gemini25Pro => Some(Self::OrGemini3Pro),
            Self::DeepseekChat | Self::DeepseekReasoner => Some(Self::OrDeepseekV3_2),
            Self::Grok4 | Self::Grok3Mini => Some(Self::OrGrok4),
            Self::Glm5
            | Self::Glm5Code
            | Self::Glm4_7
            | Self::Glm5CodingPlan
            | Self::Glm5CodeCodingPlan
            | Self::Glm4_7CodingPlan => Some(Self::OrGlm4_7),
            Self::KimiK2_5 => Some(Self::OrKimiK2_5),
            Self::Qwen3Max | Self::Qwen3Plus => Some(Self::OrQwen3Coder),
            Self::MiniMaxM21
            | Self::MiniMaxM25
            | Self::MiniMaxM21CodingPlan
            | Self::MiniMaxM25CodingPlan => Some(Self::OrMinimaxM2_1),
            _ => None,
        }
    }

    /// Get all available models as a slice
    pub fn all() -> &'static [AIModel] {
        &[
            // OpenAI
            Self::Gpt5,
            Self::Gpt5Mini,
            Self::Gpt5Nano,
            Self::Gpt5Pro,
            Self::Gpt5_1,
            Self::Gpt5_2,
            // Anthropic (direct API)
            Self::ClaudeOpus4_6,
            Self::ClaudeSonnet4_5,
            Self::ClaudeHaiku4_5,
            // Anthropic (Claude Code CLI)
            Self::ClaudeCodeOpus,
            Self::ClaudeCodeSonnet,
            Self::ClaudeCodeHaiku,
            // DeepSeek
            Self::DeepseekChat,
            Self::DeepseekReasoner,
            // Google Gemini
            Self::Gemini25Pro,
            Self::Gemini25Flash,
            Self::Gemini3Pro,
            Self::Gemini3Flash,
            // Groq
            Self::GroqLlama4Scout,
            Self::GroqLlama4Maverick,
            // X.AI
            Self::Grok4,
            Self::Grok3Mini,
            // OpenRouter
            Self::OpenRouterAuto,
            Self::OrClaudeOpus4_6,
            Self::OrGpt5,
            Self::OrGemini3Pro,
            Self::OrDeepseekV3_2,
            Self::OrGrok4,
            Self::OrLlama4Maverick,
            Self::OrQwen3Coder,
            Self::OrDevstral2,
            Self::OrGlm4_7,
            Self::OrKimiK2_5,
            Self::OrMinimaxM2_1,
            // Qwen
            Self::Qwen3Max,
            Self::Qwen3Plus,
            // Zai
            Self::Glm5,
            Self::Glm5Code,
            Self::Glm4_7,
            Self::Glm5CodingPlan,
            Self::Glm5CodeCodingPlan,
            Self::Glm4_7CodingPlan,
            // Moonshot
            Self::KimiK2_5,
            // Doubao
            Self::DoubaoPro,
            // Yi
            Self::YiLightning,
            // SiliconFlow
            Self::SiliconFlowAuto,
            // MiniMax
            Self::MiniMaxM21,
            Self::MiniMaxM25,
            Self::MiniMaxM21CodingPlan,
            Self::MiniMaxM25CodingPlan,
            // Codex CLI
            Self::Gpt5Codex,
            Self::Gpt5_1Codex,
            Self::Gpt5_2Codex,
            Self::CodexCli,
            // OpenCode CLI
            Self::OpenCodeCli,
            // Gemini CLI
            Self::GeminiCli,
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
        assert_eq!(AIModel::Gemini25Pro.provider(), Provider::Google);
        assert_eq!(AIModel::GroqLlama4Scout.provider(), Provider::Groq);
        assert_eq!(AIModel::Grok4.provider(), Provider::XAI);
        assert_eq!(AIModel::Qwen3Max.provider(), Provider::Qwen);
        assert_eq!(AIModel::Glm4_7.provider(), Provider::Zai);
        assert_eq!(AIModel::Glm5CodingPlan.provider(), Provider::ZaiCodingPlan);
        assert_eq!(AIModel::KimiK2_5.provider(), Provider::Moonshot);
        assert_eq!(AIModel::DoubaoPro.provider(), Provider::Doubao);
        assert_eq!(AIModel::YiLightning.provider(), Provider::Yi);
        assert_eq!(AIModel::MiniMaxM25.provider(), Provider::MiniMax);
        assert_eq!(AIModel::MiniMaxM21.provider(), Provider::MiniMax);
        assert_eq!(
            AIModel::MiniMaxM25CodingPlan.provider(),
            Provider::MiniMaxCodingPlan
        );
    }

    #[test]
    fn test_supports_temperature() {
        // Models that don't support temperature
        assert!(!AIModel::Gpt5.supports_temperature());
        assert!(!AIModel::Gpt5Mini.supports_temperature());
        assert!(!AIModel::Gpt5_1.supports_temperature());
        assert!(!AIModel::Gpt5_2.supports_temperature());
        assert!(!AIModel::Gpt5Codex.supports_temperature());
        assert!(!AIModel::Gpt5_1Codex.supports_temperature());
        assert!(!AIModel::Gpt5_2Codex.supports_temperature());
        assert!(!AIModel::CodexCli.supports_temperature());
        assert!(!AIModel::OpenCodeCli.supports_temperature());
        assert!(!AIModel::GeminiCli.supports_temperature());

        // Models that support temperature
        assert!(AIModel::ClaudeSonnet4_5.supports_temperature());
        assert!(AIModel::ClaudeHaiku4_5.supports_temperature());
        assert!(AIModel::DeepseekChat.supports_temperature());
        assert!(AIModel::Gemini25Flash.supports_temperature());
        assert!(AIModel::GroqLlama4Maverick.supports_temperature());
    }

    #[test]
    fn test_is_codex_cli() {
        assert!(AIModel::Gpt5Codex.is_codex_cli());
        assert!(AIModel::Gpt5_1Codex.is_codex_cli());
        assert!(AIModel::Gpt5_2Codex.is_codex_cli());
        assert!(AIModel::CodexCli.is_codex_cli());
        assert!(!AIModel::Gpt5.is_codex_cli());
    }

    #[test]
    fn test_is_opencode_cli() {
        assert!(AIModel::OpenCodeCli.is_opencode_cli());
        assert!(!AIModel::Gpt5.is_opencode_cli());
    }

    #[test]
    fn test_is_gemini_cli() {
        assert!(AIModel::GeminiCli.is_gemini_cli());
        assert!(!AIModel::Gpt5.is_gemini_cli());
    }

    #[test]
    fn test_as_str() {
        assert_eq!(AIModel::Gpt5.as_str(), "gpt-5");
        assert_eq!(AIModel::Gpt5_1.as_str(), "gpt-5.1");
        assert_eq!(AIModel::ClaudeSonnet4_5.as_str(), "claude-sonnet-4-5");
        assert_eq!(AIModel::ClaudeHaiku4_5.as_str(), "claude-haiku-4-5");
        assert_eq!(AIModel::Gpt5Codex.as_str(), "gpt-5-codex");
        assert_eq!(AIModel::Gpt5_1Codex.as_str(), "gpt-5.1-codex");
        assert_eq!(AIModel::Gpt5_2Codex.as_str(), "gpt-5.2-codex");
        assert_eq!(AIModel::CodexCli.as_str(), "gpt-5.3-codex");
        assert_eq!(AIModel::OpenCodeCli.as_str(), "opencode");
        assert_eq!(AIModel::GeminiCli.as_str(), "gemini-2.5-pro");
        assert_eq!(AIModel::MiniMaxM21.as_str(), "MiniMax-M2.1");
        assert_eq!(AIModel::MiniMaxM21CodingPlan.as_str(), "MiniMax-M2.1");
        assert_eq!(AIModel::Glm5Code.as_str(), "glm-5");
        assert_eq!(AIModel::Glm5CodingPlan.as_str(), "glm-5");
        assert_eq!(AIModel::DeepseekChat.as_str(), "deepseek-chat");
        assert_eq!(AIModel::Gemini25Pro.as_str(), "gemini-2.5-pro");
        assert_eq!(
            AIModel::GroqLlama4Scout.as_str(),
            "meta-llama/llama-4-scout-17b-16e-instruct"
        );
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
    fn test_for_provider_and_model() {
        assert_eq!(
            AIModel::for_provider_and_model(Provider::MiniMax, "minimax-m2-5"),
            Some(AIModel::MiniMaxM25)
        );
        assert_eq!(
            AIModel::for_provider_and_model(Provider::MiniMaxCodingPlan, "minimax-m2-5"),
            Some(AIModel::MiniMaxM25CodingPlan)
        );
        assert_eq!(
            AIModel::for_provider_and_model(Provider::ZaiCodingPlan, "glm-5"),
            Some(AIModel::Glm5CodingPlan)
        );
    }

    #[test]
    fn test_remap_provider() {
        assert_eq!(
            AIModel::MiniMaxM25.remap_provider(Provider::MiniMaxCodingPlan),
            Some(AIModel::MiniMaxM25CodingPlan)
        );
        assert_eq!(
            AIModel::Glm5CodingPlan.remap_provider(Provider::Zai),
            Some(AIModel::Glm5)
        );
        assert_eq!(
            AIModel::ClaudeSonnet4_5.remap_provider(Provider::MiniMax),
            None
        );
    }

    #[test]
    fn test_display_name() {
        assert_eq!(AIModel::Gpt5.display_name(), "GPT-5");
        assert_eq!(AIModel::Gpt5_2.display_name(), "GPT-5.2");
        assert_eq!(AIModel::ClaudeSonnet4_5.display_name(), "Claude Sonnet 4.5");
        assert_eq!(AIModel::ClaudeHaiku4_5.display_name(), "Claude Haiku 4.5");
        assert_eq!(AIModel::Gpt5Codex.display_name(), "Codex GPT-5");
        assert_eq!(AIModel::Gpt5_1Codex.display_name(), "Codex GPT-5.1");
        assert_eq!(AIModel::Gpt5_2Codex.display_name(), "Codex GPT-5.2");
        assert_eq!(AIModel::CodexCli.display_name(), "Codex GPT-5.3");
        assert_eq!(AIModel::OpenCodeCli.display_name(), "OpenCode CLI");
        assert_eq!(AIModel::GeminiCli.display_name(), "Gemini CLI");
        assert_eq!(AIModel::DeepseekChat.display_name(), "DeepSeek Chat");
        assert_eq!(AIModel::MiniMaxM21.display_name(), "MiniMax M2.1");
    }

    #[test]
    fn test_all_models() {
        let models = AIModel::all();
        assert_eq!(models.len(), 56);
        assert!(models.contains(&AIModel::Gpt5));
        assert!(models.contains(&AIModel::Gpt5_1));
        assert!(models.contains(&AIModel::ClaudeOpus4_6));
        assert!(models.contains(&AIModel::ClaudeSonnet4_5));
        assert!(models.contains(&AIModel::ClaudeHaiku4_5));
        assert!(models.contains(&AIModel::Gpt5Codex));
        assert!(models.contains(&AIModel::Gpt5_1Codex));
        assert!(models.contains(&AIModel::Gpt5_2Codex));
        assert!(models.contains(&AIModel::CodexCli));
        assert!(models.contains(&AIModel::OpenCodeCli));
        assert!(models.contains(&AIModel::GeminiCli));
        assert!(models.contains(&AIModel::DeepseekChat));
        assert!(models.contains(&AIModel::Gemini25Pro));
        assert!(models.contains(&AIModel::MiniMaxM21));
        assert!(models.contains(&AIModel::MiniMaxM21CodingPlan));
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

    #[test]
    fn test_provider_as_llm_provider() {
        assert_eq!(Provider::OpenAI.as_llm_provider(), LlmProvider::OpenAI);
        assert_eq!(
            Provider::Anthropic.as_llm_provider(),
            LlmProvider::Anthropic
        );
        assert_eq!(Provider::Google.as_llm_provider(), LlmProvider::Google);
    }

    #[test]
    fn test_build_model_specs_contains_codex_cli() {
        let specs = AIModel::build_model_specs();
        assert!(
            specs
                .iter()
                .any(|spec| spec.name == "gpt-5-codex" && spec.is_codex_cli)
        );
        assert!(
            specs
                .iter()
                .any(|spec| spec.name == "gpt-5.1-codex" && spec.is_codex_cli)
        );
        assert!(
            specs
                .iter()
                .any(|spec| spec.name == "gpt-5.2-codex" && spec.is_codex_cli)
        );
        assert!(
            specs
                .iter()
                .any(|spec| spec.name == "gpt-5.3-codex" && spec.is_codex_cli)
        );
    }

    #[test]
    fn test_glm5_code_uses_glm5_model_with_coding_endpoint() {
        let spec = AIModel::Glm5Code.as_model_spec();
        assert_eq!(spec.client_model, "glm-5");
        assert_eq!(spec.name, "glm-5-code");
        assert_eq!(
            spec.base_url.as_deref(),
            Some("https://api.z.ai/api/coding/paas/v4")
        );

        let coding_plan_spec = AIModel::Glm5CodingPlan.as_model_spec();
        assert_eq!(coding_plan_spec.client_model, "glm-5");
        assert_eq!(
            coding_plan_spec.base_url.as_deref(),
            Some("https://api.z.ai/api/coding/paas/v4")
        );
    }

    #[test]
    fn test_provider_api_key_env() {
        assert_eq!(Provider::Google.api_key_env(), "GEMINI_API_KEY");
        assert_eq!(Provider::Groq.api_key_env(), "GROQ_API_KEY");
        assert_eq!(Provider::Qwen.api_key_env(), "DASHSCOPE_API_KEY");
        assert_eq!(Provider::MiniMax.api_key_env(), "MINIMAX_API_KEY");
        assert_eq!(
            Provider::MiniMaxCodingPlan.api_key_env(),
            "MINIMAX_CODING_PLAN_API_KEY"
        );
        assert_eq!(Provider::Zai.api_key_env(), "ZAI_API_KEY");
        assert_eq!(
            Provider::ZaiCodingPlan.api_key_env(),
            "ZAI_CODING_PLAN_API_KEY"
        );
    }

    #[test]
    fn test_same_provider_fallback() {
        // Anthropic chain
        assert_eq!(
            AIModel::ClaudeOpus4_6.same_provider_fallback(),
            Some(AIModel::ClaudeSonnet4_5)
        );
        assert_eq!(
            AIModel::ClaudeSonnet4_5.same_provider_fallback(),
            Some(AIModel::ClaudeHaiku4_5)
        );
        assert_eq!(AIModel::ClaudeHaiku4_5.same_provider_fallback(), None);

        // OpenAI chain
        assert_eq!(
            AIModel::Gpt5Pro.same_provider_fallback(),
            Some(AIModel::Gpt5)
        );
        assert_eq!(
            AIModel::Gpt5.same_provider_fallback(),
            Some(AIModel::Gpt5Mini)
        );
        assert_eq!(
            AIModel::Gpt5Mini.same_provider_fallback(),
            Some(AIModel::Gpt5Nano)
        );
        assert_eq!(AIModel::Gpt5Nano.same_provider_fallback(), None);

        // DeepSeek chain
        assert_eq!(
            AIModel::DeepseekReasoner.same_provider_fallback(),
            Some(AIModel::DeepseekChat)
        );
        assert_eq!(AIModel::DeepseekChat.same_provider_fallback(), None);

        // CLI models have no fallback
        assert_eq!(AIModel::CodexCli.same_provider_fallback(), None);
    }

    #[test]
    fn test_openrouter_equivalent() {
        assert_eq!(
            AIModel::ClaudeOpus4_6.openrouter_equivalent(),
            Some(AIModel::OrClaudeOpus4_6)
        );
        assert_eq!(AIModel::Gpt5.openrouter_equivalent(), Some(AIModel::OrGpt5));
        assert_eq!(
            AIModel::DeepseekChat.openrouter_equivalent(),
            Some(AIModel::OrDeepseekV3_2)
        );
        assert_eq!(
            AIModel::KimiK2_5.openrouter_equivalent(),
            Some(AIModel::OrKimiK2_5)
        );
        assert_eq!(
            AIModel::MiniMaxM21.openrouter_equivalent(),
            Some(AIModel::OrMinimaxM2_1)
        );
        assert_eq!(
            AIModel::MiniMaxM25.openrouter_equivalent(),
            Some(AIModel::OrMinimaxM2_1)
        );
        // OR models themselves have no OR equivalent
        assert_eq!(AIModel::OrClaudeOpus4_6.openrouter_equivalent(), None);
        // CLI models have no OR equivalent
        assert_eq!(AIModel::CodexCli.openrouter_equivalent(), None);
    }

    #[test]
    fn test_canonical_id() {
        // Test canonical ID generation
        assert_eq!(AIModel::Gpt5.canonical_id(), "openai:gpt-5");
        assert_eq!(
            AIModel::ClaudeSonnet4_5.canonical_id(),
            "anthropic:claude-sonnet-4-5"
        );
        assert_eq!(
            AIModel::DeepseekChat.canonical_id(),
            "deepseek:deepseek-chat"
        );
        assert_eq!(AIModel::Gemini3Pro.canonical_id(), "google:gemini-3-pro");
        assert_eq!(AIModel::OrGpt5.canonical_id(), "openrouter:or-gpt-5");
        assert_eq!(AIModel::CodexCli.canonical_id(), "openai:gpt-5.3-codex");
    }

    #[test]
    fn test_from_canonical_id() {
        // Test parsing canonical IDs
        assert_eq!(
            AIModel::from_canonical_id("openai:gpt-5"),
            Some(AIModel::Gpt5)
        );
        assert_eq!(
            AIModel::from_canonical_id("anthropic:claude-sonnet-4-5"),
            Some(AIModel::ClaudeSonnet4_5)
        );
        assert_eq!(
            AIModel::from_canonical_id("deepseek:deepseek-chat"),
            Some(AIModel::DeepseekChat)
        );

        // Test legacy model-only strings (fallback)
        assert_eq!(AIModel::from_canonical_id("gpt-5"), Some(AIModel::Gpt5));
        assert_eq!(
            AIModel::from_canonical_id("claude-sonnet-4-5"),
            Some(AIModel::ClaudeSonnet4_5)
        );

        // Test invalid IDs
        assert_eq!(AIModel::from_canonical_id("unknown:model"), None);
        assert_eq!(AIModel::from_canonical_id("invalid-model"), None);
    }

    #[test]
    fn test_canonical_id_round_trip() {
        // Test round-trip: canonical_id -> from_canonical_id
        for model in AIModel::all() {
            let canonical = model.canonical_id();
            let parsed = AIModel::from_canonical_id(&canonical);
            assert_eq!(
                parsed,
                Some(*model),
                "Round-trip failed for {} -> {}",
                model.as_str(),
                canonical
            );
        }
    }

    #[test]
    fn test_provider_canonical_str() {
        // Test provider canonical strings
        assert_eq!(Provider::OpenAI.as_canonical_str(), "openai");
        assert_eq!(Provider::Anthropic.as_canonical_str(), "anthropic");
        assert_eq!(Provider::DeepSeek.as_canonical_str(), "deepseek");
        assert_eq!(Provider::Google.as_canonical_str(), "google");
        assert_eq!(Provider::OpenRouter.as_canonical_str(), "openrouter");
        assert_eq!(
            Provider::ZaiCodingPlan.as_canonical_str(),
            "zai-coding-plan"
        );
        assert_eq!(
            Provider::MiniMaxCodingPlan.as_canonical_str(),
            "minimax-coding-plan"
        );
    }

    #[test]
    fn test_provider_from_canonical_str() {
        // Test parsing provider canonical strings
        assert_eq!(
            Provider::from_canonical_str("openai"),
            Some(Provider::OpenAI)
        );
        assert_eq!(
            Provider::from_canonical_str("anthropic"),
            Some(Provider::Anthropic)
        );
        assert_eq!(
            Provider::from_canonical_str("deepseek"),
            Some(Provider::DeepSeek)
        );
        assert_eq!(
            Provider::from_canonical_str("google"),
            Some(Provider::Google)
        );
        assert_eq!(Provider::from_canonical_str("invalid"), None);
    }

    #[test]
    fn test_flagship_model() {
        assert_eq!(
            Provider::Anthropic.flagship_model(),
            AIModel::ClaudeSonnet4_5
        );
        assert_eq!(Provider::OpenAI.flagship_model(), AIModel::Gpt5);
        assert_eq!(Provider::DeepSeek.flagship_model(), AIModel::DeepseekChat);
        assert_eq!(Provider::Google.flagship_model(), AIModel::Gemini3Pro);
        assert_eq!(Provider::Zai.flagship_model(), AIModel::Glm5);
        assert_eq!(
            Provider::ZaiCodingPlan.flagship_model(),
            AIModel::Glm5CodingPlan
        );
        assert_eq!(
            Provider::MiniMaxCodingPlan.flagship_model(),
            AIModel::MiniMaxM25CodingPlan
        );
        assert_eq!(
            Provider::OpenRouter.flagship_model(),
            AIModel::OrClaudeOpus4_6
        );
    }

    #[test]
    fn test_minimax_m25_serialization_consistency() {
        // as_serialized_str() must match the serde rename
        let json_str = serde_json::to_string(&AIModel::MiniMaxM25).unwrap();
        let expected = format!("\"{}\"", AIModel::MiniMaxM25.as_serialized_str());
        assert_eq!(json_str, expected);
    }

    #[test]
    fn test_from_api_name_trimmed_input() {
        // Whitespace around model name should still resolve
        assert_eq!(
            AIModel::from_api_name("  Claude-Sonnet-4-5-20250514  "),
            Some(AIModel::ClaudeSonnet4_5)
        );
    }
}
