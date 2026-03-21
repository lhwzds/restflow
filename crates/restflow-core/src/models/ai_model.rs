use restflow_models::{LlmProvider, ModelSpec, provider_meta};
use restflow_traits::ModelProvider;
use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;
use ts_rs::TS;

use super::catalog;
use crate::models::ValidationError;

/// AI model provider
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, TS, Type)]
#[ts(export)]
pub enum Provider {
    #[serde(rename = "openai")]
    #[ts(rename = "openai")]
    OpenAI,
    #[serde(rename = "anthropic")]
    #[ts(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "claude-code")]
    #[ts(rename = "claude-code")]
    ClaudeCode,
    #[serde(rename = "codex")]
    #[ts(rename = "codex")]
    Codex,
    #[serde(rename = "deepseek")]
    #[ts(rename = "deepseek")]
    DeepSeek,
    #[serde(rename = "google")]
    #[ts(rename = "google")]
    Google,
    #[serde(rename = "groq")]
    #[ts(rename = "groq")]
    Groq,
    #[serde(rename = "openrouter")]
    #[ts(rename = "openrouter")]
    OpenRouter,
    #[serde(rename = "xai")]
    #[ts(rename = "xai")]
    XAI,
    #[serde(rename = "qwen")]
    #[ts(rename = "qwen")]
    Qwen,
    #[serde(rename = "zai")]
    #[ts(rename = "zai")]
    Zai,
    #[serde(rename = "zai-coding-plan")]
    #[ts(rename = "zai-coding-plan")]
    ZaiCodingPlan,
    #[serde(rename = "moonshot")]
    #[ts(rename = "moonshot")]
    Moonshot,
    #[serde(rename = "doubao")]
    #[ts(rename = "doubao")]
    Doubao,
    #[serde(rename = "yi")]
    #[ts(rename = "yi")]
    Yi,
    #[serde(rename = "siliconflow")]
    #[ts(rename = "siliconflow")]
    SiliconFlow,
    #[serde(rename = "minimax")]
    #[ts(rename = "minimax")]
    MiniMax,
    #[serde(rename = "minimax-coding-plan")]
    #[ts(rename = "minimax-coding-plan")]
    MiniMaxCodingPlan,
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
    pub fn all() -> &'static [Provider] {
        &[
            Self::OpenAI,
            Self::Anthropic,
            Self::ClaudeCode,
            Self::Codex,
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

    pub fn api_key_env(&self) -> Option<&'static str> {
        provider_meta(self.as_model_provider()).api_key_env
    }

    /// Convert Provider to LLM provider used by runtime factory.
    pub fn as_llm_provider(&self) -> LlmProvider {
        provider_meta(self.as_model_provider()).runtime_provider
    }

    /// Convert to shared provider identity used by cross-crate parsers.
    pub fn as_model_provider(&self) -> ModelProvider {
        match *self {
            Self::OpenAI => ModelProvider::OpenAI,
            Self::Anthropic => ModelProvider::Anthropic,
            Self::ClaudeCode => ModelProvider::ClaudeCode,
            Self::Codex => ModelProvider::Codex,
            Self::DeepSeek => ModelProvider::DeepSeek,
            Self::Google => ModelProvider::Google,
            Self::Groq => ModelProvider::Groq,
            Self::OpenRouter => ModelProvider::OpenRouter,
            Self::XAI => ModelProvider::XAI,
            Self::Qwen => ModelProvider::Qwen,
            Self::Zai => ModelProvider::Zai,
            Self::ZaiCodingPlan => ModelProvider::ZaiCodingPlan,
            Self::Moonshot => ModelProvider::Moonshot,
            Self::Doubao => ModelProvider::Doubao,
            Self::Yi => ModelProvider::Yi,
            Self::SiliconFlow => ModelProvider::SiliconFlow,
            Self::MiniMax => ModelProvider::MiniMax,
            Self::MiniMaxCodingPlan => ModelProvider::MiniMaxCodingPlan,
        }
    }

    /// Convert from shared provider identity.
    pub fn from_model_provider(provider: ModelProvider) -> Self {
        match provider {
            ModelProvider::OpenAI => Self::OpenAI,
            ModelProvider::Anthropic => Self::Anthropic,
            ModelProvider::ClaudeCode => Self::ClaudeCode,
            ModelProvider::Codex => Self::Codex,
            ModelProvider::DeepSeek => Self::DeepSeek,
            ModelProvider::Google => Self::Google,
            ModelProvider::Groq => Self::Groq,
            ModelProvider::OpenRouter => Self::OpenRouter,
            ModelProvider::XAI => Self::XAI,
            ModelProvider::Qwen => Self::Qwen,
            ModelProvider::Zai => Self::Zai,
            ModelProvider::ZaiCodingPlan => Self::ZaiCodingPlan,
            ModelProvider::Moonshot => Self::Moonshot,
            ModelProvider::Doubao => Self::Doubao,
            ModelProvider::Yi => Self::Yi,
            ModelProvider::SiliconFlow => Self::SiliconFlow,
            ModelProvider::MiniMax => Self::MiniMax,
            ModelProvider::MiniMaxCodingPlan => Self::MiniMaxCodingPlan,
        }
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

/// Model metadata containing provider, temperature support, and display name
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelMetadata {
    pub provider: Provider,
    pub supports_temperature: bool,
    pub name: &'static str,
}

/// Serializable model metadata for transferring to frontend
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Type)]
#[ts(export)]
pub struct ModelMetadataDTO {
    pub model: ModelId,
    pub provider: Provider,
    pub supports_temperature: bool,
    pub name: String,
}

/// Provider + model pair used by API and persistence layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Type)]
#[ts(export)]
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
        let normalized = self.normalized();
        let expected_provider = normalized.model.provider();
        if normalized.provider != expected_provider {
            return Err(ValidationError::new(
                "model_ref",
                format!(
                    "provider '{}' does not match model provider '{}'",
                    normalized.provider.as_canonical_str(),
                    expected_provider.as_canonical_str()
                ),
            ));
        }
        Ok(())
    }

    /// Return canonical ID in `provider:model` format.
    pub fn canonical_id(&self) -> String {
        let normalized = self.normalized();
        format!(
            "{}:{}",
            normalized.provider.as_canonical_str(),
            normalized.model.as_serialized_str()
        )
    }

    /// Normalize legacy provider/model combinations into canonical provider identities.
    pub fn normalized(&self) -> Self {
        Self {
            provider: ModelId::normalize_provider_for_model(self.model, self.provider),
            model: self.model,
        }
    }
}

/// Canonical model identifier.
///
/// This replaces the old large enum with a lightweight value object backed by
/// the provider/model catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, TS, Type)]
#[ts(type = "string")]
pub struct ModelId(&'static str);

impl Serialize for ModelId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.0)
    }
}

impl<'de> Deserialize<'de> for ModelId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::from_api_name(&raw)
            .or_else(|| Self::from_canonical_id(&raw))
            .or_else(|| Self::from_serialized_str(&raw))
            .ok_or_else(|| serde::de::Error::custom(format!("unknown model: {raw}")))
    }
}

#[allow(non_upper_case_globals)]
impl ModelId {
    pub const Gpt5: Self = Self("gpt-5");
    pub const Gpt5Mini: Self = Self("gpt-5-mini");
    pub const Gpt5Nano: Self = Self("gpt-5-nano");
    pub const Gpt5Pro: Self = Self("gpt-5-pro");
    pub const Gpt5_1: Self = Self("gpt-5-1");
    pub const Gpt5_2: Self = Self("gpt-5-2");
    pub const ClaudeOpus4_6: Self = Self("claude-opus-4-6");
    pub const ClaudeSonnet4_5: Self = Self("claude-sonnet-4-5");
    pub const ClaudeHaiku4_5: Self = Self("claude-haiku-4-5");
    pub const ClaudeCodeOpus: Self = Self("claude-code-opus");
    pub const ClaudeCodeSonnet: Self = Self("claude-code-sonnet");
    pub const ClaudeCodeHaiku: Self = Self("claude-code-haiku");
    pub const DeepseekChat: Self = Self("deepseek-chat");
    pub const DeepseekReasoner: Self = Self("deepseek-reasoner");
    pub const Gemini25Pro: Self = Self("gemini-2-5-pro");
    pub const Gemini25Flash: Self = Self("gemini-2-5-flash");
    pub const Gemini3Pro: Self = Self("gemini-3-pro");
    pub const Gemini3Flash: Self = Self("gemini-3-flash");
    pub const GroqLlama4Scout: Self = Self("groq-llama4-scout");
    pub const GroqLlama4Maverick: Self = Self("groq-llama4-maverick");
    pub const Grok4: Self = Self("grok-4");
    pub const Grok3Mini: Self = Self("grok-3-mini");
    pub const OpenRouterAuto: Self = Self("openrouter");
    pub const OrClaudeOpus4_6: Self = Self("or-claude-opus-4-6");
    pub const OrGpt5: Self = Self("or-gpt-5");
    pub const OrGemini3Pro: Self = Self("or-gemini-3-pro");
    pub const OrDeepseekV3_2: Self = Self("or-deepseek-v3-2");
    pub const OrGrok4: Self = Self("or-grok-4");
    pub const OrLlama4Maverick: Self = Self("or-llama-4-maverick");
    pub const OrQwen3Coder: Self = Self("or-qwen3-coder");
    pub const OrDevstral2: Self = Self("or-devstral-2");
    pub const OrGlm4_7: Self = Self("or-glm-4-7");
    pub const OrKimiK2_5: Self = Self("or-kimi-k2-5");
    pub const OrMinimaxM2_1: Self = Self("or-minimax-m2-1");
    pub const Qwen3Max: Self = Self("qwen3-max");
    pub const Qwen3Plus: Self = Self("qwen3-plus");
    pub const Glm5: Self = Self("glm-5");
    pub const Glm5Turbo: Self = Self("glm-5-turbo");
    pub const Glm5Code: Self = Self("glm-5-code");
    pub const Glm4_7: Self = Self("glm-4-7");
    pub const Glm5CodingPlan: Self = Self("zai-coding-plan-glm-5");
    pub const Glm5TurboCodingPlan: Self = Self("zai-coding-plan-glm-5-turbo");
    pub const Glm5CodeCodingPlan: Self = Self("zai-coding-plan-glm-5-code");
    pub const Glm4_7CodingPlan: Self = Self("zai-coding-plan-glm-4-7");
    pub const KimiK2_5: Self = Self("kimi-k2-5");
    pub const DoubaoPro: Self = Self("doubao-pro");
    pub const YiLightning: Self = Self("yi-lightning");
    pub const SiliconFlowAuto: Self = Self("siliconflow");
    pub const MiniMaxM21: Self = Self("minimax-m2-1");
    pub const MiniMaxM25: Self = Self("minimax-m2-5");
    pub const MiniMaxM27: Self = Self("minimax-m2-7");
    pub const MiniMaxM27Highspeed: Self = Self("minimax-m2-7-highspeed");
    pub const MiniMaxM21CodingPlan: Self = Self("minimax-coding-plan-m2-1");
    pub const MiniMaxM25CodingPlan: Self = Self("minimax-coding-plan-m2-5");
    pub const MiniMaxM25CodingPlanHighspeed: Self = Self("minimax-coding-plan-m2-5-highspeed");
    pub const Gpt5_4Codex: Self = Self("gpt-5.4");
    pub const Gpt5_4MiniCodex: Self = Self("gpt-5.4-mini");
    pub const Gpt5Codex: Self = Self("gpt-5-codex");
    pub const Gpt5_1Codex: Self = Self("gpt-5.1-codex");
    pub const Gpt5_2Codex: Self = Self("gpt-5.2-codex");
    pub const CodexCli: Self = Self("gpt-5.3-codex");
    pub const OpenCodeCli: Self = Self("opencode-cli");
    pub const GeminiCli: Self = Self("gemini-cli");

    pub const fn as_serialized_str(&self) -> &'static str {
        self.0
    }

    pub fn from_serialized_str(value: &str) -> Option<Self> {
        let normalized = value.trim();
        if normalized.is_empty() {
            return None;
        }

        catalog::lookup_by_name(normalized)
    }

    pub fn all() -> &'static [Self] {
        catalog::all_model_ids()
    }
}

impl ModelId {
    fn descriptor(&self) -> &'static catalog::ModelDescriptor {
        catalog::descriptor(*self).unwrap_or_else(|| {
            panic!(
                "missing model catalog entry for {}",
                self.as_serialized_str()
            )
        })
    }

    /// Convert ModelId to ModelSpec used by runtime LLM factory.
    pub fn as_model_spec(&self) -> ModelSpec {
        let provider = self.provider().as_llm_provider();
        if self.is_opencode_cli() {
            ModelSpec::opencode(self.as_serialized_str(), self.as_str())
        } else if self.is_codex_cli() {
            ModelSpec::codex(self.as_serialized_str(), self.as_str())
        } else if self.is_gemini_cli() {
            ModelSpec::gemini_cli(self.as_serialized_str(), self.as_str())
        } else if matches!(self.provider(), Provider::ZaiCodingPlan)
            || matches!(*self, Self::Glm5Code)
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
        self.descriptor().metadata()
    }

    /// Get the provider for this model
    pub fn provider(&self) -> Provider {
        self.metadata().provider
    }

    /// Normalize a provider against model-specific canonical ownership.
    pub fn normalize_provider_for_model(model: ModelId, provider: Provider) -> Provider {
        if model.is_claude_code() && provider == Provider::Anthropic {
            Provider::ClaudeCode
        } else if model.is_codex_cli() && provider == Provider::OpenAI {
            Provider::Codex
        } else {
            provider
        }
    }

    /// Check whether the provider matches the model, allowing legacy stored provider values.
    pub fn provider_matches(&self, provider: Provider) -> bool {
        Self::normalize_provider_for_model(*self, provider) == self.provider()
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

    /// Parse a canonical model ID back to ModelId.
    /// Accepts both "provider:model" format and legacy model-only strings.
    ///
    /// Returns None if the model string is not recognized.
    pub fn from_canonical_id(canonical_id: &str) -> Option<Self> {
        let normalized = canonical_id.trim().to_lowercase();

        // Try "provider:model" format first
        if let Some((provider_str, model_str)) = normalized.split_once(':')
            && let Some(provider) = Provider::from_canonical_str(provider_str)
        {
            return Self::for_provider_and_model(provider, model_str);
        }

        catalog::lookup_by_name(&normalized)
    }

    /// Check if this model supports temperature parameter
    pub fn supports_temperature(&self) -> bool {
        self.metadata().supports_temperature
    }

    /// Normalize accepted model identifiers into serialized enum form.
    ///
    /// Examples:
    /// - "MiniMax-M2.5" -> "minimax-m2-5"
    /// - "gpt-5.1" -> "gpt-5-1"
    /// - "openai:gpt-5" -> "gpt-5"
    pub fn normalize_model_id(input: &str) -> Option<String> {
        let normalized = input.trim();
        if normalized.is_empty() {
            return None;
        }

        Self::from_api_name(normalized)
            .or_else(|| Self::from_canonical_id(normalized))
            .map(|model| model.as_serialized_str().to_string())
    }

    /// Normalize model identifiers using a provider hint before falling back
    /// to global lookup. This avoids collisions between providers that expose
    /// overlapping model families or aliases.
    pub fn normalize_model_id_for_provider(provider: Provider, input: &str) -> Option<String> {
        let normalized = input.trim();
        if normalized.is_empty() {
            return None;
        }

        Self::for_provider_and_model(provider, normalized)
            .or_else(|| Self::from_canonical_id(normalized))
            .or_else(|| Self::from_api_name(normalized))
            .filter(|model| model.provider_matches(provider))
            .map(|model| model.as_serialized_str().to_string())
    }

    /// Get the string representation used for API calls
    pub fn as_str(&self) -> &'static str {
        self.descriptor().api_name
    }

    /// Convert an API model name into an ModelId.
    pub fn from_api_name(name: &str) -> Option<Self> {
        let normalized = name.trim();
        if normalized.is_empty() {
            return None;
        }

        if let Some(model) = catalog::lookup_by_name(normalized) {
            return Some(model);
        }

        match normalized.to_ascii_lowercase().as_str() {
            "gpt-5.4-codex" => Some(Self::Gpt5_4Codex),
            "gpt-5.4-mini-codex" => Some(Self::Gpt5_4MiniCodex),
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
            "gpt-5.4-codex" => "gpt-5.4",
            "gpt-5.4-mini-codex" => "gpt-5.4-mini",
            "glm5" => "glm-5",
            "glm5-turbo" => "glm-5-turbo",
            "glm5-code" => "glm-5-code",
            "glm-4.7" => "glm-4-7",
            "minimax-m2.1" => "minimax-m2-1",
            "minimax-m2.5" => "minimax-m2-5",
            "minimax-m2.7" => "minimax-m2-7",
            "minimax-m2.7-highspeed" => "minimax-m2-7-highspeed",
            value => value,
        };

        catalog::lookup_for_provider(provider, canonical).or_else(|| {
            let parsed = Self::from_api_name(canonical)?;
            parsed.provider_matches(provider).then_some(parsed)
        })
    }

    /// Remap this model into another provider when a provider-specific counterpart exists.
    pub fn remap_provider(&self, provider: Provider) -> Option<Self> {
        if self.provider() == provider {
            return Some(*self);
        }

        let canonical_family = self.descriptor().canonical_family?;
        catalog::lookup_by_canonical_family(provider, canonical_family)
    }

    /// Get the display name for UI
    pub fn display_name(&self) -> &'static str {
        self.metadata().name
    }

    /// Check if this model uses the Codex CLI
    pub fn is_codex_cli(&self) -> bool {
        matches!(
            *self,
            Self::Gpt5_4Codex
                | Self::Gpt5_4MiniCodex
                | Self::Gpt5Codex
                | Self::Gpt5_1Codex
                | Self::Gpt5_2Codex
                | Self::CodexCli
        )
    }

    /// Check if this model uses the Claude Code CLI
    pub fn is_claude_code(&self) -> bool {
        matches!(
            *self,
            Self::ClaudeCodeOpus | Self::ClaudeCodeSonnet | Self::ClaudeCodeHaiku
        )
    }

    /// Check if this model uses the OpenCode CLI
    pub fn is_opencode_cli(&self) -> bool {
        matches!(*self, Self::OpenCodeCli)
    }

    /// Check if this model uses the Gemini CLI
    pub fn is_gemini_cli(&self) -> bool {
        matches!(*self, Self::GeminiCli)
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
        self.descriptor().same_provider_fallback
    }

    /// Get the OpenRouter equivalent of this model (if one exists).
    pub fn openrouter_equivalent(&self) -> Option<Self> {
        self.descriptor().openrouter_equivalent
    }

    /// Convert metadata to serializable DTO for frontend
    pub fn to_metadata_dto(&self) -> ModelMetadataDTO {
        self.descriptor().metadata_dto()
    }

    /// Get all models with their metadata as DTOs
    pub fn all_with_metadata() -> Vec<ModelMetadataDTO> {
        catalog::all_descriptors()
            .map(catalog::ModelDescriptor::metadata_dto)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider() {
        assert_eq!(ModelId::Gpt5.provider(), Provider::OpenAI);
        assert_eq!(ModelId::ClaudeSonnet4_5.provider(), Provider::Anthropic);
        assert_eq!(ModelId::ClaudeCodeSonnet.provider(), Provider::ClaudeCode);
        assert_eq!(ModelId::DeepseekChat.provider(), Provider::DeepSeek);
        assert_eq!(ModelId::Gemini25Pro.provider(), Provider::Google);
        assert_eq!(ModelId::GroqLlama4Scout.provider(), Provider::Groq);
        assert_eq!(ModelId::Grok4.provider(), Provider::XAI);
        assert_eq!(ModelId::Qwen3Max.provider(), Provider::Qwen);
        assert_eq!(ModelId::Glm4_7.provider(), Provider::Zai);
        assert_eq!(ModelId::Glm5Turbo.provider(), Provider::Zai);
        assert_eq!(ModelId::Glm5CodingPlan.provider(), Provider::ZaiCodingPlan);
        assert_eq!(
            ModelId::Glm5TurboCodingPlan.provider(),
            Provider::ZaiCodingPlan
        );
        assert_eq!(ModelId::KimiK2_5.provider(), Provider::Moonshot);
        assert_eq!(ModelId::DoubaoPro.provider(), Provider::Doubao);
        assert_eq!(ModelId::YiLightning.provider(), Provider::Yi);
        assert_eq!(ModelId::MiniMaxM25.provider(), Provider::MiniMax);
        assert_eq!(ModelId::MiniMaxM21.provider(), Provider::MiniMax);
        assert_eq!(ModelId::MiniMaxM27.provider(), Provider::MiniMax);
        assert_eq!(ModelId::MiniMaxM27Highspeed.provider(), Provider::MiniMax);
        assert_eq!(
            ModelId::MiniMaxM25CodingPlan.provider(),
            Provider::MiniMaxCodingPlan
        );
        assert_eq!(
            ModelId::MiniMaxM25CodingPlanHighspeed.provider(),
            Provider::MiniMaxCodingPlan
        );
        assert_eq!(ModelId::CodexCli.provider(), Provider::Codex);
        assert_eq!(ModelId::Gpt5_4Codex.provider(), Provider::Codex);
        assert_eq!(ModelId::Gpt5_4MiniCodex.provider(), Provider::Codex);
    }

    #[test]
    fn test_supports_temperature() {
        // Models that don't support temperature
        assert!(!ModelId::Gpt5.supports_temperature());
        assert!(!ModelId::Gpt5Mini.supports_temperature());
        assert!(!ModelId::Gpt5_1.supports_temperature());
        assert!(!ModelId::Gpt5_2.supports_temperature());
        assert!(!ModelId::Gpt5Codex.supports_temperature());
        assert!(!ModelId::Gpt5_4Codex.supports_temperature());
        assert!(!ModelId::Gpt5_4MiniCodex.supports_temperature());
        assert!(!ModelId::Gpt5_1Codex.supports_temperature());
        assert!(!ModelId::Gpt5_2Codex.supports_temperature());
        assert!(!ModelId::CodexCli.supports_temperature());
        assert!(!ModelId::OpenCodeCli.supports_temperature());
        assert!(!ModelId::GeminiCli.supports_temperature());

        // Models that support temperature
        assert!(ModelId::ClaudeSonnet4_5.supports_temperature());
        assert!(ModelId::ClaudeHaiku4_5.supports_temperature());
        assert!(ModelId::DeepseekChat.supports_temperature());
        assert!(ModelId::Gemini25Flash.supports_temperature());
        assert!(ModelId::GroqLlama4Maverick.supports_temperature());
    }

    #[test]
    fn test_is_codex_cli() {
        assert!(ModelId::Gpt5Codex.is_codex_cli());
        assert!(ModelId::Gpt5_4Codex.is_codex_cli());
        assert!(ModelId::Gpt5_4MiniCodex.is_codex_cli());
        assert!(ModelId::Gpt5_1Codex.is_codex_cli());
        assert!(ModelId::Gpt5_2Codex.is_codex_cli());
        assert!(ModelId::CodexCli.is_codex_cli());
        assert!(!ModelId::Gpt5.is_codex_cli());
    }

    #[test]
    fn test_is_opencode_cli() {
        assert!(ModelId::OpenCodeCli.is_opencode_cli());
        assert!(!ModelId::Gpt5.is_opencode_cli());
    }

    #[test]
    fn test_is_gemini_cli() {
        assert!(ModelId::GeminiCli.is_gemini_cli());
        assert!(!ModelId::Gpt5.is_gemini_cli());
    }

    #[test]
    fn test_as_str() {
        assert_eq!(ModelId::Gpt5.as_str(), "gpt-5");
        assert_eq!(ModelId::Gpt5_1.as_str(), "gpt-5.1");
        assert_eq!(ModelId::ClaudeSonnet4_5.as_str(), "claude-sonnet-4-5");
        assert_eq!(ModelId::ClaudeHaiku4_5.as_str(), "claude-haiku-4-5");
        assert_eq!(ModelId::Gpt5Codex.as_str(), "gpt-5-codex");
        assert_eq!(ModelId::Gpt5_4Codex.as_str(), "gpt-5.4");
        assert_eq!(ModelId::Gpt5_4MiniCodex.as_str(), "gpt-5.4-mini");
        assert_eq!(ModelId::Gpt5_1Codex.as_str(), "gpt-5.1-codex");
        assert_eq!(ModelId::Gpt5_2Codex.as_str(), "gpt-5.2-codex");
        assert_eq!(ModelId::CodexCli.as_str(), "gpt-5.3-codex");
        assert_eq!(ModelId::OpenCodeCli.as_str(), "opencode");
        assert_eq!(ModelId::GeminiCli.as_str(), "gemini-2.5-pro");
        assert_eq!(ModelId::MiniMaxM21.as_str(), "MiniMax-M2.1");
        assert_eq!(ModelId::MiniMaxM27.as_str(), "MiniMax-M2.7");
        assert_eq!(
            ModelId::MiniMaxM27Highspeed.as_str(),
            "MiniMax-M2.7-highspeed"
        );
        assert_eq!(ModelId::MiniMaxM21CodingPlan.as_str(), "MiniMax-M2.1");
        assert_eq!(ModelId::MiniMaxM25CodingPlan.as_str(), "MiniMax-M2.5");
        assert_eq!(
            ModelId::MiniMaxM25CodingPlanHighspeed.as_str(),
            "MiniMax-M2.5-highspeed"
        );
        assert_eq!(ModelId::Glm5Turbo.as_str(), "glm-5-turbo");
        assert_eq!(ModelId::Glm5Code.as_str(), "glm-5");
        assert_eq!(ModelId::Glm5CodingPlan.as_str(), "glm-5");
        assert_eq!(ModelId::Glm5TurboCodingPlan.as_str(), "glm-5-turbo");
        assert_eq!(ModelId::DeepseekChat.as_str(), "deepseek-chat");
        assert_eq!(ModelId::Gemini25Pro.as_str(), "gemini-2.5-pro");
        assert_eq!(
            ModelId::GroqLlama4Scout.as_str(),
            "meta-llama/llama-4-scout-17b-16e-instruct"
        );
    }

    #[test]
    fn test_from_api_name() {
        assert_eq!(
            ModelId::from_api_name("claude-sonnet-4-5-20250514"),
            Some(ModelId::ClaudeSonnet4_5)
        );
        assert_eq!(
            ModelId::from_api_name("claude-sonnet-4-20250514"),
            Some(ModelId::ClaudeSonnet4_5)
        );
        assert_eq!(ModelId::from_api_name("nonexistent"), None);
    }

    #[test]
    fn test_for_provider_and_model() {
        assert_eq!(
            ModelId::for_provider_and_model(Provider::MiniMax, "minimax-m2-5"),
            Some(ModelId::MiniMaxM25)
        );
        assert_eq!(
            ModelId::for_provider_and_model(Provider::MiniMax, "minimax-m2.7"),
            Some(ModelId::MiniMaxM27)
        );
        assert_eq!(
            ModelId::for_provider_and_model(Provider::MiniMax, "minimax-m2-7-highspeed"),
            Some(ModelId::MiniMaxM27Highspeed)
        );
        assert_eq!(
            ModelId::for_provider_and_model(Provider::MiniMaxCodingPlan, "minimax-m2-5"),
            Some(ModelId::MiniMaxM25CodingPlan)
        );
        assert_eq!(
            ModelId::for_provider_and_model(Provider::MiniMaxCodingPlan, "minimax-m2.5-highspeed"),
            Some(ModelId::MiniMaxM25CodingPlanHighspeed)
        );
        assert_eq!(
            ModelId::for_provider_and_model(Provider::ZaiCodingPlan, "glm-5"),
            Some(ModelId::Glm5CodingPlan)
        );
        assert_eq!(
            ModelId::for_provider_and_model(Provider::Zai, "glm-5-turbo"),
            Some(ModelId::Glm5Turbo)
        );
        assert_eq!(
            ModelId::for_provider_and_model(Provider::ZaiCodingPlan, "glm5-turbo"),
            Some(ModelId::Glm5TurboCodingPlan)
        );
    }

    #[test]
    fn test_remap_provider() {
        assert_eq!(
            ModelId::MiniMaxM25.remap_provider(Provider::MiniMaxCodingPlan),
            Some(ModelId::MiniMaxM25CodingPlan)
        );
        assert_eq!(
            ModelId::MiniMaxM27.remap_provider(Provider::MiniMaxCodingPlan),
            None
        );
        assert_eq!(
            ModelId::MiniMaxM25CodingPlanHighspeed.remap_provider(Provider::MiniMax),
            None
        );
        assert_eq!(
            ModelId::Glm5CodingPlan.remap_provider(Provider::Zai),
            Some(ModelId::Glm5)
        );
        assert_eq!(
            ModelId::Glm5Turbo.remap_provider(Provider::ZaiCodingPlan),
            Some(ModelId::Glm5TurboCodingPlan)
        );
        assert_eq!(
            ModelId::ClaudeSonnet4_5.remap_provider(Provider::MiniMax),
            None
        );
    }

    #[test]
    fn test_display_name() {
        assert_eq!(ModelId::Gpt5.display_name(), "GPT-5");
        assert_eq!(ModelId::Gpt5_2.display_name(), "GPT-5.2");
        assert_eq!(ModelId::ClaudeSonnet4_5.display_name(), "Claude Sonnet 4.5");
        assert_eq!(ModelId::ClaudeHaiku4_5.display_name(), "Claude Haiku 4.5");
        assert_eq!(ModelId::Gpt5Codex.display_name(), "Codex GPT-5");
        assert_eq!(ModelId::Gpt5_4Codex.display_name(), "GPT-5.4");
        assert_eq!(ModelId::Gpt5_4MiniCodex.display_name(), "GPT-5.4 Mini");
        assert_eq!(ModelId::Gpt5_1Codex.display_name(), "Codex GPT-5.1");
        assert_eq!(ModelId::Gpt5_2Codex.display_name(), "Codex GPT-5.2");
        assert_eq!(ModelId::CodexCli.display_name(), "Codex GPT-5.3");
        assert_eq!(ModelId::OpenCodeCli.display_name(), "OpenCode CLI");
        assert_eq!(ModelId::GeminiCli.display_name(), "Gemini CLI");
        assert_eq!(ModelId::DeepseekChat.display_name(), "DeepSeek Chat");
        assert_eq!(ModelId::MiniMaxM21.display_name(), "MiniMax M2.1");
        assert_eq!(ModelId::MiniMaxM27.display_name(), "MiniMax M2.7");
        assert_eq!(
            ModelId::MiniMaxM25CodingPlanHighspeed.display_name(),
            "MiniMax M2.5 Highspeed (Coding Plan)"
        );
        assert_eq!(ModelId::Glm5Turbo.display_name(), "GLM-5 Turbo");
    }

    #[test]
    fn test_all_models() {
        let models = ModelId::all();
        assert_eq!(models.len(), 63);
        assert!(models.contains(&ModelId::Gpt5));
        assert!(models.contains(&ModelId::Gpt5_1));
        assert!(models.contains(&ModelId::ClaudeOpus4_6));
        assert!(models.contains(&ModelId::ClaudeSonnet4_5));
        assert!(models.contains(&ModelId::ClaudeHaiku4_5));
        assert!(models.contains(&ModelId::Gpt5_4Codex));
        assert!(models.contains(&ModelId::Gpt5_4MiniCodex));
        assert!(models.contains(&ModelId::Gpt5Codex));
        assert!(models.contains(&ModelId::Gpt5_1Codex));
        assert!(models.contains(&ModelId::Gpt5_2Codex));
        assert!(models.contains(&ModelId::CodexCli));
        assert!(models.contains(&ModelId::OpenCodeCli));
        assert!(models.contains(&ModelId::GeminiCli));
        assert!(models.contains(&ModelId::DeepseekChat));
        assert!(models.contains(&ModelId::Gemini25Pro));
        assert!(models.contains(&ModelId::MiniMaxM21));
        assert!(models.contains(&ModelId::MiniMaxM27));
        assert!(models.contains(&ModelId::MiniMaxM27Highspeed));
        assert!(models.contains(&ModelId::MiniMaxM21CodingPlan));
        assert!(models.contains(&ModelId::MiniMaxM25CodingPlanHighspeed));
        assert!(models.contains(&ModelId::Glm5Turbo));
        assert!(models.contains(&ModelId::Glm5TurboCodingPlan));
    }

    #[test]
    fn test_metadata() {
        // Test metadata for GPT-5 (no temperature)
        let metadata = ModelId::Gpt5.metadata();
        assert_eq!(metadata.provider, Provider::OpenAI);
        assert!(!metadata.supports_temperature);
        assert_eq!(metadata.name, "GPT-5");

        // Test metadata for Claude Sonnet 4.5 (with temperature)
        let metadata = ModelId::ClaudeSonnet4_5.metadata();
        assert_eq!(metadata.provider, Provider::Anthropic);
        assert!(metadata.supports_temperature);
        assert_eq!(metadata.name, "Claude Sonnet 4.5");

        // Test metadata for DeepSeek Chat
        let metadata = ModelId::DeepseekChat.metadata();
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
        assert_eq!(
            Provider::ClaudeCode.as_llm_provider(),
            LlmProvider::Anthropic
        );
        assert_eq!(Provider::Codex.as_llm_provider(), LlmProvider::OpenAI);
        assert_eq!(Provider::Google.as_llm_provider(), LlmProvider::Google);
    }

    #[test]
    fn test_build_model_specs_contains_codex_cli() {
        let specs = ModelId::build_model_specs();
        assert!(specs.iter().any(|spec| spec.name == "gpt-5.4"
            && spec.client_model == "gpt-5.4"
            && spec.client_kind == restflow_models::ClientKind::CodexCli));
        assert!(specs.iter().any(|spec| spec.name == "gpt-5.4-mini"
            && spec.client_model == "gpt-5.4-mini"
            && spec.client_kind == restflow_models::ClientKind::CodexCli));
        assert!(specs.iter().any(|spec| spec.name == "gpt-5-codex"
            && spec.client_kind == restflow_models::ClientKind::CodexCli));
        assert!(specs.iter().any(|spec| spec.name == "gpt-5.1-codex"
            && spec.client_kind == restflow_models::ClientKind::CodexCli));
        assert!(specs.iter().any(|spec| spec.name == "gpt-5.2-codex"
            && spec.client_kind == restflow_models::ClientKind::CodexCli));
        assert!(specs.iter().any(|spec| spec.name == "gpt-5.3-codex"
            && spec.client_kind == restflow_models::ClientKind::CodexCli));
    }

    #[test]
    fn test_glm5_code_uses_glm5_model_with_coding_endpoint() {
        let spec = ModelId::Glm5Code.as_model_spec();
        assert_eq!(spec.client_model, "glm-5");
        assert_eq!(spec.name, "glm-5-code");
        assert_eq!(
            spec.base_url.as_deref(),
            Some("https://api.z.ai/api/coding/paas/v4")
        );

        let turbo_spec = ModelId::Glm5Turbo.as_model_spec();
        assert_eq!(turbo_spec.client_model, "glm-5-turbo");
        assert_eq!(turbo_spec.name, "glm-5-turbo");
        assert_eq!(turbo_spec.base_url, None);

        let coding_plan_spec = ModelId::Glm5CodingPlan.as_model_spec();
        assert_eq!(coding_plan_spec.client_model, "glm-5");
        assert_eq!(
            coding_plan_spec.base_url.as_deref(),
            Some("https://api.z.ai/api/coding/paas/v4")
        );

        let coding_plan_turbo_spec = ModelId::Glm5TurboCodingPlan.as_model_spec();
        assert_eq!(coding_plan_turbo_spec.client_model, "glm-5-turbo");
        assert_eq!(
            coding_plan_turbo_spec.base_url.as_deref(),
            Some("https://api.z.ai/api/coding/paas/v4")
        );
    }

    #[test]
    fn test_provider_api_key_env() {
        assert_eq!(Provider::Google.api_key_env(), Some("GEMINI_API_KEY"));
        assert_eq!(Provider::Groq.api_key_env(), Some("GROQ_API_KEY"));
        assert_eq!(Provider::Qwen.api_key_env(), Some("DASHSCOPE_API_KEY"));
        assert_eq!(Provider::MiniMax.api_key_env(), Some("MINIMAX_API_KEY"));
        assert_eq!(
            Provider::MiniMaxCodingPlan.api_key_env(),
            Some("MINIMAX_CODING_PLAN_API_KEY")
        );
        assert_eq!(Provider::Zai.api_key_env(), Some("ZAI_API_KEY"));
        assert_eq!(
            Provider::ZaiCodingPlan.api_key_env(),
            Some("ZAI_CODING_PLAN_API_KEY")
        );
        assert_eq!(Provider::ClaudeCode.api_key_env(), None);
        assert_eq!(Provider::Codex.api_key_env(), None);
    }

    #[test]
    fn test_same_provider_fallback() {
        // Anthropic chain
        assert_eq!(
            ModelId::ClaudeOpus4_6.same_provider_fallback(),
            Some(ModelId::ClaudeSonnet4_5)
        );
        assert_eq!(
            ModelId::ClaudeSonnet4_5.same_provider_fallback(),
            Some(ModelId::ClaudeHaiku4_5)
        );
        assert_eq!(ModelId::ClaudeHaiku4_5.same_provider_fallback(), None);

        // OpenAI chain
        assert_eq!(
            ModelId::Gpt5Pro.same_provider_fallback(),
            Some(ModelId::Gpt5)
        );
        assert_eq!(
            ModelId::Gpt5.same_provider_fallback(),
            Some(ModelId::Gpt5Mini)
        );
        assert_eq!(
            ModelId::Gpt5Mini.same_provider_fallback(),
            Some(ModelId::Gpt5Nano)
        );
        assert_eq!(ModelId::Gpt5Nano.same_provider_fallback(), None);

        // DeepSeek chain
        assert_eq!(
            ModelId::DeepseekReasoner.same_provider_fallback(),
            Some(ModelId::DeepseekChat)
        );
        assert_eq!(ModelId::DeepseekChat.same_provider_fallback(), None);

        // GLM chain
        assert_eq!(
            ModelId::Glm5.same_provider_fallback(),
            Some(ModelId::Glm5Turbo)
        );
        assert_eq!(
            ModelId::Glm5Turbo.same_provider_fallback(),
            Some(ModelId::Glm5Code)
        );
        assert_eq!(
            ModelId::Glm5CodingPlan.same_provider_fallback(),
            Some(ModelId::Glm5TurboCodingPlan)
        );
        assert_eq!(
            ModelId::Glm5TurboCodingPlan.same_provider_fallback(),
            Some(ModelId::Glm5CodeCodingPlan)
        );
        assert_eq!(
            ModelId::MiniMaxM25CodingPlanHighspeed.same_provider_fallback(),
            Some(ModelId::MiniMaxM25CodingPlan)
        );

        // CLI models have no fallback
        assert_eq!(
            ModelId::Gpt5_4Codex.same_provider_fallback(),
            Some(ModelId::Gpt5_4MiniCodex)
        );
        assert_eq!(ModelId::Gpt5_4MiniCodex.same_provider_fallback(), None);
        assert_eq!(ModelId::CodexCli.same_provider_fallback(), None);
    }

    #[test]
    fn test_openrouter_equivalent() {
        assert_eq!(
            ModelId::ClaudeOpus4_6.openrouter_equivalent(),
            Some(ModelId::OrClaudeOpus4_6)
        );
        assert_eq!(ModelId::Gpt5.openrouter_equivalent(), Some(ModelId::OrGpt5));
        assert_eq!(
            ModelId::DeepseekChat.openrouter_equivalent(),
            Some(ModelId::OrDeepseekV3_2)
        );
        assert_eq!(
            ModelId::Glm5Turbo.openrouter_equivalent(),
            Some(ModelId::OrGlm4_7)
        );
        assert_eq!(
            ModelId::KimiK2_5.openrouter_equivalent(),
            Some(ModelId::OrKimiK2_5)
        );
        assert_eq!(
            ModelId::MiniMaxM21.openrouter_equivalent(),
            Some(ModelId::OrMinimaxM2_1)
        );
        assert_eq!(
            ModelId::MiniMaxM25.openrouter_equivalent(),
            Some(ModelId::OrMinimaxM2_1)
        );
        assert_eq!(
            ModelId::MiniMaxM27.openrouter_equivalent(),
            Some(ModelId::OrMinimaxM2_1)
        );
        assert_eq!(
            ModelId::MiniMaxM27Highspeed.openrouter_equivalent(),
            Some(ModelId::OrMinimaxM2_1)
        );
        assert_eq!(
            ModelId::MiniMaxM25CodingPlanHighspeed.openrouter_equivalent(),
            Some(ModelId::OrMinimaxM2_1)
        );
        // OR models themselves have no OR equivalent
        assert_eq!(ModelId::OrClaudeOpus4_6.openrouter_equivalent(), None);
        // CLI models have no OR equivalent
        assert_eq!(ModelId::Gpt5_4Codex.openrouter_equivalent(), None);
        assert_eq!(ModelId::Gpt5_4MiniCodex.openrouter_equivalent(), None);
        assert_eq!(ModelId::CodexCli.openrouter_equivalent(), None);
    }

    #[test]
    fn test_canonical_id() {
        // Test canonical ID generation
        assert_eq!(ModelId::Gpt5.canonical_id(), "openai:gpt-5");
        assert_eq!(
            ModelId::ClaudeSonnet4_5.canonical_id(),
            "anthropic:claude-sonnet-4-5"
        );
        assert_eq!(
            ModelId::DeepseekChat.canonical_id(),
            "deepseek:deepseek-chat"
        );
        assert_eq!(ModelId::Gemini3Pro.canonical_id(), "google:gemini-3-pro");
        assert_eq!(ModelId::OrGpt5.canonical_id(), "openrouter:or-gpt-5");
        assert_eq!(ModelId::Gpt5_4Codex.canonical_id(), "codex:gpt-5.4");
        assert_eq!(
            ModelId::Gpt5_4MiniCodex.canonical_id(),
            "codex:gpt-5.4-mini"
        );
        assert_eq!(ModelId::CodexCli.canonical_id(), "codex:gpt-5.3-codex");
        assert_eq!(
            ModelId::ClaudeCodeSonnet.canonical_id(),
            "claude-code:claude-code-sonnet"
        );
    }

    #[test]
    fn test_from_canonical_id() {
        // Test parsing canonical IDs
        assert_eq!(
            ModelId::from_canonical_id("openai:gpt-5"),
            Some(ModelId::Gpt5)
        );
        assert_eq!(
            ModelId::from_canonical_id("anthropic:claude-sonnet-4-5"),
            Some(ModelId::ClaudeSonnet4_5)
        );
        assert_eq!(
            ModelId::from_canonical_id("deepseek:deepseek-chat"),
            Some(ModelId::DeepseekChat)
        );
        assert_eq!(
            ModelId::from_canonical_id("claude-code:claude-code-sonnet"),
            Some(ModelId::ClaudeCodeSonnet)
        );
        assert_eq!(
            ModelId::from_canonical_id("codex:gpt-5.3-codex"),
            Some(ModelId::CodexCli)
        );
        assert_eq!(
            ModelId::from_canonical_id("codex:gpt-5.4"),
            Some(ModelId::Gpt5_4Codex)
        );
        assert_eq!(
            ModelId::from_canonical_id("codex:gpt-5.4-mini"),
            Some(ModelId::Gpt5_4MiniCodex)
        );
        assert_eq!(
            ModelId::from_canonical_id("codex:gpt-5.4-codex"),
            Some(ModelId::Gpt5_4Codex)
        );
        assert_eq!(
            ModelId::from_canonical_id("codex:gpt-5.4-mini-codex"),
            Some(ModelId::Gpt5_4MiniCodex)
        );
        assert_eq!(
            ModelId::from_canonical_id("anthropic:claude-code-sonnet"),
            Some(ModelId::ClaudeCodeSonnet)
        );
        assert_eq!(
            ModelId::from_canonical_id("openai:gpt-5.3-codex"),
            Some(ModelId::CodexCli)
        );
        assert_eq!(
            ModelId::from_canonical_id("openai:gpt-5.4"),
            Some(ModelId::Gpt5_4Codex)
        );

        // Test legacy model-only strings (fallback)
        assert_eq!(ModelId::from_canonical_id("gpt-5"), Some(ModelId::Gpt5));
        assert_eq!(
            ModelId::from_canonical_id("claude-sonnet-4-5"),
            Some(ModelId::ClaudeSonnet4_5)
        );

        // Test invalid IDs
        assert_eq!(ModelId::from_canonical_id("unknown:model"), None);
        assert_eq!(ModelId::from_canonical_id("invalid-model"), None);
    }

    #[test]
    fn test_canonical_id_round_trip() {
        // Test round-trip: canonical_id -> from_canonical_id
        for model in ModelId::all() {
            let canonical = model.canonical_id();
            let parsed = ModelId::from_canonical_id(&canonical);
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
    fn test_model_ref_from_model_is_consistent() {
        let model_ref = ModelRef::from_model(ModelId::Gpt5);
        assert_eq!(model_ref.provider, Provider::OpenAI);
        assert_eq!(model_ref.model, ModelId::Gpt5);
        assert_eq!(model_ref.canonical_id(), "openai:gpt-5");
        assert!(model_ref.validate().is_ok());
    }

    #[test]
    fn test_model_ref_validate_rejects_provider_mismatch() {
        let model_ref = ModelRef {
            provider: Provider::Anthropic,
            model: ModelId::Gpt5,
        };
        let error = model_ref
            .validate()
            .expect_err("provider mismatch should fail");
        assert_eq!(error.field, "model_ref");
        assert!(error.message.contains("does not match"));
    }

    #[test]
    fn test_model_ref_validate_accepts_legacy_cli_provider_pairs() {
        let claude_code_ref = ModelRef {
            provider: Provider::Anthropic,
            model: ModelId::ClaudeCodeSonnet,
        };
        assert!(claude_code_ref.validate().is_ok());
        assert_eq!(
            claude_code_ref.normalized(),
            ModelRef {
                provider: Provider::ClaudeCode,
                model: ModelId::ClaudeCodeSonnet,
            }
        );

        let codex_ref = ModelRef {
            provider: Provider::OpenAI,
            model: ModelId::Gpt5_4Codex,
        };
        assert!(codex_ref.validate().is_ok());
        assert_eq!(
            codex_ref.normalized(),
            ModelRef {
                provider: Provider::Codex,
                model: ModelId::Gpt5_4Codex,
            }
        );
    }

    #[test]
    fn test_provider_canonical_str() {
        // Test provider canonical strings
        assert_eq!(Provider::OpenAI.as_canonical_str(), "openai");
        assert_eq!(Provider::Anthropic.as_canonical_str(), "anthropic");
        assert_eq!(Provider::DeepSeek.as_canonical_str(), "deepseek");
        assert_eq!(Provider::Google.as_canonical_str(), "google");
        assert_eq!(Provider::OpenRouter.as_canonical_str(), "openrouter");
        assert_eq!(Provider::ClaudeCode.as_canonical_str(), "claude-code");
        assert_eq!(Provider::Codex.as_canonical_str(), "codex");
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
        // Test parsing provider canonical strings and supported aliases
        assert_eq!(
            Provider::from_canonical_str("openai"),
            Some(Provider::OpenAI)
        );
        assert_eq!(Provider::from_canonical_str("gpt"), Some(Provider::OpenAI));
        assert_eq!(
            Provider::from_canonical_str("anthropic"),
            Some(Provider::Anthropic)
        );
        assert_eq!(
            Provider::from_canonical_str("claude-code"),
            Some(Provider::ClaudeCode)
        );
        assert_eq!(Provider::from_canonical_str("codex"), Some(Provider::Codex));
        assert_eq!(
            Provider::from_canonical_str("openai-codex"),
            Some(Provider::Codex)
        );
        assert_eq!(
            Provider::from_canonical_str("deepseek"),
            Some(Provider::DeepSeek)
        );
        assert_eq!(
            Provider::from_canonical_str("google"),
            Some(Provider::Google)
        );
        assert_eq!(
            Provider::from_canonical_str("gemini"),
            Some(Provider::Google)
        );
        assert_eq!(
            Provider::from_canonical_str("zhipu-coding-plan"),
            Some(Provider::ZaiCodingPlan)
        );
        assert_eq!(Provider::from_canonical_str("invalid"), None);
    }

    #[test]
    fn test_normalize_model_id() {
        assert_eq!(
            ModelId::normalize_model_id("MiniMax-M2.5"),
            Some("minimax-m2-5".to_string())
        );
        assert_eq!(
            ModelId::normalize_model_id("MiniMax-M2.7"),
            Some("minimax-m2-7".to_string())
        );
        assert_eq!(
            ModelId::normalize_model_id("MiniMax-M2.7-highspeed"),
            Some("minimax-m2-7-highspeed".to_string())
        );
        assert_eq!(
            ModelId::normalize_model_id("gpt-5.1"),
            Some("gpt-5-1".to_string())
        );
        assert_eq!(
            ModelId::normalize_model_id("openai:gpt-5"),
            Some("gpt-5".to_string())
        );
        assert_eq!(
            ModelId::normalize_model_id("claude-sonnet-4-20250514"),
            Some("claude-sonnet-4-5".to_string())
        );
        assert_eq!(ModelId::normalize_model_id(""), None);
    }

    #[test]
    fn test_normalize_model_id_for_provider_avoids_minimax_collision() {
        assert_eq!(
            ModelId::normalize_model_id_for_provider(Provider::MiniMaxCodingPlan, "MiniMax-M2.5"),
            Some("minimax-coding-plan-m2-5".to_string())
        );
        assert_eq!(
            ModelId::normalize_model_id_for_provider(
                Provider::MiniMaxCodingPlan,
                "MiniMax-M2.5-highspeed"
            ),
            Some("minimax-coding-plan-m2-5-highspeed".to_string())
        );
        assert_eq!(
            ModelId::normalize_model_id_for_provider(Provider::MiniMax, "MiniMax-M2.5"),
            Some("minimax-m2-5".to_string())
        );
    }

    #[test]
    fn test_flagship_model() {
        assert_eq!(
            Provider::Anthropic.flagship_model(),
            ModelId::ClaudeSonnet4_5
        );
        assert_eq!(Provider::OpenAI.flagship_model(), ModelId::Gpt5);
        assert_eq!(Provider::DeepSeek.flagship_model(), ModelId::DeepseekChat);
        assert_eq!(Provider::Google.flagship_model(), ModelId::Gemini3Pro);
        assert_eq!(Provider::MiniMax.flagship_model(), ModelId::MiniMaxM27);
        assert_eq!(Provider::Zai.flagship_model(), ModelId::Glm5);
        assert_eq!(
            Provider::ZaiCodingPlan.flagship_model(),
            ModelId::Glm5CodingPlan
        );
        assert_eq!(
            Provider::MiniMaxCodingPlan.flagship_model(),
            ModelId::MiniMaxM25CodingPlan
        );
        assert_eq!(
            Provider::ClaudeCode.flagship_model(),
            ModelId::ClaudeCodeOpus
        );
        assert_eq!(Provider::Codex.flagship_model(), ModelId::Gpt5_4Codex);
        assert_eq!(
            Provider::OpenRouter.flagship_model(),
            ModelId::OrClaudeOpus4_6
        );
    }

    #[test]
    fn test_provider_catalog_completeness() {
        assert_eq!(Provider::all().len(), catalog::PROVIDER_CATALOGS.len());

        for provider in Provider::all() {
            let provider_catalog = catalog::provider_catalog(*provider)
                .unwrap_or_else(|| panic!("missing provider catalog for {provider:?}"));
            assert_eq!(provider_catalog.provider, *provider);
            assert_eq!(provider_catalog.flagship.provider(), *provider);
            assert!(!provider_catalog.models.is_empty());
        }
    }

    #[test]
    fn test_catalog_lookup_round_trips_model_ids() {
        for model in ModelId::all() {
            let descriptor = catalog::descriptor(*model)
                .unwrap_or_else(|| panic!("missing descriptor for {model:?}"));
            assert_eq!(descriptor.id, *model);
            assert_eq!(descriptor.provider, model.provider());
            assert_eq!(
                catalog::lookup_by_name(model.as_serialized_str()),
                Some(*model)
            );
            let resolved = catalog::lookup_for_provider(model.provider(), model.as_str())
                .unwrap_or_else(|| {
                    panic!(
                        "missing provider lookup for {} via {}",
                        model.as_serialized_str(),
                        model.as_str()
                    )
                });
            assert_eq!(resolved.provider(), model.provider());
            assert_eq!(resolved.as_str(), model.as_str());
        }
    }

    #[test]
    fn test_minimax_m25_serialization_consistency() {
        // as_serialized_str() must match the serde rename
        let json_str = serde_json::to_string(&ModelId::MiniMaxM25).unwrap();
        let expected = format!("\"{}\"", ModelId::MiniMaxM25.as_serialized_str());
        assert_eq!(json_str, expected);
    }

    #[test]
    fn test_from_api_name_trimmed_input() {
        // Whitespace around model name should still resolve
        assert_eq!(
            ModelId::from_api_name("  Claude-Sonnet-4-5-20250514  "),
            Some(ModelId::ClaudeSonnet4_5)
        );
    }
}
