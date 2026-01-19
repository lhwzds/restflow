//! Agent node configuration
//!
//! This module defines the AgentNode configuration structure.
//! Agent execution is handled by restflow-ai's AgentExecutor.

use crate::models::AIModel;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Configuration for API key source
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
pub enum ApiKeyConfig {
    /// Direct API key value
    Direct(String),
    /// Reference to secret name in secret manager
    Secret(String),
}

/// Agent configuration node
///
/// This struct holds the configuration for an AI agent.
/// Execution is handled by restflow-ai's AgentExecutor.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentNode {
    /// The AI model to use
    pub model: AIModel,
    /// System prompt for the agent
    pub prompt: Option<String>,
    /// Temperature for generation (0.0-1.0)
    pub temperature: Option<f64>,
    /// API key configuration
    pub api_key_config: Option<ApiKeyConfig>,
    /// Tool names to enable for this agent
    pub tools: Option<Vec<String>>,
}

impl AgentNode {
    /// Create a new agent node with the given configuration
    pub fn new(
        model: AIModel,
        prompt: String,
        temperature: Option<f64>,
        api_key_config: Option<ApiKeyConfig>,
    ) -> Self {
        Self {
            model,
            prompt: Some(prompt),
            temperature,
            api_key_config,
            tools: None,
        }
    }

    /// Create an agent node from a JSON config
    pub fn from_config(config: &serde_json::Value) -> Result<Self> {
        let model = config
            .get("model")
            .ok_or_else(|| anyhow::anyhow!("Model missing in config"))?;
        let model: AIModel = serde_json::from_value(model.clone())?;

        let prompt = config
            .get("prompt")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let temperature = config.get("temperature").and_then(|v| v.as_f64());

        let api_key_config = config
            .get("api_key_config")
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()?;

        let tools = config.get("tools").and_then(|v| v.as_array()).map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        });

        Ok(Self {
            model,
            prompt,
            temperature,
            api_key_config,
            tools,
        })
    }

    /// Set the tools for this agent
    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.tools = Some(tools);
        self
    }
}
