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
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentNode {
    /// AI model to use for this agent
    pub model: AIModel,
    /// System prompt for the agent
    #[ts(optional)]
    pub prompt: Option<String>,
    /// Temperature setting for model responses
    #[ts(optional)]
    pub temperature: Option<f64>,
    /// API key configuration (direct or from secret)
    #[ts(optional)]
    pub api_key_config: Option<ApiKeyConfig>,
    /// List of tool names the agent is allowed to use
    #[ts(optional)]
    pub tools: Option<Vec<String>>,
}

impl AgentNode {
    /// Create a new agent with default settings
    pub fn new(model: AIModel) -> Self {
        Self {
            model,
            prompt: None,
            temperature: None,
            api_key_config: None,
            tools: None,
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
}
