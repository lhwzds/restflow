//! OpenAI LLM provider

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AiError, Result};
use crate::llm::client::{
    CompletionRequest, CompletionResponse, FinishReason, LlmClient, Role, TokenUsage, ToolCall,
};

/// OpenAI client
pub struct OpenAIClient {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl OpenAIClient {
    /// Create a new OpenAI client
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            model: "gpt-4o".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
        }
    }

    /// Set the model to use
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Set custom base URL (for API-compatible services)
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }
}

#[derive(Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAITool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Serialize)]
struct OpenAIMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAIMessageToolCall>>,
}

#[derive(Serialize)]
struct OpenAIMessageToolCall {
    id: String,
    r#type: String,
    function: OpenAIMessageFunction,
}

#[derive(Serialize)]
struct OpenAIMessageFunction {
    name: String,
    arguments: String,
}

#[derive(Serialize)]
struct OpenAITool {
    r#type: String,
    function: OpenAIFunction,
}

#[derive(Serialize)]
struct OpenAIFunction {
    name: String,
    description: String,
    parameters: Value,
}

#[derive(Deserialize)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
    usage: Option<OpenAIUsage>,
}

#[derive(Deserialize)]
struct OpenAIChoice {
    message: OpenAIResponseMessage,
    finish_reason: String,
}

#[derive(Deserialize)]
struct OpenAIResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAIToolCall>>,
}

#[derive(Deserialize)]
struct OpenAIToolCall {
    id: String,
    function: OpenAIFunctionCall,
}

#[derive(Deserialize)]
struct OpenAIFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[async_trait]
impl LlmClient for OpenAIClient {
    fn provider(&self) -> &str {
        "openai"
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let messages: Vec<OpenAIMessage> = request
            .messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                }
                .to_string();

                // Convert tool_calls if present
                let tool_calls = m.tool_calls.as_ref().map(|tcs| {
                    tcs.iter()
                        .map(|tc| OpenAIMessageToolCall {
                            id: tc.id.clone(),
                            r#type: "function".to_string(),
                            function: OpenAIMessageFunction {
                                name: tc.name.clone(),
                                arguments: serde_json::to_string(&tc.arguments)
                                    .unwrap_or_default(),
                            },
                        })
                        .collect()
                });

                // For assistant messages with tool_calls, content can be null
                let content = if m.tool_calls.is_some() && m.content.is_empty() {
                    None
                } else {
                    Some(m.content.clone())
                };

                OpenAIMessage {
                    role,
                    content,
                    tool_call_id: m.tool_call_id.clone(),
                    tool_calls,
                }
            })
            .collect();

        let tools: Option<Vec<OpenAITool>> = if request.tools.is_empty() {
            None
        } else {
            Some(
                request
                    .tools
                    .iter()
                    .map(|t| OpenAITool {
                        r#type: "function".to_string(),
                        function: OpenAIFunction {
                            name: t.name.clone(),
                            description: t.description.clone(),
                            parameters: t.parameters.clone(),
                        },
                    })
                    .collect(),
            )
        };

        let body = OpenAIRequest {
            model: self.model.clone(),
            messages,
            tools,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
        };

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(AiError::Llm(format!("OpenAI API error: {}", error)));
        }

        let data: OpenAIResponse = response.json().await?;
        let choice = data
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| AiError::Llm("No response from OpenAI".to_string()))?;

        let tool_calls = choice
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .map(|tc| ToolCall {
                id: tc.id,
                name: tc.function.name,
                arguments: serde_json::from_str(&tc.function.arguments).unwrap_or(Value::Null),
            })
            .collect();

        let finish_reason = match choice.finish_reason.as_str() {
            "stop" => FinishReason::Stop,
            "tool_calls" => FinishReason::ToolCalls,
            "length" => FinishReason::MaxTokens,
            _ => FinishReason::Error,
        };

        let usage = data.usage.map(|u| TokenUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });

        Ok(CompletionResponse {
            content: choice.message.content,
            tool_calls,
            finish_reason,
            usage,
        })
    }
}
