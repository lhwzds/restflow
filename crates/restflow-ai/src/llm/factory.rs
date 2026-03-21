//! LLM client factory for dynamic model creation

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{AiError, Result};
use crate::llm::retry::RetryingLlmClient;
use crate::llm::{
    AnthropicClient, ClaudeCodeClient, CodexClient, GeminiCliClient, LlmClient, OpenAIClient,
    OpenCodeClient,
};
use restflow_models::{ClientKind, LlmProvider, ModelSpec};

pub trait LlmClientFactory: Send + Sync {
    fn create_client(&self, model: &str, api_key: Option<&str>) -> Result<Arc<dyn LlmClient>>;
    fn available_models(&self) -> Vec<String>;
    fn resolve_api_key(&self, provider: LlmProvider) -> Option<String>;
    fn provider_for_model(&self, model: &str) -> Option<LlmProvider>;
    fn client_kind_for_model(&self, model: &str) -> Option<ClientKind>;
}

pub struct DefaultLlmClientFactory {
    api_keys: HashMap<LlmProvider, String>,
    models: HashMap<String, ModelSpec>,
}

impl DefaultLlmClientFactory {
    pub fn new(api_keys: HashMap<LlmProvider, String>, models: Vec<ModelSpec>) -> Self {
        let mut map = HashMap::new();
        for spec in models {
            map.insert(normalize_model_name(&spec.name), spec);
        }
        Self {
            api_keys,
            models: map,
        }
    }

    fn model_spec(&self, model: &str) -> Result<ModelSpec> {
        let key = normalize_model_name(model);
        self.models
            .get(&key)
            .cloned()
            .ok_or_else(|| AiError::Llm(format!("Unknown model '{model}'")))
    }
}

impl LlmClientFactory for DefaultLlmClientFactory {
    fn create_client(&self, model: &str, api_key: Option<&str>) -> Result<Arc<dyn LlmClient>> {
        let spec = self.model_spec(model)?;

        let client: Arc<dyn LlmClient> = match spec.client_kind {
            ClientKind::OpenCodeCli => {
                let mut c = OpenCodeClient::new().with_model(spec.client_model);
                if let Some(key) = api_key {
                    let env_var = detect_env_var(key);
                    c = c.with_provider_env(env_var, key.to_string());
                }
                Arc::new(c)
            }
            ClientKind::CodexCli => Arc::new(CodexClient::new().with_model(spec.client_model)),
            ClientKind::GeminiCli => {
                let mut c = GeminiCliClient::new().with_model(spec.client_model);
                if let Some(key) = api_key {
                    c = c.with_api_key(key.to_string());
                }
                Arc::new(c)
            }
            ClientKind::Http => {
                let key = api_key.ok_or_else(|| {
                    AiError::Llm(format!("{} API key is required", spec.provider.as_str()))
                })?;

                match spec.provider {
                    LlmProvider::Anthropic => {
                        if key.starts_with("sk-ant-oat") {
                            Arc::new(ClaudeCodeClient::new(key).with_model(spec.client_model))
                        } else {
                            Arc::new(AnthropicClient::new(key)?.with_model(spec.client_model))
                        }
                    }
                    LlmProvider::MiniMax | LlmProvider::MiniMaxCodingPlan => Arc::new(
                        AnthropicClient::new(key)?
                            .with_model(spec.client_model)
                            .with_base_url("https://api.minimax.io/anthropic"),
                    ),
                    provider => {
                        let base_url = spec.base_url.as_deref().unwrap_or(provider.base_url());
                        Arc::new(
                            OpenAIClient::new(key)?
                                .with_model(spec.client_model)
                                .with_base_url(base_url),
                        )
                    }
                }
            }
        };

        Ok(Arc::new(RetryingLlmClient::with_default_config(client)))
    }

    fn available_models(&self) -> Vec<String> {
        let mut models: Vec<String> = self.models.values().map(|spec| spec.name.clone()).collect();
        models.sort();
        models
    }

    fn resolve_api_key(&self, provider: LlmProvider) -> Option<String> {
        self.api_keys.get(&provider).cloned()
    }

    fn provider_for_model(&self, model: &str) -> Option<LlmProvider> {
        let key = normalize_model_name(model);
        self.models.get(&key).map(|spec| spec.provider)
    }

    fn client_kind_for_model(&self, model: &str) -> Option<ClientKind> {
        let key = normalize_model_name(model);
        self.models.get(&key).map(|spec| spec.client_kind)
    }
}

fn normalize_model_name(model: &str) -> String {
    model.trim().to_lowercase()
}

fn detect_env_var(api_key: &str) -> &'static str {
    let normalized = api_key.trim();
    if normalized.starts_with("sk-ant-") {
        "ANTHROPIC_API_KEY"
    } else if normalized.starts_with("ghp_") || normalized.starts_with("gho_") {
        "GITHUB_TOKEN"
    } else if normalized.starts_with("xai-") {
        "XAI_API_KEY"
    } else if normalized.starts_with("sk-or-") {
        "OPENROUTER_API_KEY"
    } else if normalized.starts_with("gsk_") {
        "GROQ_API_KEY"
    } else if normalized.starts_with("AIza") {
        "GEMINI_API_KEY"
    } else {
        "OPENAI_API_KEY"
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{DefaultLlmClientFactory, LlmClientFactory, LlmProvider};
    use restflow_models::{ClientKind, ModelSpec};

    #[test]
    fn zai_uses_api_z_ai_endpoint() {
        assert_eq!(LlmProvider::Zai.base_url(), "https://api.z.ai/api/paas/v4");
    }

    #[test]
    fn factory_reports_client_kind_for_known_models() {
        let factory = DefaultLlmClientFactory::new(
            HashMap::new(),
            vec![
                ModelSpec::new("gpt-5", LlmProvider::OpenAI, "gpt-5"),
                ModelSpec::codex("gpt-5.3-codex", "gpt-5.3-codex"),
            ],
        );

        assert_eq!(
            factory.client_kind_for_model("gpt-5"),
            Some(ClientKind::Http)
        );
        assert_eq!(
            factory.client_kind_for_model("gpt-5.3-codex"),
            Some(ClientKind::CodexCli)
        );
        assert_eq!(factory.client_kind_for_model("missing"), None);
    }
}
