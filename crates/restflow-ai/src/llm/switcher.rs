//! Concrete [`LlmSwitcher`] implementation wrapping `SwappableLlm` + `LlmClientFactory`.

use std::sync::Arc;

use restflow_traits::ModelProvider;
use restflow_traits::ToolError;
use restflow_traits::llm::{LlmSwitcher, SwapResult};

use super::factory::LlmClientFactory;
use super::swappable::SwappableLlm;
use restflow_models::LlmProvider;

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
    let provider = ModelProvider::parse_alias(value)?;
    Some(match provider {
        ModelProvider::OpenAI => LlmProvider::OpenAI,
        ModelProvider::Anthropic => LlmProvider::Anthropic,
        ModelProvider::ClaudeCode => LlmProvider::Anthropic,
        ModelProvider::Codex => LlmProvider::OpenAI,
        ModelProvider::DeepSeek => LlmProvider::DeepSeek,
        ModelProvider::Google => LlmProvider::Google,
        ModelProvider::Groq => LlmProvider::Groq,
        ModelProvider::OpenRouter => LlmProvider::OpenRouter,
        ModelProvider::XAI => LlmProvider::XAI,
        ModelProvider::Qwen => LlmProvider::Qwen,
        ModelProvider::Zai => LlmProvider::Zai,
        ModelProvider::ZaiCodingPlan => LlmProvider::ZaiCodingPlan,
        ModelProvider::Moonshot => LlmProvider::Moonshot,
        ModelProvider::Doubao => LlmProvider::Doubao,
        ModelProvider::Yi => LlmProvider::Yi,
        ModelProvider::SiliconFlow => LlmProvider::SiliconFlow,
        ModelProvider::MiniMax => LlmProvider::MiniMax,
        ModelProvider::MiniMaxCodingPlan => LlmProvider::MiniMaxCodingPlan,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_provider_str;
    use restflow_models::LlmProvider;

    #[test]
    fn parse_provider_aliases_from_shared_model_provider() {
        assert_eq!(parse_provider_str("gpt"), Some(LlmProvider::OpenAI));
        assert_eq!(parse_provider_str("gemini"), Some(LlmProvider::Google));
        assert_eq!(
            parse_provider_str("claude-code"),
            Some(LlmProvider::Anthropic)
        );
        assert_eq!(parse_provider_str("codex"), Some(LlmProvider::OpenAI));
        assert_eq!(
            parse_provider_str("zai-coding"),
            Some(LlmProvider::ZaiCodingPlan)
        );
        assert_eq!(
            parse_provider_str("minimaxcodingplan"),
            Some(LlmProvider::MiniMaxCodingPlan)
        );
    }
}
