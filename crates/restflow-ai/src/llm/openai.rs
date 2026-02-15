//! OpenAI LLM provider

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AiError, Result};
use crate::http_client::build_http_client;
use crate::llm::client::{
    CompletionRequest, CompletionResponse, FinishReason, LlmClient, Role, StreamChunk,
    StreamResult, TokenUsage, ToolCall, ToolCallDelta,
};
use crate::llm::pricing::calculate_cost;
use crate::llm::retry::{LlmRetryConfig, response_to_error};

/// OpenAI client
pub struct OpenAIClient {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
    retry_config: LlmRetryConfig,
}

impl OpenAIClient {
    /// Create a new OpenAI client
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: build_http_client(),
            api_key: api_key.into(),
            model: "gpt-4o".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            retry_config: LlmRetryConfig::default(),
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

    pub fn with_retry_config(mut self, config: LlmRetryConfig) -> Self {
        self.retry_config = config;
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

#[derive(Deserialize, Debug)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

// Streaming types

#[derive(Deserialize, Debug)]
struct OpenAIStreamResponse {
    choices: Vec<OpenAIStreamChoice>,
    usage: Option<OpenAIUsage>,
}

#[derive(Deserialize, Debug)]
struct OpenAIStreamChoice {
    delta: OpenAIStreamDelta,
    finish_reason: Option<String>,
}

#[derive(Deserialize, Debug)]
struct OpenAIStreamDelta {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAIStreamToolCall>>,
}

#[derive(Deserialize, Debug)]
struct OpenAIStreamToolCall {
    index: usize,
    id: Option<String>,
    function: Option<OpenAIStreamFunction>,
}

#[derive(Deserialize, Debug)]
struct OpenAIStreamFunction {
    name: Option<String>,
    arguments: Option<String>,
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
                                arguments: serde_json::to_string(&tc.arguments).unwrap_or_default(),
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

        let mut last_error = None;

        for attempt in 0..=self.retry_config.max_retries {
            let response = match self
                .client
                .post(format!("{}/chat/completions", self.base_url))
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    let error = AiError::Http(e);
                    if !error.is_retryable() || attempt == self.retry_config.max_retries {
                        return Err(error);
                    }
                    let delay = self.retry_config.delay_for(attempt + 1, None);
                    tracing::warn!(
                        attempt = attempt + 1,
                        delay_ms = delay.as_millis(),
                        "Retrying OpenAI request after connection error"
                    );
                    tokio::time::sleep(delay).await;
                    last_error = Some(error);
                    continue;
                }
            };

            if response.status().is_success() {
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
                        arguments: serde_json::from_str(&tc.function.arguments)
                            .unwrap_or(Value::Null),
                    })
                    .collect();

                let finish_reason = match choice.finish_reason.as_str() {
                    "stop" => FinishReason::Stop,
                    "tool_calls" => FinishReason::ToolCalls,
                    "length" => FinishReason::MaxTokens,
                    _ => FinishReason::Error,
                };

                let usage = data.usage.map(|u| {
                    let cost_usd =
                        calculate_cost(&self.model, u.prompt_tokens, u.completion_tokens);
                    TokenUsage {
                        prompt_tokens: u.prompt_tokens,
                        completion_tokens: u.completion_tokens,
                        total_tokens: u.total_tokens,
                        cost_usd,
                    }
                });

                return Ok(CompletionResponse {
                    content: choice.message.content,
                    tool_calls,
                    finish_reason,
                    usage,
                });
            }

            let error = response_to_error(response, "OpenAI").await;
            if !error.is_retryable() || attempt == self.retry_config.max_retries {
                return Err(error);
            }

            let delay = self
                .retry_config
                .delay_for(attempt + 1, error.retry_after());
            tracing::warn!(
                attempt = attempt + 1,
                delay_ms = delay.as_millis(),
                "Retrying OpenAI request"
            );
            tokio::time::sleep(delay).await;
            last_error = Some(error);
        }

        Err(last_error
            .unwrap_or_else(|| AiError::Llm("OpenAI request failed after retries".to_string())))
    }

    fn complete_stream(&self, request: CompletionRequest) -> StreamResult {
        let client = self.client.clone();
        let api_key = self.api_key.clone();
        let base_url = self.base_url.clone();
        let model = self.model.clone();

        Box::pin(async_stream::stream! {
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

                    let tool_calls = m.tool_calls.as_ref().map(|tcs| {
                        tcs.iter()
                            .map(|tc| OpenAIMessageToolCall {
                                id: tc.id.clone(),
                                r#type: "function".to_string(),
                                function: OpenAIMessageFunction {
                                    name: tc.name.clone(),
                                    arguments: serde_json::to_string(&tc.arguments).unwrap_or_default(),
                                },
                            })
                            .collect()
                    });

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

            let body = serde_json::json!({
                "model": model,
                "messages": messages,
                "tools": tools,
                "temperature": request.temperature,
                "max_tokens": request.max_tokens,
                "stream": true,
                "stream_options": { "include_usage": true }
            });

            let response = match client
                .post(format!("{}/chat/completions", base_url))
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    yield Err(AiError::Llm(format!("Request failed: {}", e)));
                    return;
                }
            };

            if !response.status().is_success() {
                yield Err(response_to_error(response, "OpenAI").await);
                return;
            }

            let mut byte_stream = response.bytes_stream();
            let mut buffer = String::new();
            let mut tool_call_ids: std::collections::HashMap<usize, String> = std::collections::HashMap::new();
            let mut tool_call_names: std::collections::HashMap<usize, String> = std::collections::HashMap::new();

            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = match chunk_result {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        yield Err(AiError::Llm(format!("Stream error: {}", e)));
                        return;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // Process complete SSE events from buffer
                while let Some(pos) = buffer.find("\n\n") {
                    let event_str = buffer[..pos].to_string();
                    buffer = buffer[pos + 2..].to_string();

                    for line in event_str.lines() {
                        if let Some(data) = line.strip_prefix("data: ") {
                            if data.trim() == "[DONE]" {
                                continue;
                            }

                            let parsed: OpenAIStreamResponse = match serde_json::from_str(data) {
                                Ok(p) => p,
                                Err(_) => continue,
                            };

                            // Handle usage (at the end of stream)
                            if let Some(usage) = parsed.usage {
                                yield Ok(StreamChunk::final_chunk(
                                    FinishReason::Stop,
                                    Some(TokenUsage {
                                        prompt_tokens: usage.prompt_tokens,
                                        completion_tokens: usage.completion_tokens,
                                        total_tokens: usage.total_tokens,
                                        cost_usd: calculate_cost(
                                            &model,
                                            usage.prompt_tokens,
                                            usage.completion_tokens,
                                        ),
                                    }),
                                ));
                                continue;
                            }

                            for choice in parsed.choices {
                                // Handle finish reason
                                if let Some(finish_reason) = choice.finish_reason {
                                    let reason = match finish_reason.as_str() {
                                        "stop" => FinishReason::Stop,
                                        "tool_calls" => FinishReason::ToolCalls,
                                        "length" => FinishReason::MaxTokens,
                                        _ => FinishReason::Error,
                                    };
                                    // Final chunk with reason but no usage yet (usage comes separately)
                                    if reason != FinishReason::Stop {
                                        yield Ok(StreamChunk::final_chunk(reason, None));
                                    }
                                    continue;
                                }

                                // Handle content delta
                                if let Some(content) = choice.delta.content
                                    && !content.is_empty()
                                {
                                    yield Ok(StreamChunk::text(&content));
                                }

                                // Handle tool calls delta
                                if let Some(tool_calls) = choice.delta.tool_calls {
                                    for tc in tool_calls {
                                        // Store id and name when they first appear
                                        if let Some(id) = &tc.id {
                                            tool_call_ids.insert(tc.index, id.clone());
                                        }
                                        if let Some(func) = &tc.function
                                            && let Some(name) = &func.name
                                        {
                                            tool_call_names.insert(tc.index, name.clone());
                                        }

                                        let arguments = tc.function.as_ref().and_then(|f| f.arguments.clone());

                                        yield Ok(StreamChunk {
                                            text: String::new(),
                                            thinking: None,
                                            tool_call_delta: Some(ToolCallDelta {
                                                index: tc.index,
                                                id: tool_call_ids.get(&tc.index).cloned(),
                                                name: tool_call_names.get(&tc.index).cloned(),
                                                arguments,
                                            }),
                                            finish_reason: None,
                                            usage: None,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Process any remaining data in the buffer after the stream ends.
            // This handles the case where the last SSE event lacks a trailing \n\n
            // (e.g., due to a network interruption).
            let remaining = buffer.trim();
            if !remaining.is_empty() {
                for line in remaining.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if data.trim() == "[DONE]" || data.trim().is_empty() {
                            continue;
                        }
                        // Best effort: try to parse final event
                        if let Ok(parsed) = serde_json::from_str::<OpenAIStreamResponse>(data)
                            && let Some(usage) = parsed.usage
                        {
                            yield Ok(StreamChunk::final_chunk(
                                FinishReason::Stop,
                                Some(TokenUsage {
                                    prompt_tokens: usage.prompt_tokens,
                                    completion_tokens: usage.completion_tokens,
                                    total_tokens: usage.total_tokens,
                                    cost_usd: calculate_cost(
                                        &model,
                                        usage.prompt_tokens,
                                        usage.completion_tokens,
                                    ),
                                }),
                            ));
                        }
                    }
                }
            }
        })
    }
}
