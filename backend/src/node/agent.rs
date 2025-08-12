use rig::{agent::AgentBuilder, client::CompletionClient, completion::Prompt, providers::openai};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentNode {
    pub model: String,
    pub prompt: String,
    pub temperature: f64,
    pub api_key: Option<String>,
}

impl AgentNode {
    pub fn new(model: String, prompt: String, temperature: f64, api_key: Option<String>) -> Self {
        Self {
            model,
            prompt,
            temperature,
            api_key,
        }
    }

    pub async fn execute(&self, input: &str) -> Result<String, Box<dyn std::error::Error>> {
        let api_key = self
            .api_key
            .clone()
            .ok_or("API key not found. Please provide api key.")?;

        let openai = openai::Client::new(&api_key);

        let model = openai.completion_model(&self.model);

        let agent = AgentBuilder::new(model)
            .preamble(&self.prompt)
            .temperature(self.temperature)
            .build();

        let response = agent.prompt(input).await?;

        Ok(response)
    }
}
