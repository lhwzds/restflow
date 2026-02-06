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

    fn resolve_provider(&self, model: &str) -> Result<LlmProvider> {
        self.factory
            .provider_for_model(model)
            .ok_or_else(|| AiError::Tool(format!("Unknown model: {model}")))
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
            "properties": {
                "model": {
                    "type": "string",
                    "description": "Model name to switch to"
                },
                "reason": {
                    "type": "string",
                    "description": "Optional reason for switching models"
                }
            },
            "required": ["model"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let requested = input
            .get("model")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AiError::Tool("Missing 'model' parameter".to_string()))?;
        let reason = input
            .get("reason")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string());

        let normalized = Self::normalize_model(requested);
        let available = self.factory.available_models();
        let model_name = available
            .iter()
            .find(|name| Self::normalize_model(name) == normalized)
            .cloned()
            .ok_or_else(|| AiError::Tool(format!("Unknown model: {requested}")))?;

        let provider = self.resolve_provider(&model_name)?;
        let api_key = if self.factory.is_codex_cli_model(&model_name) {
            None
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
