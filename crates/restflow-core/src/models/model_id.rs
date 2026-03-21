use restflow_models::{ClientKind, ModelSpec};
use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;
use ts_rs::TS;

mod constants;

use super::{
    catalog,
    model_ref::{ModelMetadata, ModelMetadataDTO},
    provider::Provider,
};

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
        self.descriptor().client_kind == ClientKind::CodexCli
    }

    /// Check if this model uses the Claude Code CLI
    pub fn is_claude_code(&self) -> bool {
        self.descriptor().client_kind == ClientKind::ClaudeCodeCli
    }

    /// Check if this model uses the OpenCode CLI
    pub fn is_opencode_cli(&self) -> bool {
        self.descriptor().client_kind == ClientKind::OpenCodeCli
    }

    /// Check if this model uses the Gemini CLI
    pub fn is_gemini_cli(&self) -> bool {
        self.descriptor().client_kind == ClientKind::GeminiCli
    }

    /// Check if this model is any CLI-based model (manages its own auth)
    pub fn is_cli_model(&self) -> bool {
        self.descriptor().client_kind.is_cli()
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
mod tests;
