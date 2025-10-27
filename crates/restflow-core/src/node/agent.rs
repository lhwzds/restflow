use crate::tools::{AddTool, GetTimeTool};
use anyhow::Result;
use rig::{
    client::CompletionClient,
    completion::Prompt,
    providers::{anthropic, deepseek, openai},
};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use ts_rs::TS;

/// Macro to build agent with tools
/// In rig-core 0.22.0, calling .tool() changes builder type from AgentBuilder to AgentBuilderSimple
macro_rules! build_with_tools {
    ($self:expr, $builder:expr, $input:expr) => {{
        let agent = match &$self.tools {
            Some(tool_names) if !tool_names.is_empty() => {
                debug!(tools = ?tool_names, "Configuring agent tools");

                let has_add = tool_names.iter().any(|t| t == "add");
                let has_time = tool_names.iter().any(|t| t == "get_current_time");

                // Log unknown tools
                for name in tool_names {
                    if name != "add" && name != "get_current_time" {
                        warn!(tool = %name, "Unknown tool specified");
                    }
                }

                match (has_add, has_time) {
                    (true, true) => {
                        debug!("Adding tools: add, get_current_time");
                        $builder.tool(AddTool).tool(GetTimeTool).build()
                    }
                    (true, false) => {
                        debug!("Adding tool: add");
                        $builder.tool(AddTool).build()
                    }
                    (false, true) => {
                        debug!("Adding tool: get_current_time");
                        $builder.tool(GetTimeTool).build()
                    }
                    (false, false) => $builder.build(),
                }
            }
            _ => $builder.build(),
        };

        agent.prompt($input).await?
    }};
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
pub enum ApiKeyConfig {
    Direct(String),
    Secret(String), // Reference to secret name in secret manager
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentNode {
    pub model: String,
    pub prompt: Option<String>,
    pub temperature: Option<f64>,
    pub api_key_config: Option<ApiKeyConfig>,
    pub tools: Option<Vec<String>>, // Tool names to enable
}


impl AgentNode {
    pub fn new(
        model: String,
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

    pub fn from_config(config: &serde_json::Value) -> Result<Self> {
        let model = config["model"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Model missing in config"))?
            .to_string();

        let prompt = config
            .get("prompt")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let temperature = config.get("temperature").and_then(|v| v.as_f64());

        let api_key_config = config
            .get("api_key_config")
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()?;

        let tools = config["tools"].as_array().map(|arr| {
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

    pub async fn execute(
        &self,
        input: &str,
        secret_storage: Option<&crate::storage::SecretStorage>,
    ) -> Result<String> {
        // Get API key from direct input or secret manager
        let api_key = match &self.api_key_config {
            Some(ApiKeyConfig::Direct(key)) => key.clone(),
            Some(ApiKeyConfig::Secret(secret_name)) => {
                if let Some(storage) = secret_storage {
                    storage.get_secret(secret_name)?.ok_or_else(|| {
                        anyhow::anyhow!("Secret '{}' not found in secret manager", secret_name)
                    })?
                } else {
                    return Err(anyhow::anyhow!(
                        "Secret manager not available but secret reference is configured"
                    ));
                }
            }
            None => {
                return Err(anyhow::anyhow!(
                    "No API key configured. Please provide api_key_config"
                ));
            }
        };

        let response = match self.model.as_str() {
            m @ ("o4-mini" | "o3" | "o3-mini" | "gpt-4.1" | "gpt-4.1-mini" | "gpt-4.1-nano"
            | "gpt-4" | "gpt-4-turbo" | "gpt-3.5-turbo" | "gpt-4o" | "gpt-4o-mini") => {
                let client = openai::Client::new(&api_key);

                let builder = match m {
                    // O-series models don't support temperature
                    "o4-mini" | "o3" | "o3-mini" => {
                        let mut b = client.agent(m);
                        if let Some(ref prompt) = self.prompt {
                            b = b.preamble(prompt);
                        }
                        b
                    }
                    _ => {
                        let mut b = client.agent(m);
                        if let Some(ref prompt) = self.prompt {
                            b = b.preamble(prompt);
                        }
                        if let Some(temp) = self.temperature {
                            b.temperature(temp)
                        } else {
                            b
                        }
                    }
                };

                build_with_tools!(self, builder, input)
            }

            m @ ("claude-4-opus" | "claude-4-sonnet" | "claude-3.7-sonnet") => {
                let client = anthropic::Client::new(&api_key);

                let mut builder = match m {
                    "claude-4-opus" => client.agent(anthropic::CLAUDE_4_OPUS),
                    "claude-4-sonnet" => client.agent(anthropic::CLAUDE_4_SONNET),
                    "claude-3.7-sonnet" => client.agent(anthropic::CLAUDE_3_7_SONNET),
                    _ => unreachable!(), // We already matched these exact models
                };
                if let Some(ref prompt) = self.prompt {
                    builder = builder.preamble(prompt);
                }
                let builder = if let Some(temp) = self.temperature {
                    builder.temperature(temp)
                } else {
                    builder
                };

                build_with_tools!(self, builder, input)
            }

            m @ ("deepseek-chat" | "deepseek-reasoner") => {
                let client = deepseek::Client::new(&api_key);

                let mut builder = match m {
                    "deepseek-chat" => client.agent(deepseek::DEEPSEEK_CHAT),
                    "deepseek-reasoner" => client.agent(deepseek::DEEPSEEK_REASONER),
                    _ => unreachable!(), // We already matched these exact models
                };
                if let Some(ref prompt) = self.prompt {
                    builder = builder.preamble(prompt);
                }
                let builder = if let Some(temp) = self.temperature {
                    builder.temperature(temp)
                } else {
                    builder
                };

                build_with_tools!(self, builder, input)
            }

            _ => {
                return Err(anyhow::anyhow!(
                    "Unsupported model: {}. Supported models: o4-mini, o3, o3-mini, gpt-4.1, gpt-4.1-mini, gpt-4.1-nano, gpt-4, gpt-4-turbo, gpt-3.5-turbo, gpt-4o, gpt-4o-mini, claude-4-opus, claude-4-sonnet, claude-3.7-sonnet, deepseek-chat, deepseek-reasoner",
                    self.model
                ));
            }
        };

        Ok(response)
    }
}
