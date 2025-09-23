use rig::{
    client::CompletionClient,
    completion::Prompt,
    providers::{openai, anthropic, deepseek}
};
use serde::{Deserialize, Serialize};
use anyhow::Result;
use ts_rs::TS;
use crate::tools::{AddTool, GetTimeTool};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentNode {
    pub model: String,
    pub prompt: String,
    pub temperature: f64,
    pub api_key: Option<String>,
    pub tools: Option<Vec<String>>,  // Tool names to enable
}

macro_rules! configure_tools {
    ($self:expr, $builder:expr) => {{
        let mut builder = $builder;
        if let Some(ref tool_names) = $self.tools {
            println!("🔧 Configuring tools: {:?}", tool_names);
            for tool_name in tool_names {
                match tool_name.as_str() {
                    "add" => {
                        builder = builder.tool(AddTool);
                        println!("✅ Added tool: add");
                    }
                    "get_current_time" => {
                        builder = builder.tool(GetTimeTool);
                        println!("✅ Added tool: get_current_time");
                    }
                    unknown => {
                        println!("⚠️ Unknown tool: {}", unknown);
                    }
                }
            }
        }
        builder
    }};
}

impl AgentNode {
    pub fn new(model: String, prompt: String, temperature: f64, api_key: Option<String>) -> Self {
        Self {
            model,
            prompt,
            temperature,
            api_key,
            tools: None,
        }
    }


    pub fn from_config(config: &serde_json::Value) -> Result<Self> {
        let model = config["model"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Model missing in config"))?
            .to_string();

        let prompt = config["prompt"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Prompt missing in config"))?
            .to_string();

        let temperature = config["temperature"]
            .as_f64()
            .ok_or_else(|| anyhow::anyhow!("Temperature missing in config"))?;

        let api_key = config["api_key"].as_str().map(|s| s.to_string());

        let tools = config["tools"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            });

        Ok(Self {
            model,
            prompt,
            temperature,
            api_key,
            tools,
        })
    }

    pub async fn execute(&self, input: &str) -> Result<String> {
        let api_key = self
            .api_key
            .clone()
            .ok_or_else(|| anyhow::anyhow!("API key not found. Please provide api key"))?;

        let response = match self.model.as_str() {
            m if m.starts_with("gpt") || m.starts_with("o") => {
                let client = openai::Client::new(&api_key);

                let builder = match m {
                    "o4-mini" => client.agent(openai::O4_MINI),
                    "o3" => client.agent(openai::O3),
                    "o3-mini" => client.agent(openai::O3_MINI),

                    "gpt-4.1" => client.agent(openai::GPT_4_1),
                    "gpt-4.1-mini" => client.agent(openai::GPT_4_1_MINI),
                    "gpt-4.1-nano" => client.agent(openai::GPT_4_1_NANO),

                    _ => client.agent(m),
                }
                .preamble(&self.prompt)
                .temperature(self.temperature);

                let builder = configure_tools!(self, builder);
                let agent = builder.build();
                agent.prompt(input).await?
            },

            m if m.starts_with("claude") => {
                let client = anthropic::Client::new(&api_key);

                let builder = match m {
                    "claude-4-opus" | "claude-opus-4" => client.agent(anthropic::CLAUDE_4_OPUS),
                    "claude-4-sonnet" | "claude-sonnet-4" => client.agent(anthropic::CLAUDE_4_SONNET),
                    "claude-3.7-sonnet" | "claude-3-7-sonnet" => client.agent(anthropic::CLAUDE_3_7_SONNET),
                    _ => client.agent(m),
                }
                .preamble(&self.prompt)
                .temperature(self.temperature);

                let builder = configure_tools!(self, builder);
                let agent = builder.build();
                agent.prompt(input).await?
            },

            m if m.contains("deepseek") => {
                let client = deepseek::Client::new(&api_key);

                let builder = match m {
                    "deepseek-chat" => client.agent(deepseek::DEEPSEEK_CHAT),
                    "deepseek-reasoner" => client.agent(deepseek::DEEPSEEK_REASONER),
                    _ => client.agent(m),
                }
                .preamble(&self.prompt)
                .temperature(self.temperature);

                let builder = configure_tools!(self, builder);
                let agent = builder.build();
                agent.prompt(input).await?
            },

            _ => {
                return Err(anyhow::anyhow!(
                    "Unsupported model: {}. Supported prefixes: gpt (OpenAI), claude (Anthropic), deepseek (DeepSeek)",
                    self.model
                ));
            }
        };

        Ok(response)
    }
}
