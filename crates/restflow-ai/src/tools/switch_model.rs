//! Tool for switching the active LLM model at runtime

use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::error::{AiError, Result};
use crate::llm::{LlmClientFactory, LlmProvider, SwappableLlm};
use crate::tools::{Tool, ToolOutput};

#[derive(Clone)]
pub struct SwitchModelTool {
    llm: Arc<SwappableLlm>,
    factory: Arc<dyn LlmClientFactory>,
}

impl SwitchModelTool {
    pub fn new(llm: Arc<SwappableLlm>, factory: Arc<dyn LlmClientFactory>) -> Self {
        Self { llm, factory }
    }

    fn normalize_model(model: &str) -> String {
        model.trim().to_lowercase()
    }

    fn normalize_provider(value: &str) -> String {
        value
            .trim()
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .collect::<String>()
            .to_lowercase()
    }

    fn parse_provider(value: &str) -> Option<ProviderSelector> {
        let normalized = Self::normalize_provider(value);
        match normalized.as_str() {
            "openai" | "gpt" => Some(ProviderSelector::Api(LlmProvider::OpenAI)),
            "anthropic" | "claude" => Some(ProviderSelector::Api(LlmProvider::Anthropic)),
            "deepseek" => Some(ProviderSelector::Api(LlmProvider::DeepSeek)),
            "google" | "gemini" => Some(ProviderSelector::Api(LlmProvider::Google)),
            "groq" => Some(ProviderSelector::Api(LlmProvider::Groq)),
            "openrouter" => Some(ProviderSelector::Api(LlmProvider::OpenRouter)),
            "xai" => Some(ProviderSelector::Api(LlmProvider::XAI)),
            "qwen" => Some(ProviderSelector::Api(LlmProvider::Qwen)),
            "zhipu" => Some(ProviderSelector::Api(LlmProvider::Zhipu)),
            "moonshot" => Some(ProviderSelector::Api(LlmProvider::Moonshot)),
            "doubao" => Some(ProviderSelector::Api(LlmProvider::Doubao)),
            "yi" => Some(ProviderSelector::Api(LlmProvider::Yi)),
            "siliconflow" => Some(ProviderSelector::Api(LlmProvider::SiliconFlow)),
            "codex" | "codexcli" | "openaicodex" => Some(ProviderSelector::CodexCli),
            "opencode" | "opencodecli" => Some(ProviderSelector::OpenCodeCli),
            "geminicli" => Some(ProviderSelector::GeminiCli),
            _ => None,
        }
    }

    fn split_provider_qualified_model(value: &str) -> Option<(ProviderSelector, String)> {
        for separator in [':', '/'] {
            let Some((provider_raw, model_raw)) = value.split_once(separator) else {
                continue;
            };
            if model_raw.trim().is_empty() {
                continue;
            }
            if let Some(provider) = Self::parse_provider(provider_raw) {
                return Some((provider, model_raw.trim().to_string()));
            }
        }
        None
    }

    fn find_model_by_name<'a>(&self, available: &'a [String], requested: &str) -> Option<&'a str> {
        let normalized = Self::normalize_model(requested);
        available
            .iter()
            .find(|name| Self::normalize_model(name) == normalized)
            .map(|name| name.as_str())
    }

    fn model_matches_provider(&self, model: &str, provider: ProviderSelector) -> bool {
        match provider {
            ProviderSelector::Api(value) => self.factory.provider_for_model(model) == Some(value),
            ProviderSelector::CodexCli => self.factory.is_codex_cli_model(model),
            ProviderSelector::OpenCodeCli => self.factory.is_opencode_cli_model(model),
            ProviderSelector::GeminiCli => self.factory.is_gemini_cli_model(model),
        }
    }

    fn resolve_target_model(
        &self,
        requested_provider: Option<&str>,
        requested_model: Option<&str>,
    ) -> Result<String> {
        let available = self.factory.available_models();

        if requested_provider.is_none() || requested_model.is_none() {
            return Err(AiError::Tool(
                "Missing parameters: both 'provider' and 'model' are required".to_string(),
            ));
        }

        let provider_raw = requested_provider.expect("requested_provider checked above");
        let provider = Self::parse_provider(provider_raw).ok_or_else(|| {
            AiError::Tool(format!(
                "Unknown provider: {provider_raw}. Use provider names like openai, anthropic, codex-cli, opencode-cli, gemini-cli"
            ))
        })?;

        let model_raw = requested_model.expect("requested_model checked above");
        let model_candidate = if let Some((inline_provider, inline_model)) =
            Self::split_provider_qualified_model(model_raw)
        {
            if inline_provider != provider {
                return Err(AiError::Tool(format!(
                    "Model '{model_raw}' does not belong to provider '{}'",
                    provider.label()
                )));
            }
            inline_model
        } else {
            model_raw.to_string()
        };

        let model = self
            .find_model_by_name(&available, &model_candidate)
            .ok_or_else(|| AiError::Tool(format!("Unknown model: {model_candidate}")))?;
        if !self.model_matches_provider(model, provider) {
            return Err(AiError::Tool(format!(
                "Model '{model_raw}' does not belong to provider '{}'",
                provider.label()
            )));
        }
        Ok(model.to_string())
    }

    fn resolve_provider(&self, model: &str) -> Result<LlmProvider> {
        self.factory
            .provider_for_model(model)
            .ok_or_else(|| AiError::Tool(format!("Unknown model: {model}")))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProviderSelector {
    Api(LlmProvider),
    CodexCli,
    OpenCodeCli,
    GeminiCli,
}

impl ProviderSelector {
    fn label(self) -> &'static str {
        match self {
            Self::Api(provider) => provider.as_str(),
            Self::CodexCli => "codex-cli",
            Self::OpenCodeCli => "opencode-cli",
            Self::GeminiCli => "gemini-cli",
        }
    }
}

#[async_trait]
impl Tool for SwitchModelTool {
    fn name(&self) -> &str {
        "switch_model"
    }

    fn description(&self) -> &str {
        "Switch the agent to a different LLM model during execution"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "description": "Both 'provider' and 'model' are required.",
            "properties": {
                "provider": {
                    "type": "string",
                    "description": "Provider selector (e.g. openai, anthropic, codex-cli, opencode-cli, gemini-cli)"
                },
                "model": {
                    "type": "string",
                    "description": "Model name to switch to. Supports `provider:model` format for compatibility."
                },
                "reason": {
                    "type": "string",
                    "description": "Optional reason for switching models"
                }
            },
            "required": ["provider", "model"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let requested_model = input
            .get("model")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let requested_provider = input
            .get("provider")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let reason = input
            .get("reason")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string());

        let model_name = self.resolve_target_model(requested_provider, requested_model)?;

        let provider = self.resolve_provider(&model_name)?;
        let is_cli = self.factory.is_codex_cli_model(&model_name)
            || self.factory.is_opencode_cli_model(&model_name)
            || self.factory.is_gemini_cli_model(&model_name);
        let api_key = if is_cli {
            self.factory.resolve_api_key(provider)
        } else {
            Some(self.factory.resolve_api_key(provider).ok_or_else(|| {
                AiError::Tool(format!(
                    "No API key available for provider {}",
                    provider.as_str()
                ))
            })?)
        };

        let client = self
            .factory
            .create_client(&model_name, api_key.as_deref())?;
        let previous = self.llm.swap(client.clone());

        let payload = json!({
            "switched": true,
            "request": {
                "provider": requested_provider,
                "model": requested_model
            },
            "from": {
                "provider": previous.provider(),
                "model": previous.model()
            },
            "to": {
                "provider": client.provider(),
                "model": client.model()
            },
            "reason": reason
        });

        Ok(ToolOutput::success(payload))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{
        CompletionRequest, CompletionResponse, FinishReason, LlmClient, StreamResult, TokenUsage,
    };
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;

    struct MockClient {
        provider: String,
        model: String,
    }

    impl MockClient {
        fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
            Self {
                provider: provider.into(),
                model: model.into(),
            }
        }
    }

    #[async_trait]
    impl LlmClient for MockClient {
        fn provider(&self) -> &str {
            &self.provider
        }

        fn model(&self) -> &str {
            &self.model
        }

        async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
            Ok(CompletionResponse {
                content: Some(String::new()),
                tool_calls: vec![],
                finish_reason: FinishReason::Stop,
                usage: Some(TokenUsage::default()),
            })
        }

        fn complete_stream(&self, _request: CompletionRequest) -> StreamResult {
            unimplemented!("streaming is not used in switch_model tests")
        }
    }

    struct MockFactory {
        available: Vec<String>,
        providers: HashMap<String, LlmProvider>,
        api_keys: HashMap<LlmProvider, String>,
        codex_models: HashSet<String>,
        opencode_models: HashSet<String>,
        gemini_models: HashSet<String>,
        create_calls: Mutex<Vec<(String, Option<String>)>>,
    }

    impl MockFactory {
        fn new(
            available: Vec<&str>,
            providers: Vec<(&str, LlmProvider)>,
            api_keys: Vec<(LlmProvider, &str)>,
            codex_models: Vec<&str>,
        ) -> Self {
            let normalize = |value: &str| value.trim().to_lowercase();
            Self {
                available: available.into_iter().map(str::to_string).collect(),
                providers: providers
                    .into_iter()
                    .map(|(model, provider)| (normalize(model), provider))
                    .collect(),
                api_keys: api_keys
                    .into_iter()
                    .map(|(provider, key)| (provider, key.to_string()))
                    .collect(),
                codex_models: codex_models.into_iter().map(normalize).collect(),
                opencode_models: HashSet::new(),
                gemini_models: HashSet::new(),
                create_calls: Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<(String, Option<String>)> {
            self.create_calls.lock().expect("lock poisoned").clone()
        }
    }

    impl LlmClientFactory for MockFactory {
        fn create_client(&self, model: &str, api_key: Option<&str>) -> Result<Arc<dyn LlmClient>> {
            self.create_calls
                .lock()
                .expect("lock poisoned")
                .push((model.to_string(), api_key.map(ToString::to_string)));
            let provider = self
                .provider_for_model(model)
                .ok_or_else(|| AiError::Llm(format!("no provider found for model {model}")))?;
            Ok(Arc::new(MockClient::new(provider.as_str(), model)))
        }

        fn available_models(&self) -> Vec<String> {
            self.available.clone()
        }

        fn resolve_api_key(&self, provider: LlmProvider) -> Option<String> {
            self.api_keys.get(&provider).cloned()
        }

        fn provider_for_model(&self, model: &str) -> Option<LlmProvider> {
            self.providers.get(&model.trim().to_lowercase()).copied()
        }

        fn is_codex_cli_model(&self, model: &str) -> bool {
            self.codex_models.contains(&model.trim().to_lowercase())
        }

        fn is_opencode_cli_model(&self, model: &str) -> bool {
            self.opencode_models.contains(&model.trim().to_lowercase())
        }

        fn is_gemini_cli_model(&self, model: &str) -> bool {
            self.gemini_models.contains(&model.trim().to_lowercase())
        }
    }

    fn build_tool(factory: Arc<MockFactory>) -> (SwitchModelTool, Arc<SwappableLlm>) {
        let llm = Arc::new(SwappableLlm::new(Arc::new(MockClient::new(
            "anthropic",
            "claude-haiku-4-5",
        ))));
        (SwitchModelTool::new(llm.clone(), factory), llm)
    }

    #[tokio::test]
    async fn execute_requires_provider_and_model() {
        let factory = Arc::new(MockFactory::new(
            vec!["claude-sonnet-4-5", "gpt-5.3-codex"],
            vec![
                ("claude-sonnet-4-5", LlmProvider::Anthropic),
                ("gpt-5.3-codex", LlmProvider::OpenAI),
            ],
            vec![(LlmProvider::Anthropic, "anthropic-key")],
            vec!["gpt-5.3-codex"],
        ));
        let (tool, _) = build_tool(factory);

        let error = tool
            .execute(json!({ "model": "CLAUDE-SONNET-4-5" }))
            .await
            .expect_err("switch should fail without provider");

        assert!(
            error
                .to_string()
                .contains("both 'provider' and 'model' are required"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn execute_supports_provider_and_model_for_codex_cli() {
        let factory = Arc::new(MockFactory::new(
            vec!["claude-sonnet-4-5", "gpt-5.3-codex"],
            vec![
                ("claude-sonnet-4-5", LlmProvider::Anthropic),
                ("gpt-5.3-codex", LlmProvider::OpenAI),
            ],
            vec![],
            vec!["gpt-5.3-codex"],
        ));
        let (tool, llm) = build_tool(factory.clone());

        let output = tool
            .execute(json!({
                "provider": "codex-cli",
                "model": "gpt-5.3-codex"
            }))
            .await
            .expect("switch should succeed");

        assert!(output.success);
        assert_eq!(llm.current_model(), "gpt-5.3-codex");
        assert_eq!(factory.calls(), vec![("gpt-5.3-codex".to_string(), None)]);
    }

    #[tokio::test]
    async fn execute_rejects_provider_model_mismatch() {
        let factory = Arc::new(MockFactory::new(
            vec!["claude-sonnet-4-5", "gpt-5.3-codex"],
            vec![
                ("claude-sonnet-4-5", LlmProvider::Anthropic),
                ("gpt-5.3-codex", LlmProvider::OpenAI),
            ],
            vec![(LlmProvider::Anthropic, "anthropic-key")],
            vec!["gpt-5.3-codex"],
        ));
        let (tool, _) = build_tool(factory);

        let error = tool
            .execute(json!({
                "provider": "anthropic",
                "model": "gpt-5.3-codex"
            }))
            .await
            .expect_err("switch should fail");

        assert!(
            error
                .to_string()
                .contains("does not belong to provider 'anthropic'"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn execute_rejects_missing_model() {
        let factory = Arc::new(MockFactory::new(
            vec!["gpt-5.3-codex", "claude-sonnet-4-5"],
            vec![
                ("claude-sonnet-4-5", LlmProvider::Anthropic),
                ("gpt-5.3-codex", LlmProvider::OpenAI),
            ],
            vec![],
            vec!["gpt-5.3-codex"],
        ));
        let (tool, _) = build_tool(factory);

        let error = tool
            .execute(json!({ "provider": "codex-cli" }))
            .await
            .expect_err("switch should fail without model");

        assert!(
            error
                .to_string()
                .contains("both 'provider' and 'model' are required"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn execute_supports_provider_qualified_model_when_provider_matches() {
        let factory = Arc::new(MockFactory::new(
            vec!["gpt-5.3-codex", "claude-sonnet-4-5"],
            vec![
                ("claude-sonnet-4-5", LlmProvider::Anthropic),
                ("gpt-5.3-codex", LlmProvider::OpenAI),
            ],
            vec![],
            vec!["gpt-5.3-codex"],
        ));
        let (tool, llm) = build_tool(factory.clone());

        let output = tool
            .execute(json!({
                "provider": "codex-cli",
                "model": "codex-cli:gpt-5.3-codex"
            }))
            .await
            .expect("switch should succeed");

        assert!(output.success);
        assert_eq!(llm.current_model(), "gpt-5.3-codex");
        assert_eq!(factory.calls(), vec![("gpt-5.3-codex".to_string(), None)]);
    }

    #[test]
    fn schema_is_claude_compatible() {
        let factory = Arc::new(MockFactory::new(
            vec!["claude-sonnet-4-5"],
            vec![("claude-sonnet-4-5", LlmProvider::Anthropic)],
            vec![(LlmProvider::Anthropic, "anthropic-key")],
            vec![],
        ));
        let (tool, _) = build_tool(factory);
        let schema = tool.parameters_schema();

        assert!(schema.get("anyOf").is_none());
        assert!(schema.get("oneOf").is_none());
        assert!(schema.get("allOf").is_none());
        assert_eq!(
            schema["required"],
            json!(["provider", "model"]),
            "provider and model should both be required"
        );
    }
}
