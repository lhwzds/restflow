//! LLM client factory for dynamic model creation

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{AiError, Result};
use crate::llm::retry::RetryingLlmClient;
use crate::llm::{
    AnthropicClient, ClaudeCodeClient, CodexClient, GeminiCliClient, LlmClient, OpenAIClient,
    OpenCodeClient,
};
use restflow_models::{LlmProvider, ModelSpec};

pub trait LlmClientFactory: Send + Sync {
    fn create_client(&self, model: &str, api_key: Option<&str>) -> Result<Arc<dyn LlmClient>>;
    fn available_models(&self) -> Vec<String>;
    fn resolve_api_key(&self, provider: LlmProvider) -> Option<String>;
    fn provider_for_model(&self, model: &str) -> Option<LlmProvider>;
    fn is_codex_cli_model(&self, model: &str) -> bool;
    fn is_opencode_cli_model(&self, model: &str) -> bool;
    fn is_gemini_cli_model(&self, model: &str) -> bool;
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

        let client: Arc<dyn LlmClient> = if spec.is_opencode_cli {
            let mut c = OpenCodeClient::new().with_model(spec.client_model);
            if let Some(key) = api_key {
                let env_var = detect_env_var(key);
                c = c.with_provider_env(env_var, key.to_string());
            }
            Arc::new(c)
        } else if spec.is_codex_cli {
            Arc::new(CodexClient::new().with_model(spec.client_model))
        } else if spec.is_gemini_cli {
            let mut c = GeminiCliClient::new().with_model(spec.client_model);
            if let Some(key) = api_key {
                c = c.with_api_key(key.to_string());
            }
            Arc::new(c)
        } else {
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

    fn is_codex_cli_model(&self, model: &str) -> bool {
        let key = normalize_model_name(model);
        self.models
            .get(&key)
            .map(|spec| spec.is_codex_cli)
            .unwrap_or(false)
    }

    fn is_opencode_cli_model(&self, model: &str) -> bool {
        let key = normalize_model_name(model);
        self.models
            .get(&key)
            .map(|spec| spec.is_opencode_cli)
            .unwrap_or(false)
    }

    fn is_gemini_cli_model(&self, model: &str) -> bool {
        let key = normalize_model_name(model);
        self.models
            .get(&key)
            .map(|spec| spec.is_gemini_cli)
            .unwrap_or(false)
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
    use super::LlmProvider;

    #[test]
    fn zai_uses_api_z_ai_endpoint() {
        assert_eq!(LlmProvider::Zai.base_url(), "https://api.z.ai/api/paas/v4");
    }
}
