//! LLM switching abstractions.
//!
//! Defines the [`LlmSwitcher`] trait for runtime model switching without
//! coupling consumers to concrete LLM client implementations.

use crate::error::ToolError;

/// Result of a successful model swap.
#[derive(Debug, Clone)]
pub struct SwapResult {
    /// Previous provider name.
    pub previous_provider: String,
    /// Previous model name.
    pub previous_model: String,
    /// New provider name.
    pub new_provider: String,
    /// New model name.
    pub new_model: String,
}

/// Runtime LLM model switching.
///
/// Abstracts `SwappableLlm` + `LlmClientFactory` so that tool implementations
/// can switch models without depending on the concrete AI framework.
pub trait LlmSwitcher: Send + Sync {
    /// Current model name.
    fn current_model(&self) -> String;

    /// Current provider name.
    fn current_provider(&self) -> String;

    /// List all available model names.
    fn available_models(&self) -> Vec<String>;

    /// Return the provider name for a given model, if known.
    fn provider_for_model(&self, model: &str) -> Option<String>;

    /// Resolve the API key for a provider name.
    fn resolve_api_key(&self, provider: &str) -> Option<String>;

    /// Whether the model is a Codex CLI model.
    fn is_codex_cli_model(&self, model: &str) -> bool;

    /// Whether the model is an OpenCode CLI model.
    fn is_opencode_cli_model(&self, model: &str) -> bool;

    /// Whether the model is a Gemini CLI model.
    fn is_gemini_cli_model(&self, model: &str) -> bool;

    /// Create a new LLM client for the given model and swap the active client.
    ///
    /// Returns the previous and new provider/model information.
    fn create_and_swap(
        &self,
        model: &str,
        api_key: Option<&str>,
    ) -> std::result::Result<SwapResult, ToolError>;
}
