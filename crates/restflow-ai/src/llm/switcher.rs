//! Concrete [`LlmSwitcher`] implementation wrapping `SwappableLlm` + `LlmClientFactory`.

use std::sync::Arc;

use restflow_traits::llm::{LlmSwitcher, SwapResult};
use restflow_traits::ToolError;

use super::factory::{LlmClientFactory, LlmProvider};
use super::swappable::SwappableLlm;

/// Concrete implementation of [`LlmSwitcher`].
pub struct LlmSwitcherImpl {
    llm: Arc<SwappableLlm>,
    factory: Arc<dyn LlmClientFactory>,
}

impl LlmSwitcherImpl {
    pub fn new(llm: Arc<SwappableLlm>, factory: Arc<dyn LlmClientFactory>) -> Self {
        Self { llm, factory }
    }
}

impl LlmSwitcher for LlmSwitcherImpl {
    fn current_model(&self) -> String {
        self.llm.current_model()
    }

    fn current_provider(&self) -> String {
        self.llm.current_provider()
    }

    fn available_models(&self) -> Vec<String> {
        self.factory.available_models()
    }

    fn provider_for_model(&self, model: &str) -> Option<String> {
        self.factory
            .provider_for_model(model)
            .map(|p| p.as_str().to_string())
    }

    fn resolve_api_key(&self, provider: &str) -> Option<String> {
        let llm_provider = parse_provider_str(provider)?;
        self.factory.resolve_api_key(llm_provider)
    }

    fn is_codex_cli_model(&self, model: &str) -> bool {
        self.factory.is_codex_cli_model(model)
    }

    fn is_opencode_cli_model(&self, model: &str) -> bool {
        self.factory.is_opencode_cli_model(model)
    }

    fn is_gemini_cli_model(&self, model: &str) -> bool {
        self.factory.is_gemini_cli_model(model)
    }

    fn create_and_swap(
        &self,
        model: &str,
        api_key: Option<&str>,
    ) -> std::result::Result<SwapResult, ToolError> {
        let client = self
            .factory
            .create_client(model, api_key)
            .map_err(|e| ToolError::Tool(e.to_string()))?;

        let previous = self.llm.swap(client.clone());

        Ok(SwapResult {
            previous_provider: previous.provider().to_string(),
            previous_model: previous.model().to_string(),
            new_provider: client.provider().to_string(),
            new_model: client.model().to_string(),
        })
    }
}

/// Parse a provider string into `LlmProvider`.
fn parse_provider_str(value: &str) -> Option<LlmProvider> {
    let normalized: String = value
        .trim()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_lowercase();

    match normalized.as_str() {
        "openai" | "gpt" => Some(LlmProvider::OpenAI),
        "anthropic" => Some(LlmProvider::Anthropic),
        "deepseek" => Some(LlmProvider::DeepSeek),
        "google" | "gemini" => Some(LlmProvider::Google),
        "groq" => Some(LlmProvider::Groq),
        "openrouter" => Some(LlmProvider::OpenRouter),
        "xai" => Some(LlmProvider::XAI),
        "qwen" => Some(LlmProvider::Qwen),
        "zai" => Some(LlmProvider::Zai),
        "zaicodingplan" | "zaicoding" => Some(LlmProvider::ZaiCodingPlan),
        "moonshot" => Some(LlmProvider::Moonshot),
        "doubao" => Some(LlmProvider::Doubao),
        "yi" => Some(LlmProvider::Yi),
        "siliconflow" => Some(LlmProvider::SiliconFlow),
        "minimax" => Some(LlmProvider::MiniMax),
        "minimaxcodingplan" | "minimaxcoding" => Some(LlmProvider::MiniMaxCodingPlan),
        _ => None,
    }
}
