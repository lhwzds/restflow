//! Agent helpers shared across execution paths.

use anyhow::{Result, anyhow};
use restflow_ai::{AnthropicClient, LlmClient, OpenAIClient};
use restflow_core::models::{AgentNode, ApiKeyConfig};
use restflow_core::{AIModel, Provider};
use restflow_core::storage::Storage;
use std::sync::Arc;

/// Create an LLM client for the given agent using configured secrets.
pub async fn create_llm_client_for_agent(
    agent: &AgentNode,
    storage: &Storage,
) -> Result<Arc<dyn LlmClient>> {
    let model = agent.require_model().map_err(|e| anyhow!(e))?;
    let api_key = resolve_api_key(model, agent.api_key_config.as_ref(), storage)?;
    Ok(create_llm_client(model, &api_key))
}

fn resolve_api_key(
    model: AIModel,
    agent_api_key_config: Option<&ApiKeyConfig>,
    storage: &Storage,
) -> Result<String> {
    if let Some(config) = agent_api_key_config {
        match config {
            ApiKeyConfig::Direct(key) => {
                if !key.is_empty() {
                    return Ok(key.clone());
                }
            }
            ApiKeyConfig::Secret(secret_name) => {
                if let Some(secret_value) = storage.secrets.get_secret(secret_name)? {
                    return Ok(secret_value);
                }
                return Err(anyhow!("Secret '{}' not found", secret_name));
            }
        }
    }

    let secret_name = match model.provider() {
        Provider::OpenAI => "OPENAI_API_KEY",
        Provider::Anthropic => "ANTHROPIC_API_KEY",
        Provider::DeepSeek => "DEEPSEEK_API_KEY",
    };

    if let Some(secret_value) = storage.secrets.get_secret(secret_name)? {
        return Ok(secret_value);
    }

    Err(anyhow!(
        "No API key configured for provider {:?}. Please add secret '{}' in Settings.",
        model.provider(),
        secret_name
    ))
}

fn create_llm_client(model: AIModel, api_key: &str) -> Arc<dyn LlmClient> {
    let model_str = model.as_str();
    match model.provider() {
        Provider::OpenAI => Arc::new(OpenAIClient::new(api_key).with_model(model_str)),
        Provider::Anthropic => Arc::new(AnthropicClient::new(api_key).with_model(model_str)),
        Provider::DeepSeek => Arc::new(
            OpenAIClient::new(api_key)
                .with_model(model_str)
                .with_base_url("https://api.deepseek.com/v1"),
        ),
    }
}
