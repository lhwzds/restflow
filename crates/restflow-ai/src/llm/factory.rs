//! LLM client factory for dynamic model creation

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{AiError, Result};
use crate::llm::{AnthropicClient, ClaudeCodeClient, CodexClient, LlmClient, OpenAIClient};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LlmProvider {
    OpenAI,
    Anthropic,
    DeepSeek,
}

impl LlmProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OpenAI => "openai",
            Self::Anthropic => "anthropic",
            Self::DeepSeek => "deepseek",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelSpec {
    pub name: String,
    pub provider: LlmProvider,
    pub client_model: String,
    pub is_codex_cli: bool,
}

impl ModelSpec {
    pub fn new(
        name: impl Into<String>,
        provider: LlmProvider,
        client_model: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            provider,
            client_model: client_model.into(),
            is_codex_cli: false,
        }
    }

    pub fn codex(name: impl Into<String>, client_model: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provider: LlmProvider::OpenAI,
            client_model: client_model.into(),
            is_codex_cli: true,
        }
    }
}

pub trait LlmClientFactory: Send + Sync {
    fn create_client(&self, model: &str, api_key: Option<&str>) -> Result<Arc<dyn LlmClient>>;
    fn available_models(&self) -> Vec<String>;
    fn resolve_api_key(&self, provider: LlmProvider) -> Option<String>;
    fn provider_for_model(&self, model: &str) -> Option<LlmProvider>;
    fn is_codex_cli_model(&self, model: &str) -> bool;
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

        match spec.provider {
            LlmProvider::OpenAI => {
                if spec.is_codex_cli {
                    Ok(Arc::new(CodexClient::new().with_model(spec.client_model)))
                } else {
                    let key = api_key
                        .ok_or_else(|| AiError::Llm("OpenAI API key is required".to_string()))?;
                    let client = OpenAIClient::new(key).with_model(spec.client_model);
                    Ok(Arc::new(client))
                }
            }
            LlmProvider::Anthropic => {
                let key = api_key
                    .ok_or_else(|| AiError::Llm("Anthropic API key is required".to_string()))?;
                if key.starts_with("sk-ant-oat") {
                    let client = ClaudeCodeClient::new(key).with_model(spec.client_model);
                    Ok(Arc::new(client))
                } else {
                    let client = AnthropicClient::new(key).with_model(spec.client_model);
                    Ok(Arc::new(client))
                }
            }
            LlmProvider::DeepSeek => {
                let key = api_key
                    .ok_or_else(|| AiError::Llm("DeepSeek API key is required".to_string()))?;
                let client = OpenAIClient::new(key)
                    .with_model(spec.client_model)
                    .with_base_url("https://api.deepseek.com/v1");
                Ok(Arc::new(client))
            }
        }
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
}

fn normalize_model_name(model: &str) -> String {
    model.trim().to_lowercase()
}
