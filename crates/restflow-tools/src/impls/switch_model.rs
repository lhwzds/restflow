//! Tool for switching the active LLM model at runtime

use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::{Result, ToolError};
use crate::{Tool, ToolOutput};
use restflow_models::{
    ProviderSelector, parse_model_reference, parse_provider_selector, resolve_available_model_name,
    split_provider_qualified_model,
};
use restflow_traits::LlmSwitcher;

#[derive(Clone)]
pub struct SwitchModelTool {
    switcher: Arc<dyn LlmSwitcher>,
}

impl SwitchModelTool {
    pub fn new(switcher: Arc<dyn LlmSwitcher>) -> Self {
        Self { switcher }
    }

    fn model_matches_provider(&self, model: &str, provider: ProviderSelector) -> bool {
        parse_model_reference(model)
            .map(|model_id| provider.matches_model(model_id))
            .unwrap_or(false)
    }

    fn resolve_target_model(
        &self,
        requested_provider: Option<&str>,
        requested_model: Option<&str>,
    ) -> Result<String> {
        let available = self.switcher.available_models();

        if requested_provider.is_none() || requested_model.is_none() {
            return Err(ToolError::Tool(
                "Missing parameters: both 'provider' and 'model' are required".to_string(),
            ));
        }

        let provider_raw = requested_provider.expect("requested_provider checked above");
        let provider = parse_provider_selector(provider_raw).ok_or_else(|| {
            ToolError::Tool(format!(
                "Unknown provider: {provider_raw}. Use provider names like openai, anthropic, minimax, minimax-coding-plan, zai, zai-coding-plan, claude-code, openai-codex, gemini-cli"
            ))
        })?;

        let model_raw = requested_model.expect("requested_model checked above");
        if let Some(model) = resolve_available_model_name(model_raw, &available) {
            if !self.model_matches_provider(&model, provider) {
                return Err(ToolError::Tool(format!(
                    "Model '{model_raw}' does not belong to provider '{}'",
                    provider.label()
                )));
            }
            return Ok(model);
        }

        let model_candidate = if let Some((inline_provider, inline_model)) =
            split_provider_qualified_model(model_raw)
        {
            if inline_provider != provider {
                return Err(ToolError::Tool(format!(
                    "Model '{model_raw}' does not belong to provider '{}'",
                    provider.label()
                )));
            }
            inline_model.to_string()
        } else {
            model_raw.to_string()
        };

        let model = resolve_available_model_name(&model_candidate, &available).ok_or_else(|| {
            ToolError::Tool(format!(
                "Unknown model: '{model_candidate}'. Use manage_agents tool to list available models, or check the provider's documentation."
            ))
        })?;
        if !self.model_matches_provider(&model, provider) {
            return Err(ToolError::Tool(format!(
                "Model '{model_raw}' does not belong to provider '{}'",
                provider.label()
            )));
        }
        Ok(model)
    }
}

#[async_trait]
impl Tool for SwitchModelTool {
    fn name(&self) -> &str {
        "switch_model"
    }

    fn description(&self) -> &str {
        "Switch the active LLM provider and model for the current agent execution."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "description": "Both 'provider' and 'model' are required.",
            "properties": {
                "provider": {
                    "type": "string",
                    "description": "Provider selector (e.g. openai, anthropic, claude-code, openai-codex, gemini-cli)"
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
        let swap_result = self.switcher.switch_model(&model_name)?;

        let payload = json!({
            "switched": true,
            "request": {
                "provider": requested_provider,
                "model": requested_model
            },
            "from": {
                "provider": swap_result.previous_provider,
                "model": swap_result.previous_model
            },
            "to": {
                "provider": swap_result.new_provider,
                "model": swap_result.new_model
            },
            "reason": reason
        });

        Ok(ToolOutput::success(payload))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_ai::error::AiError;
    use restflow_ai::llm::{
        ClientKind, CompletionRequest, CompletionResponse, FinishReason, LlmClient,
        LlmClientFactory, LlmProvider, StreamResult, SwappableLlm, TokenUsage,
    };
    type AiResult<T> = std::result::Result<T, AiError>;
    use std::collections::HashMap;
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

        async fn complete(&self, _request: CompletionRequest) -> AiResult<CompletionResponse> {
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
        client_kinds: HashMap<String, ClientKind>,
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
                client_kinds: codex_models
                    .into_iter()
                    .map(|model| (normalize(model), ClientKind::CodexCli))
                    .collect(),
                create_calls: Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<(String, Option<String>)> {
            self.create_calls.lock().expect("lock poisoned").clone()
        }
    }

    impl LlmClientFactory for MockFactory {
        fn create_client(
            &self,
            model: &str,
            api_key: Option<&str>,
        ) -> AiResult<Arc<dyn LlmClient>> {
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

        fn client_kind_for_model(&self, model: &str) -> Option<ClientKind> {
            let normalized = model.trim().to_lowercase();
            self.providers.contains_key(&normalized).then(|| {
                self.client_kinds
                    .get(&normalized)
                    .copied()
                    .unwrap_or(ClientKind::Http)
            })
        }
    }

    fn build_tool(factory: Arc<MockFactory>) -> (SwitchModelTool, Arc<SwappableLlm>) {
        use restflow_ai::llm::LlmSwitcherImpl;
        let llm = Arc::new(SwappableLlm::new(Arc::new(MockClient::new(
            "anthropic",
            "claude-haiku-4-5",
        ))));
        let switcher = Arc::new(LlmSwitcherImpl::new(llm.clone(), factory));
        (SwitchModelTool::new(switcher), llm)
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
                "provider": "openai-codex",
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
    async fn execute_supports_claude_code_provider_for_claude_code_models() {
        let factory = Arc::new(MockFactory::new(
            vec!["claude-sonnet-4-5", "claude-code-sonnet"],
            vec![
                ("claude-sonnet-4-5", LlmProvider::Anthropic),
                ("claude-code-sonnet", LlmProvider::Anthropic),
            ],
            vec![(LlmProvider::Anthropic, "anthropic-key")],
            vec![],
        ));
        let (tool, llm) = build_tool(factory.clone());

        let output = tool
            .execute(json!({
                "provider": "claude-code",
                "model": "claude-code-sonnet"
            }))
            .await
            .expect("switch should succeed");

        assert!(output.success);
        assert_eq!(llm.current_model(), "claude-code-sonnet");
        assert_eq!(
            factory.calls(),
            vec![(
                "claude-code-sonnet".to_string(),
                Some("anthropic-key".to_string())
            )]
        );
    }

    #[tokio::test]
    async fn execute_rejects_openai_provider_for_openai_codex_model() {
        let factory = Arc::new(MockFactory::new(
            vec!["gpt-5", "gpt-5.3-codex"],
            vec![
                ("gpt-5", LlmProvider::OpenAI),
                ("gpt-5.3-codex", LlmProvider::OpenAI),
            ],
            vec![(LlmProvider::OpenAI, "openai-key")],
            vec!["gpt-5.3-codex"],
        ));
        let (tool, _) = build_tool(factory);

        let error = tool
            .execute(json!({
                "provider": "openai",
                "model": "gpt-5.3-codex"
            }))
            .await
            .expect_err("switch should fail");

        assert!(
            error
                .to_string()
                .contains("does not belong to provider 'openai'"),
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
            .execute(json!({ "provider": "openai-codex" }))
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
    async fn execute_reports_unknown_model_with_actionable_guidance() {
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
            .execute(json!({
                "provider": "openai-codex",
                "model": "missing-model"
            }))
            .await
            .expect_err("switch should fail for unknown model");
        let message = error.to_string();

        assert!(message.contains("Unknown model: 'missing-model'"));
        assert!(message.contains("Use manage_agents tool to list available models"));
    }

    #[tokio::test]
    async fn execute_reports_missing_api_key_with_manage_secrets_guidance() {
        let factory = Arc::new(MockFactory::new(
            vec!["claude-sonnet-4-5"],
            vec![("claude-sonnet-4-5", LlmProvider::Anthropic)],
            vec![],
            vec![],
        ));
        let (tool, _) = build_tool(factory);

        let error = tool
            .execute(json!({
                "provider": "anthropic",
                "model": "claude-sonnet-4-5"
            }))
            .await
            .expect_err("switch should fail without provider key");
        let message = error.to_string();

        assert!(message.contains("No API key for provider 'anthropic'"));
        assert!(message.contains("Set the key via manage_secrets tool"));
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
                "provider": "openai-codex",
                "model": "openai-codex:gpt-5.3-codex"
            }))
            .await
            .expect("switch should succeed");

        assert!(output.success);
        assert_eq!(llm.current_model(), "gpt-5.3-codex");
        assert_eq!(factory.calls(), vec![("gpt-5.3-codex".to_string(), None)]);
    }

    #[tokio::test]
    async fn execute_supports_shared_catalog_aliases_for_coding_plan_models() {
        let factory = Arc::new(MockFactory::new(
            vec!["minimax-coding-plan-m2-1", "minimax-coding-plan-m2-5"],
            vec![
                ("minimax-coding-plan-m2-1", LlmProvider::MiniMaxCodingPlan),
                ("minimax-coding-plan-m2-5", LlmProvider::MiniMaxCodingPlan),
            ],
            vec![(LlmProvider::MiniMaxCodingPlan, "minimax-key")],
            vec![],
        ));
        let (tool, llm) = build_tool(factory.clone());

        let output = tool
            .execute(json!({
                "provider": "minimax-coding-plan",
                "model": "minimax/coding-plan"
            }))
            .await
            .expect("switch should resolve shared alias");

        assert!(output.success);
        assert_eq!(llm.current_model(), "minimax-coding-plan-m2-5");
        assert_eq!(
            factory.calls(),
            vec![(
                "minimax-coding-plan-m2-5".to_string(),
                Some("minimax-key".to_string())
            )]
        );
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
