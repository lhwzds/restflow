//! Concrete [`LlmSwitcher`] implementation wrapping `SwappableLlm` + `LlmClientFactory`.

use std::sync::Arc;

use restflow_traits::ToolError;
use restflow_traits::llm::{ClientKind, LlmProvider, LlmSwitcher, SwapResult};

use super::factory::LlmClientFactory;
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

    fn provider_for_model(&self, model: &str) -> Option<LlmProvider> {
        self.factory.provider_for_model(model)
    }

    fn resolve_api_key(&self, provider: LlmProvider) -> Option<String> {
        self.factory.resolve_api_key(provider)
    }

    fn client_kind_for_model(&self, model: &str) -> Option<ClientKind> {
        self.factory.client_kind_for_model(model)
    }

    fn create_and_swap(
        &self,
        model: &str,
        api_key: Option<&str>,
    ) -> std::result::Result<SwapResult, ToolError> {
        let new_runtime_provider = self.factory.provider_for_model(model).ok_or_else(|| {
            ToolError::Tool(format!("Unknown runtime provider for model '{model}'"))
        })?;
        let client = self
            .factory
            .create_client(model, api_key)
            .map_err(|e| ToolError::Tool(e.to_string()))?;

        let previous = self.llm.swap(client.clone());
        let previous_runtime_provider = self.factory.provider_for_model(previous.model());

        Ok(SwapResult {
            previous_provider: previous.provider().to_string(),
            previous_model: previous.model().to_string(),
            previous_runtime_provider,
            new_provider: client.provider().to_string(),
            new_model: client.model().to_string(),
            new_runtime_provider,
        })
    }
}
