//! DeepSeek LLM provider.
//!
//! DeepSeek is OpenAI-compatible, but thinking-mode tool calls require the
//! provider-specific `reasoning_content` field to be preserved and sent back on
//! subsequent assistant tool-call messages.

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AiError, Result};
use crate::llm::client::{
    CompletionRequest, CompletionResponse, FinishReason, LlmClient, Role, StreamChunk,
    StreamResult, TokenUsage, ToolCall, ToolCallDelta,
};
use crate::llm::pricing::calculate_cost;
use crate::llm::retry::response_to_error;
use types::http_client::build_http_client;

/// DeepSeek HTTP client.
pub struct DeepSeekClient {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl DeepSeekClient {
    /// Create a new DeepSeek client.
    pub fn new(api_key: impl Into<String>) -> std::result::Result<Self, reqwest::Error> {
        Ok(Self {
            client: build_http_client()?,
            api_key: api_key.into(),
            model: "deepseek-chat".to_string(),
            base_url: "https://api.deepseek.com/v1".to_string(),
        })
    }

    /// Set the model to use.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Set custom base URL.
    #[allow(dead_code)]
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }
}

#[derive(Serialize)]
struct DeepSeekRequest {
    model: String,
    messages: Vec<DeepSeekMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<DeepSeekTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Serialize)]
struct DeepSeekMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<DeepSeekMessageToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_content: Option<String>,
}

#[derive(Serialize)]
struct DeepSeekMessageToolCall {
    id: String,
    r#type: String,
    function: DeepSeekMessageFunction,
}

#[derive(Serialize)]
struct DeepSeekMessageFunction {
    name: String,
    arguments: String,
}

#[derive(Serialize)]
struct DeepSeekTool {
    r#type: String,
    function: DeepSeekFunction,
}

#[derive(Serialize)]
struct DeepSeekFunction {
    name: String,
    description: String,
    parameters: Value,
}

#[derive(Deserialize)]
struct DeepSeekResponse {
    choices: Vec<DeepSeekChoice>,
    usage: Option<DeepSeekUsage>,
}

#[derive(Deserialize)]
struct DeepSeekChoice {
    message: DeepSeekResponseMessage,
    finish_reason: String,
}

#[derive(Deserialize)]
struct DeepSeekResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<DeepSeekToolCall>>,
    reasoning_content: Option<String>,
}

#[derive(Deserialize)]
struct DeepSeekToolCall {
    id: String,
    function: DeepSeekFunctionCall,
}

#[derive(Deserialize)]
struct DeepSeekFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Deserialize, Debug)]
struct DeepSeekUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Deserialize, Debug)]
struct DeepSeekStreamResponse {
    choices: Vec<DeepSeekStreamChoice>,
    usage: Option<DeepSeekUsage>,
}

#[derive(Deserialize, Debug)]
struct DeepSeekStreamChoice {
    delta: DeepSeekStreamDelta,
    finish_reason: Option<String>,
}

#[derive(Deserialize, Debug)]
struct DeepSeekStreamDelta {
    content: Option<String>,
    tool_calls: Option<Vec<DeepSeekStreamToolCall>>,
    reasoning_content: Option<String>,
}

#[derive(Deserialize, Debug)]
struct DeepSeekStreamToolCall {
    index: usize,
    id: Option<String>,
    function: Option<DeepSeekStreamFunction>,
}

#[derive(Deserialize, Debug)]
struct DeepSeekStreamFunction {
    name: Option<String>,
    arguments: Option<String>,
}

fn convert_messages(request: &CompletionRequest) -> Vec<DeepSeekMessage> {
    request
        .messages
        .iter()
        .map(|message| {
            let role = match message.role {
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::Tool => "tool",
            }
            .to_string();

            let tool_calls = message.tool_calls.as_ref().map(|calls| {
                calls
                    .iter()
                    .map(|call| DeepSeekMessageToolCall {
                        id: call.id.clone(),
                        r#type: "function".to_string(),
                        function: DeepSeekMessageFunction {
                            name: call.name.clone(),
                            arguments: serde_json::to_string(&call.arguments).unwrap_or_default(),
                        },
                    })
                    .collect()
            });

            let content = if message.tool_calls.is_some() && message.content.is_empty() {
                None
            } else {
                Some(message.content.clone())
            };

            let reasoning_content = if message.role == Role::Assistant {
                message.reasoning_content.clone()
            } else {
                None
            };

            DeepSeekMessage {
                role,
                content,
                tool_call_id: message.tool_call_id.clone(),
                tool_calls,
                reasoning_content,
            }
        })
        .collect()
}

fn convert_tools(request: &CompletionRequest) -> Option<Vec<DeepSeekTool>> {
    if request.tools.is_empty() {
        None
    } else {
        Some(
            request
                .tools
                .iter()
                .map(|tool| DeepSeekTool {
                    r#type: "function".to_string(),
                    function: DeepSeekFunction {
                        name: tool.name.clone(),
                        description: tool.description.clone(),
                        parameters: tool.parameters.clone(),
                    },
                })
                .collect(),
        )
    }
}

fn map_finish_reason(reason: &str) -> FinishReason {
    match reason {
        "stop" => FinishReason::Stop,
        "tool_calls" => FinishReason::ToolCalls,
        "length" => FinishReason::MaxTokens,
        _ => FinishReason::Error,
    }
}

#[async_trait]
impl LlmClient for DeepSeekClient {
    fn provider(&self) -> &str {
        "deepseek"
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let body = DeepSeekRequest {
            model: self.model.clone(),
            messages: convert_messages(&request),
            tools: convert_tools(&request),
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
            .await
            .map_err(AiError::Http)?;

        if !response.status().is_success() {
            return Err(response_to_error(response, "DeepSeek").await);
        }

        let data: DeepSeekResponse = response.json().await?;
        let choice = data
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| AiError::Llm("No response from DeepSeek".to_string()))?;

        let tool_calls = choice
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .map(|call| ToolCall {
                id: call.id,
                name: call.function.name,
                arguments: serde_json::from_str(&call.function.arguments).unwrap_or(Value::Null),
            })
            .collect();

        let usage = data.usage.map(|usage| {
            let cost_usd =
                calculate_cost(&self.model, usage.prompt_tokens, usage.completion_tokens);
            TokenUsage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
                cost_usd,
            }
        });

        Ok(CompletionResponse {
            content: choice.message.content,
            tool_calls,
            finish_reason: map_finish_reason(&choice.finish_reason),
            usage,
            reasoning_content: choice.message.reasoning_content,
        })
    }

    fn complete_stream(&self, request: CompletionRequest) -> StreamResult {
        let client = self.client.clone();
        let api_key = self.api_key.clone();
        let base_url = self.base_url.clone();
        let model = self.model.clone();

        Box::pin(async_stream::stream! {
            let mut body = serde_json::json!({
                "model": model,
                "messages": convert_messages(&request),
                "tools": convert_tools(&request),
                "temperature": request.temperature,
                "stream": true,
                "stream_options": { "include_usage": true }
            });
            if let Some(max_tokens) = request.max_tokens {
                body["max_tokens"] = Value::from(max_tokens);
            }

            let response = match client
                .post(format!("{}/chat/completions", base_url))
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    yield Err(AiError::Llm(format!("Request failed: {}", error)));
                    return;
                }
            };

            if !response.status().is_success() {
                yield Err(response_to_error(response, "DeepSeek").await);
                return;
            }

            let mut byte_stream = response.bytes_stream();
            let mut buffer = String::new();
            let mut tool_call_ids = std::collections::HashMap::<usize, String>::new();
            let mut tool_call_names = std::collections::HashMap::<usize, String>::new();

            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = match chunk_result {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        yield Err(AiError::Llm(format!("Stream error: {}", error)));
                        return;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(pos) = buffer.find("\n\n") {
                    let event_str = buffer[..pos].to_string();
                    buffer = buffer[pos + 2..].to_string();

                    for line in event_str.lines() {
                        if let Some(data) = line.strip_prefix("data: ") {
                            if data.trim() == "[DONE]" {
                                continue;
                            }

                            let parsed: DeepSeekStreamResponse = match serde_json::from_str(data) {
                                Ok(parsed) => parsed,
                                Err(_) => continue,
                            };

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
                                if let Some(finish_reason) = choice.finish_reason {
                                    yield Ok(StreamChunk::final_chunk(
                                        map_finish_reason(&finish_reason),
                                        None,
                                    ));
                                    continue;
                                }

                                if let Some(reasoning) = &choice.delta.reasoning_content
                                    && !reasoning.is_empty()
                                {
                                    yield Ok(StreamChunk::thinking(reasoning));
                                }

                                if let Some(content) = &choice.delta.content
                                    && !content.is_empty()
                                {
                                    yield Ok(StreamChunk::text(content));
                                }

                                if let Some(tool_calls) = choice.delta.tool_calls {
                                    for call in tool_calls {
                                        if let Some(id) = &call.id {
                                            tool_call_ids.insert(call.index, id.clone());
                                        }
                                        if let Some(function) = &call.function
                                            && let Some(name) = &function.name
                                        {
                                            tool_call_names.insert(call.index, name.clone());
                                        }

                                        let arguments = call
                                            .function
                                            .as_ref()
                                            .and_then(|function| function.arguments.clone());

                                        yield Ok(StreamChunk {
                                            text: String::new(),
                                            thinking: None,
                                            tool_call_delta: Some(ToolCallDelta {
                                                index: call.index,
                                                id: tool_call_ids.get(&call.index).cloned(),
                                                name: tool_call_names.get(&call.index).cloned(),
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

            let remaining = buffer.trim();
            if !remaining.is_empty() {
                for line in remaining.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if data.trim() == "[DONE]" || data.trim().is_empty() {
                            continue;
                        }
                        if let Ok(parsed) = serde_json::from_str::<DeepSeekStreamResponse>(data)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::client::{Message, ToolCall};
    use serde_json::json;

    #[test]
    fn request_serializes_reasoning_content_on_assistant_messages() {
        let request = CompletionRequest::new(vec![
            Message::user("hello"),
            Message::assistant_with_tool_calls_and_reasoning(
                None,
                vec![ToolCall {
                    id: "call_1".to_string(),
                    name: "bash".to_string(),
                    arguments: json!({"command": "ls"}),
                }],
                Some("Let me think about this...".to_string()),
            ),
            Message::tool_result("call_1", "file1.txt\nfile2.txt"),
        ]);

        let messages = convert_messages(&request);
        assert!(messages[0].reasoning_content.is_none());
        assert_eq!(
            messages[1].reasoning_content.as_deref(),
            Some("Let me think about this...")
        );
        assert!(messages[2].reasoning_content.is_none());

        let serialized = serde_json::to_value(&messages[1]).unwrap();
        assert_eq!(
            serialized["reasoning_content"],
            "Let me think about this..."
        );
        assert!(serialized["tool_calls"].is_array());
    }

    #[test]
    fn request_omits_reasoning_content_when_none() {
        let request =
            CompletionRequest::new(vec![Message::user("hello"), Message::assistant("hi there")]);

        let messages = convert_messages(&request);
        let serialized = serde_json::to_value(&messages[1]).unwrap();
        assert!(serialized.get("reasoning_content").is_none());
    }

    #[test]
    fn request_ignores_reasoning_content_on_non_assistant_messages() {
        let mut user_msg = Message::user("hello");
        user_msg.reasoning_content = Some("should be ignored".to_string());

        let request = CompletionRequest::new(vec![user_msg]);
        let messages = convert_messages(&request);
        assert!(messages[0].reasoning_content.is_none());
    }

    #[test]
    fn stream_response_deserializes_reasoning_content() {
        let json = r#"{"choices":[{"delta":{"reasoning_content":"Hmm, let me think..."},"finish_reason":null}]}"#;
        let parsed: DeepSeekStreamResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            parsed.choices[0].delta.reasoning_content.as_deref(),
            Some("Hmm, let me think...")
        );
    }

    #[test]
    fn response_deserializes_reasoning_content() {
        let json = r#"{
            "choices": [{
                "message": {
                    "content": null,
                    "tool_calls": [{"id":"c1","function":{"name":"bash","arguments":"{}"}}],
                    "reasoning_content": "Step 1: analyze..."
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens":10,"completion_tokens":20,"total_tokens":30}
        }"#;
        let parsed: DeepSeekResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            parsed.choices[0].message.reasoning_content.as_deref(),
            Some("Step 1: analyze...")
        );
        assert_eq!(parsed.choices[0].finish_reason, "tool_calls");
    }

    #[test]
    fn finish_reason_mapping() {
        assert_eq!(map_finish_reason("stop"), FinishReason::Stop);
        assert_eq!(map_finish_reason("tool_calls"), FinishReason::ToolCalls);
        assert_eq!(map_finish_reason("length"), FinishReason::MaxTokens);
        assert_eq!(map_finish_reason("unknown"), FinishReason::Error);
    }
}
