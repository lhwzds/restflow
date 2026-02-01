//! Agent-related models
//!
//! These models define the configuration structure for AI agents.

use crate::models::AIModel;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// API key or password configuration (direct value or secret reference)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
pub enum ApiKeyConfig {
    /// Direct password/key value
    Direct(String),
    /// Reference to secret name in secret manager
    Secret(String),
}

/// Agent configuration for AI-powered execution
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct AgentNode {
    /// AI model to use for this agent (None = auto-select based on auth profile)
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<AIModel>,
    /// System prompt for the agent
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Temperature setting for model responses
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// API key configuration (direct or from secret)
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_config: Option<ApiKeyConfig>,
    /// List of tool names the agent is allowed to use
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
}

impl AgentNode {
    /// Create a new agent with default settings (no model specified)
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new agent with a specific model
    pub fn with_model(model: AIModel) -> Self {
        Self {
            model: Some(model),
            ..Default::default()
        }
    }

    /// Set the system prompt
    pub fn with_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = Some(prompt.into());
        self
    }

    /// Set the temperature
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Set the API key configuration
    pub fn with_api_key(mut self, config: ApiKeyConfig) -> Self {
        self.api_key_config = Some(config);
        self
    }

    /// Set the allowed tools
    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.tools = Some(tools);
        self
    }

    /// Get the model, returning an error if not specified
    pub fn require_model(&self) -> Result<AIModel, &'static str> {
        self.model.ok_or("Model not specified. Please set a model for this agent.")
    }

    /// Get the model or use a fallback default
    pub fn get_model_or(&self, default: AIModel) -> AIModel {
        self.model.unwrap_or(default)
    }
}
