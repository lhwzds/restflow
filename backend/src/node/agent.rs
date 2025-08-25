use rig::{agent::AgentBuilder, client::CompletionClient, completion::Prompt, providers::openai};
use serde::{Deserialize, Serialize};
use anyhow::Result;
use crate::tools::{AddTool, GetTimeTool};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentNode {
    pub model: String,
    pub prompt: String,
    pub temperature: f64,
    pub api_key: Option<String>,
    pub tools: Option<Vec<String>>,  // Tool names to enable
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

        let openai = openai::Client::new(&api_key);

        let model = openai.completion_model(&self.model);

        // Build agent with selected tools
        let mut builder = AgentBuilder::new(model)
            .preamble(&self.prompt)
            .temperature(self.temperature);

        // Add tools based on configuration
        if let Some(ref tool_names) = self.tools {
            println!("🔧 Configuring tools for agent: {:?}", tool_names);
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

        let agent = builder.build();
        let response = agent.prompt(input).await?;

        Ok(response)
    }
}
