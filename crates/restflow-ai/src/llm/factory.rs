//! LLM client factory for dynamic model creation

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{AiError, Result};
use crate::llm::{
    AnthropicClient, ClaudeCodeClient, CodexClient, GeminiCliClient, LlmClient, OpenAIClient,
    OpenCodeClient,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LlmProvider {
    OpenAI,
    Anthropic,
    DeepSeek,
    Google,
    Groq,
    OpenRouter,
    XAI,
    Qwen,
    Zhipu,
    Moonshot,
    Doubao,
    Yi,
    SiliconFlow,
}

impl LlmProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OpenAI => "openai",
            Self::Anthropic => "anthropic",
            Self::DeepSeek => "deepseek",
            Self::Google => "google",
            Self::Groq => "groq",
            Self::OpenRouter => "openrouter",
            Self::XAI => "xai",
            Self::Qwen => "qwen",
            Self::Zhipu => "zhipu",
            Self::Moonshot => "moonshot",
            Self::Doubao => "doubao",
            Self::Yi => "yi",
            Self::SiliconFlow => "siliconflow",
        }
    }

    pub fn base_url(&self) -> &'static str {
        match self {
            Self::OpenAI => "https://api.openai.com/v1",
            Self::Anthropic => "",
            Self::DeepSeek => "https://api.deepseek.com/v1",
            Self::Google => "https://generativelanguage.googleapis.com/v1beta/openai",
            Self::Groq => "https://api.groq.com/openai/v1",
            Self::OpenRouter => "https://openrouter.ai/api/v1",
            Self::XAI => "https://api.x.ai/v1",
            Self::Qwen => "https://dashscope.aliyuncs.com/compatible-mode/v1",
            Self::Zhipu => "https://open.bigmodel.cn/api/paas/v4",
            Self::Moonshot => "https://api.moonshot.cn/v1",
            Self::Doubao => "https://ark.cn-beijing.volces.com/api/v3",
            Self::Yi => "https://api.lingyiwanwu.com/v1",
            Self::SiliconFlow => "https://api.siliconflow.cn/v1",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelSpec {
    pub name: String,
    pub provider: LlmProvider,
    pub client_model: String,
    pub is_codex_cli: bool,
    pub is_opencode_cli: bool,
    pub is_gemini_cli: bool,
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
            is_opencode_cli: false,
            is_gemini_cli: false,
        }
    }

    pub fn codex(name: impl Into<String>, client_model: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provider: LlmProvider::OpenAI,
            client_model: client_model.into(),
            is_codex_cli: true,
            is_opencode_cli: false,
            is_gemini_cli: false,
        }
    }

    pub fn opencode(name: impl Into<String>, client_model: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provider: LlmProvider::OpenAI,
            client_model: client_model.into(),
            is_codex_cli: false,
            is_opencode_cli: true,
            is_gemini_cli: false,
        }
    }

    pub fn gemini_cli(name: impl Into<String>, client_model: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provider: LlmProvider::Google,
            client_model: client_model.into(),
            is_codex_cli: false,
            is_opencode_cli: false,
            is_gemini_cli: true,
        }
    }
}

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

        if spec.is_opencode_cli {
            let mut client = OpenCodeClient::new().with_model(spec.client_model);
            if let Some(key) = api_key {
                let env_var = detect_env_var(key);
                client = client.with_provider_env(env_var, key.to_string());
            }
            return Ok(Arc::new(client));
        }

        if spec.is_codex_cli {
            return Ok(Arc::new(CodexClient::new().with_model(spec.client_model)));
        }

        if spec.is_gemini_cli {
            let mut client = GeminiCliClient::new().with_model(spec.client_model);
            if let Some(key) = api_key {
                client = client.with_api_key(key.to_string());
            }
            return Ok(Arc::new(client));
        }

        let key = api_key.ok_or_else(|| {
            AiError::Llm(format!("{} API key is required", spec.provider.as_str()))
        })?;

        match spec.provider {
            LlmProvider::Anthropic => {
                if key.starts_with("sk-ant-oat") {
                    let client = ClaudeCodeClient::new(key).with_model(spec.client_model);
                    Ok(Arc::new(client))
                } else {
                    let client = AnthropicClient::new(key).with_model(spec.client_model);
                    Ok(Arc::new(client))
                }
            }
            provider => {
                let client = OpenAIClient::new(key)
                    .with_model(spec.client_model)
                    .with_base_url(provider.base_url());
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
