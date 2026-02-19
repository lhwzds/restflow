//! Anthropic LLM provider

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AiError, Result};
use crate::http_client::build_http_client;
use crate::llm::client::{
    CompletionRequest, CompletionResponse, FinishReason, LlmClient, Role, StreamChunk,
    StreamResult, TokenUsage, ToolCall, ToolCallDelta,
};
use crate::llm::pricing::calculate_cost;
use crate::llm::retry::response_to_error;

/// Anthropic client
pub struct AnthropicClient {
    client: Client,
    api_key: String,
    auth_type: AnthropicAuthType,
    model: String,
    base_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AnthropicAuthType {
    ApiKey,
    OAuth,
}

impl AnthropicAuthType {
    fn from_key(key: &str) -> Self {
        if key.starts_with("sk-ant-oat") {
            Self::OAuth
        } else {
            Self::ApiKey
        }
    }
}

impl AnthropicClient {
    /// Create a new Anthropic client
    pub fn new(api_key: impl Into<String>) -> Self {
        let api_key = api_key.into();
        let auth_type = AnthropicAuthType::from_key(&api_key);
        Self {
            client: build_http_client(),
            api_key,
            auth_type,
            model: "claude-sonnet-4-20250514".to_string(),
            base_url: None,
        }
    }

    /// Set the model to use
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Set a custom base URL (for Anthropic-compatible APIs like MiniMax)
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    fn api_base_url(&self) -> &str {
        self.base_url
            .as_deref()
            .unwrap_or("https://api.anthropic.com")
    }

    fn build_auth_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();

        match self.auth_type {
            AnthropicAuthType::OAuth => {
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {}", self.api_key)).unwrap(),
                );
            }
            AnthropicAuthType::ApiKey => {
                headers.insert(
                    HeaderName::from_static("x-api-key"),
                    HeaderValue::from_str(&self.api_key).unwrap(),
                );
            }
        }

        headers.insert(
            HeaderName::from_static("anthropic-version"),
            HeaderValue::from_static("2023-06-01"),
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        headers
    }
}

fn build_auth_headers(api_key: &str, auth_type: AnthropicAuthType) -> HeaderMap {
    let mut headers = HeaderMap::new();

    match auth_type {
        AnthropicAuthType::OAuth => {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", api_key)).unwrap(),
            );
        }
        AnthropicAuthType::ApiKey => {
            headers.insert(
                HeaderName::from_static("x-api-key"),
                HeaderValue::from_str(api_key).unwrap(),
            );
        }
    }

    headers.insert(
        HeaderName::from_static("anthropic-version"),
        HeaderValue::from_static("2023-06-01"),
    );
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    headers
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

// Streaming response types

/// Anthropic SSE event types
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicStreamEvent {
    MessageStart {
        message: MessageStartPayload,
    },
    ContentBlockStart {
        index: usize,
        content_block: ContentBlockStartPayload,
    },
    ContentBlockDelta {
        index: usize,
        delta: ContentBlockDelta,
    },
    ContentBlockStop {
        #[allow(dead_code)]
        index: usize,
    },
    MessageDelta {
        delta: MessageDeltaPayload,
        usage: Option<OutputUsage>,
    },
    MessageStop,
    Ping,
    Error {
        error: ErrorPayload,
    },
}

#[derive(Debug, Deserialize)]
struct MessageStartPayload {
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
    model: String,
    usage: Option<InputUsage>,
}

#[derive(Debug, Deserialize)]
struct InputUsage {
    input_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct OutputUsage {
    output_tokens: u32,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentBlockStartPayload {
    Text { text: String },
    ToolUse { id: String, name: String },
    Thinking { thinking: String },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)]
enum ContentBlockDelta {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
    ThinkingDelta { thinking: String },
}

#[derive(Debug, Deserialize)]
struct MessageDeltaPayload {
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ErrorPayload {
    message: String,
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
            .post(format!("{}/v1/messages", self.api_base_url()))
            .headers(self.build_auth_headers())
            .json(&body)
            .send()
            .await
            .map_err(AiError::Http)?;

        if !response.status().is_success() {
            return Err(response_to_error(response, "Anthropic").await);
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

        let cost_usd = calculate_cost(
            &self.model,
            data.usage.input_tokens,
            data.usage.output_tokens,
        );

        Ok(CompletionResponse {
            content,
            tool_calls,
            finish_reason,
            usage: Some(TokenUsage {
                prompt_tokens: data.usage.input_tokens,
                completion_tokens: data.usage.output_tokens,
                total_tokens: data.usage.input_tokens + data.usage.output_tokens,
                cost_usd,
            }),
        })
    }

    fn complete_stream(&self, request: CompletionRequest) -> StreamResult {
        let client = self.client.clone();
        let api_key = self.api_key.clone();
        let model = self.model.clone();
        let auth_type = self.auth_type;
        let base_url = self.api_base_url().to_string();

        Box::pin(async_stream::stream! {
            // OAuth tokens can't use Anthropic API - CLI doesn't support streaming
            if auth_type == AnthropicAuthType::OAuth {
                yield Err(AiError::Llm(
                    "Streaming not supported with OAuth tokens. Use non-streaming mode.".to_string()
                ));
                return;
            }

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
                        let mut blocks = Vec::new();
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

            // Build streaming request body
            let body = serde_json::json!({
                "model": model,
                "max_tokens": request.max_tokens.unwrap_or(4096),
                "system": system,
                "messages": messages,
                "tools": tools,
                "stream": true
            });

            let response = match client
                .post(format!("{}/v1/messages", base_url))
                .headers(build_auth_headers(&api_key, auth_type))
                .json(&body)
                .send()
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    yield Err(AiError::Http(e));
                    return;
                }
            };

            if !response.status().is_success() {
                yield Err(response_to_error(response, "Anthropic").await);
                return;
            }

            let mut byte_stream = response.bytes_stream();
            let mut buffer = String::new();
            let mut input_tokens = 0u32;
            let mut output_tokens = 0u32;
            let mut _current_tool_index: Option<usize> = None;
            let mut current_tool_id: Option<String> = None;
            let mut current_tool_name: Option<String> = None;

            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = match chunk_result {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        yield Err(AiError::Http(e));
                        return;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // Process complete SSE events from buffer
                while let Some(pos) = buffer.find("\n\n") {
                    let event_str = buffer[..pos].to_string();
                    buffer = buffer[pos + 2..].to_string();

                    // Parse SSE event
                    for line in event_str.lines() {
                        if let Some(data) = line.strip_prefix("data: ") {
                            if data.trim().is_empty() {
                                continue;
                            }

                            let event: AnthropicStreamEvent = match serde_json::from_str(data) {
                                Ok(e) => e,
                                Err(_) => continue,
                            };

                            match event {
                                AnthropicStreamEvent::MessageStart { message } => {
                                    if let Some(usage) = message.usage {
                                        input_tokens = usage.input_tokens;
                                    }
                                }
                                AnthropicStreamEvent::ContentBlockStart { index, content_block } => {
                                    match content_block {
                                        ContentBlockStartPayload::Text { text } => {
                                            if !text.is_empty() {
                                                yield Ok(StreamChunk::text(&text));
                                            }
                                        }
                                        ContentBlockStartPayload::ToolUse { id, name } => {
                                            _current_tool_index = Some(index);
                                            current_tool_id = Some(id.clone());
                                            current_tool_name = Some(name.clone());
                                            yield Ok(StreamChunk {
                                                text: String::new(),
                                                thinking: None,
                                                tool_call_delta: Some(ToolCallDelta {
                                                    index,
                                                    id: Some(id),
                                                    name: Some(name),
                                                    arguments: None,
                                                }),
                                                finish_reason: None,
                                                usage: None,
                                            });
                                        }
                                        ContentBlockStartPayload::Thinking { thinking } => {
                                            if !thinking.is_empty() {
                                                yield Ok(StreamChunk::thinking(&thinking));
                                            }
                                        }
                                    }
                                }
                                AnthropicStreamEvent::ContentBlockDelta { index, delta } => {
                                    match delta {
                                        ContentBlockDelta::TextDelta { text } => {
                                            yield Ok(StreamChunk::text(&text));
                                        }
                                        ContentBlockDelta::InputJsonDelta { partial_json } => {
                                            yield Ok(StreamChunk {
                                                text: String::new(),
                                                thinking: None,
                                                tool_call_delta: Some(ToolCallDelta {
                                                    index,
                                                    id: current_tool_id.clone(),
                                                    name: current_tool_name.clone(),
                                                    arguments: Some(partial_json),
                                                }),
                                                finish_reason: None,
                                                usage: None,
                                            });
                                        }
                                        ContentBlockDelta::ThinkingDelta { thinking } => {
                                            yield Ok(StreamChunk::thinking(&thinking));
                                        }
                                    }
                                }
                                AnthropicStreamEvent::ContentBlockStop { index: _ } => {
                                    _current_tool_index = None;
                                    current_tool_id = None;
                                    current_tool_name = None;
                                }
                                AnthropicStreamEvent::MessageDelta { delta, usage } => {
                                    if let Some(u) = usage {
                                        output_tokens = u.output_tokens;
                                    }
                                    if let Some(stop_reason) = delta.stop_reason {
                                        let finish_reason = match stop_reason.as_str() {
                                            "end_turn" => FinishReason::Stop,
                                            "tool_use" => FinishReason::ToolCalls,
                                            "max_tokens" => FinishReason::MaxTokens,
                                            _ => FinishReason::Stop,
                                        };
                                        let cost_usd = calculate_cost(&model, input_tokens, output_tokens);
                                        yield Ok(StreamChunk::final_chunk(
                                            finish_reason,
                                            Some(TokenUsage {
                                                prompt_tokens: input_tokens,
                                                completion_tokens: output_tokens,
                                                total_tokens: input_tokens + output_tokens,
                                                cost_usd,
                                            }),
                                        ));
                                    }
                                }
                                AnthropicStreamEvent::MessageStop => {
                                    // Stream complete
                                }
                                AnthropicStreamEvent::Ping => {
                                    // Keep-alive, ignore
                                }
                                AnthropicStreamEvent::Error { error } => {
                                    yield Err(AiError::Llm(format!("Stream error: {}", error.message)));
                                    return;
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
                        if data.trim().is_empty() {
                            continue;
                        }
                        if let Ok(AnthropicStreamEvent::Error { error }) = serde_json::from_str::<AnthropicStreamEvent>(data) {
                            yield Err(AiError::Llm(format!("Stream error: {}", error.message)));
                        }
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_type_detection() {
        assert_eq!(
            AnthropicAuthType::from_key("sk-ant-oat01-xxx"),
            AnthropicAuthType::OAuth
        );
        assert_eq!(
            AnthropicAuthType::from_key("sk-ant-api03-xxx"),
            AnthropicAuthType::ApiKey
        );
    }

    #[test]
    fn test_oauth_headers() {
        let headers = build_auth_headers("sk-ant-oat01-test", AnthropicAuthType::OAuth);
        assert!(headers.contains_key(AUTHORIZATION));
        assert!(!headers.contains_key("x-api-key"));
    }
}
