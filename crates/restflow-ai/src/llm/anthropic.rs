//! Anthropic LLM provider

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AiError, Result};
use crate::http_client::build_http_client;
use crate::llm::client::{
    CompletionRequest, CompletionResponse, FinishReason, LlmClient, Role, TokenUsage, ToolCall,
};

/// Anthropic client
pub struct AnthropicClient {
    client: Client,
    api_key: String,
    model: String,
}

impl AnthropicClient {
    /// Create a new Anthropic client
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: build_http_client(),
            api_key: api_key.into(),
            model: "claude-sonnet-4-20250514".to_string(),
        }
    }

    /// Set the model to use
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: AnthropicContent,
}

#[derive(Serialize)]
#[serde(untagged)]
enum AnthropicContent {
    Text(String),
    Blocks(Vec<AnthropicContentBlock>),
}

#[derive(Serialize)]
struct AnthropicContentBlock {
    r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_use_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    // For tool_use blocks (assistant's tool calls)
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    input: Option<Value>,
}

#[derive(Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: Value,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicResponseContent>,
    stop_reason: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Deserialize)]
struct AnthropicResponseContent {
    r#type: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    input: Option<Value>,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

#[async_trait]
impl LlmClient for AnthropicClient {
    fn provider(&self) -> &str {
        "anthropic"
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        // Extract system message
        let system = request
            .messages
            .iter()
            .find(|m| m.role == Role::System)
            .map(|m| m.content.clone());

        // Convert messages (excluding system)
        let messages: Vec<AnthropicMessage> = request
            .messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| {
                let role = match m.role {
                    Role::User | Role::Tool => "user",
                    Role::Assistant => "assistant",
                    _ => "user",
                }
                .to_string();

                let content = if m.role == Role::Tool {
                    // Tool result message
                    AnthropicContent::Blocks(vec![AnthropicContentBlock {
                        r#type: "tool_result".to_string(),
                        tool_use_id: m.tool_call_id.clone(),
                        content: Some(m.content.clone()),
                        text: None,
                        id: None,
                        name: None,
                        input: None,
                    }])
                } else if let Some(tool_calls) = &m.tool_calls {
                    // Assistant message with tool calls
                    let mut blocks = Vec::new();

                    // Add text block if there's content
                    if !m.content.is_empty() {
                        blocks.push(AnthropicContentBlock {
                            r#type: "text".to_string(),
                            text: Some(m.content.clone()),
                            tool_use_id: None,
                            content: None,
                            id: None,
                            name: None,
                            input: None,
                        });
                    }

                    // Add tool_use blocks
                    for tc in tool_calls {
                        blocks.push(AnthropicContentBlock {
                            r#type: "tool_use".to_string(),
                            text: None,
                            tool_use_id: None,
                            content: None,
                            id: Some(tc.id.clone()),
                            name: Some(tc.name.clone()),
                            input: Some(tc.arguments.clone()),
                        });
                    }

                    AnthropicContent::Blocks(blocks)
                } else {
                    // Regular text message
                    AnthropicContent::Text(m.content.clone())
                };

                AnthropicMessage { role, content }
            })
            .collect();

        let tools: Option<Vec<AnthropicTool>> = if request.tools.is_empty() {
            None
        } else {
            Some(
                request
                    .tools
                    .iter()
                    .map(|t| AnthropicTool {
                        name: t.name.clone(),
                        description: t.description.clone(),
                        input_schema: t.parameters.clone(),
                    })
                    .collect(),
            )
        };

        let body = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: request.max_tokens.unwrap_or(4096),
            system,
            messages,
            tools,
        };

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(AiError::Llm(format!("Anthropic API error: {}", error)));
        }

        let data: AnthropicResponse = response.json().await?;

        let mut content = None;
        let mut tool_calls = vec![];

        for block in data.content {
            match block.r#type.as_str() {
                "text" => content = block.text,
                "tool_use" => {
                    if let (Some(id), Some(name), Some(input)) = (block.id, block.name, block.input)
                    {
                        tool_calls.push(ToolCall {
                            id,
                            name,
                            arguments: input,
                        });
                    }
                }
                _ => {}
            }
        }

        let finish_reason = match data.stop_reason.as_deref() {
            Some("end_turn") => FinishReason::Stop,
            Some("tool_use") => FinishReason::ToolCalls,
            Some("max_tokens") => FinishReason::MaxTokens,
            _ => FinishReason::Stop,
        };

        Ok(CompletionResponse {
            content,
            tool_calls,
            finish_reason,
            usage: Some(TokenUsage {
                prompt_tokens: data.usage.input_tokens,
                completion_tokens: data.usage.output_tokens,
                total_tokens: data.usage.input_tokens + data.usage.output_tokens,
            }),
        })
    }
}
