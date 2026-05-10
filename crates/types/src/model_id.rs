use crate::{ClientKind, ModelMetadata, ModelMetadataDTO, ModelSpec, Provider, catalog};
use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;

/// Canonical model identifier.
///
/// This replaces the old large enum with a lightweight value object backed by
/// the provider/model catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Type)]
pub struct ModelId(&'static str);

#[allow(non_upper_case_globals)]
impl ModelId {
    pub const Gpt5: Self = Self("gpt-5");
    pub const Gpt5Mini: Self = Self("gpt-5-mini");
    pub const Gpt5Nano: Self = Self("gpt-5-nano");
    pub const Gpt5Pro: Self = Self("gpt-5-pro");
    pub const Gpt5_1: Self = Self("gpt-5-1");
    pub const Gpt5_2: Self = Self("gpt-5-2");
    pub const Gpt5_4: Self = Self("gpt-5-4");
    pub const Gpt5_4Mini: Self = Self("gpt-5-4-mini");
    pub const Gpt5_4Nano: Self = Self("gpt-5-4-nano");
    pub const Gpt5_5: Self = Self("gpt-5-5");
    pub const Gpt5_5Pro: Self = Self("gpt-5-5-pro");
    pub const ClaudeOpus4_6: Self = Self("claude-opus-4-6");
    pub const ClaudeSonnet4_5: Self = Self("claude-sonnet-4-5");
    pub const ClaudeHaiku4_5: Self = Self("claude-haiku-4-5");
    pub const ClaudeCodeOpus: Self = Self("claude-code-opus");
    pub const ClaudeCodeSonnet: Self = Self("claude-code-sonnet");
    pub const ClaudeCodeHaiku: Self = Self("claude-code-haiku");
    pub const DeepseekChat: Self = Self("deepseek-chat");
    pub const DeepseekReasoner: Self = Self("deepseek-reasoner");
    pub const DeepseekV4Pro: Self = Self("deepseek-v4-pro");
    pub const DeepseekV4Flash: Self = Self("deepseek-v4-flash");
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
    pub const Glm5_1CodingPlan: Self = Self("zai-coding-plan-glm-5-1");
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
    pub const MiniMaxM27CodingPlan: Self = Self("minimax-coding-plan-m2-7");
    pub const MiniMaxM27CodingPlanHighspeed: Self = Self("minimax-coding-plan-m2-7-highspeed");
    pub const Gpt5_4Codex: Self = Self("gpt-5.4");
    pub const Gpt5_4MiniCodex: Self = Self("gpt-5.4-mini");
    pub const Gpt5_5Codex: Self = Self("gpt-5.5");
    pub const Gpt5_5ProCodex: Self = Self("gpt-5.5-pro");
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

impl ModelId {
    fn model_spec_named(&self, name: &str) -> ModelSpec {
        let descriptor = self.descriptor();
        let provider = self.provider().as_llm_provider();
        let mut spec = match descriptor.client_kind {
            ClientKind::Http => ModelSpec::new(name, provider, self.as_str()),
            ClientKind::CodexCli => ModelSpec::codex(name, self.as_str()),
            ClientKind::OpenCodeCli => ModelSpec::opencode(name, self.as_str()),
            ClientKind::GeminiCli => ModelSpec::gemini_cli(name, self.as_str()),
            ClientKind::ClaudeCodeCli => ModelSpec::claude_code(name, self.as_str()),
        };

        if let Some(base_url) = descriptor.base_url_override {
            spec = spec.with_base_url(base_url);
        }

        spec
    }

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
        self.model_spec_named(self.as_serialized_str())
    }

    /// Build the shared model catalog for dynamic model switching.
    pub fn build_model_specs() -> Vec<ModelSpec> {
        let mut specs = Vec::new();
        for model in Self::all() {
            specs.push(model.as_model_spec());

            // OpenAI 5.4 API models use serialized IDs to avoid colliding with
            // Codex CLI IDs, but runtime switching should accept the public API
            // names for OpenAI provider usage.
            if matches!(model.provider(), Provider::OpenAI)
                && model.as_str() != model.as_serialized_str()
            {
                specs.push(model.model_spec_named(model.as_str()));
            }

            // Claude Code aliases are matched by `as_str()` at runtime as well.
            if model.is_claude_code() {
                specs.push(model.model_spec_named(model.as_str()));
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

    /// Get the concrete execution path for this model.
    pub fn client_kind(&self) -> ClientKind {
        self.descriptor().client_kind
    }

    /// Check whether the provider matches the model.
    pub fn provider_matches(&self, provider: Provider) -> bool {
        provider == self.provider()
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
    /// Accepts only "provider:model" format.
    ///
    /// Returns None if the model string is not recognized.
    pub fn from_canonical_id(canonical_id: &str) -> Option<Self> {
        let normalized = canonical_id.trim().to_lowercase();

        if let Some((provider_str, model_str)) = normalized.split_once(':')
            && let Some(provider) = Provider::from_canonical_str(provider_str)
        {
            return Self::for_provider_and_model(provider, model_str);
        }

        None
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

        catalog::lookup_by_name(normalized)
    }

    /// Resolve a concrete model for a specific provider/model pair.
    pub fn for_provider_and_model(provider: Provider, model: &str) -> Option<Self> {
        let normalized = model.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            return None;
        }

        catalog::lookup_for_provider(provider, &normalized).or_else(|| {
            let parsed = Self::from_api_name(&normalized)?;
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
        self.client_kind() == ClientKind::CodexCli
    }

    /// Check if this model uses the Claude Code CLI
    pub fn is_claude_code(&self) -> bool {
        self.client_kind() == ClientKind::ClaudeCodeCli
    }

    /// Check if this model uses the OpenCode CLI
    pub fn is_opencode_cli(&self) -> bool {
        self.client_kind() == ClientKind::OpenCodeCli
    }

    /// Check if this model uses the Gemini CLI
    pub fn is_gemini_cli(&self) -> bool {
        self.client_kind() == ClientKind::GeminiCli
    }

    /// Check if this model is any CLI-based model (manages its own auth)
    pub fn is_cli_model(&self) -> bool {
        self.client_kind().is_cli()
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

    /// Convert metadata to serializable DTO for runtime clients.
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
