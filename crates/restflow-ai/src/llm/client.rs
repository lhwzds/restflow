//! LLM client trait and types

use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::Result;
use crate::tools::ToolSchema;

/// Chat message role
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// Chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Tool calls made by the assistant (for assistant messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl Message {
    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }
    }

    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }
    }

    /// Create an assistant message with tool calls
    pub fn assistant_with_tool_calls(content: Option<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.unwrap_or_default(),
            tool_call_id: None,
            name: None,
            tool_calls: Some(tool_calls),
        }
    }

    /// Create a tool result message
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            tool_call_id: Some(tool_call_id.into()),
            name: None,
            tool_calls: None,
        }
    }
}

/// Tool call request from LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// LLM completion response
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: FinishReason,
    pub usage: Option<TokenUsage>,
}

/// Reason for completion
#[derive(Debug, Clone, PartialEq)]
pub enum FinishReason {
    Stop,
    ToolCalls,
    MaxTokens,
    Error,
}

/// Token usage statistics
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub cost_usd: Option<f64>,
}

/// A chunk of streamed response
#[derive(Debug, Clone)]
pub struct StreamChunk {
    /// Text content in this chunk
    pub text: String,
    /// Thinking/reasoning content (for extended thinking models)
    pub thinking: Option<String>,
    /// Tool call being built incrementally
    pub tool_call_delta: Option<ToolCallDelta>,
    /// Finish reason (set on final chunk)
    pub finish_reason: Option<FinishReason>,
    /// Usage statistics (typically on final chunk)
    pub usage: Option<TokenUsage>,
}

impl StreamChunk {
    /// Create a text chunk
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            thinking: None,
            tool_call_delta: None,
            finish_reason: None,
            usage: None,
        }
    }

    /// Create a thinking chunk
    pub fn thinking(content: impl Into<String>) -> Self {
        Self {
            text: String::new(),
            thinking: Some(content.into()),
            tool_call_delta: None,
            finish_reason: None,
            usage: None,
        }
    }

    /// Create a final chunk with usage
    pub fn final_chunk(finish_reason: FinishReason, usage: Option<TokenUsage>) -> Self {
        Self {
            text: String::new(),
            thinking: None,
            tool_call_delta: None,
            finish_reason: Some(finish_reason),
            usage,
        }
    }

    /// Check if this is an empty chunk
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
            && self.thinking.is_none()
            && self.tool_call_delta.is_none()
            && self.finish_reason.is_none()
    }
}

/// Delta for incremental tool call building
#[derive(Debug, Clone)]
pub struct ToolCallDelta {
    /// Tool call index
    pub index: usize,
    /// Tool call ID (may be partial)
    pub id: Option<String>,
    /// Tool name (may be partial)
    pub name: Option<String>,
    /// Arguments JSON fragment
    pub arguments: Option<String>,
}

/// Type alias for boxed stream of chunks
pub type StreamResult = Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>;

/// LLM completion request
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSchema>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

impl CompletionRequest {
    /// Create a new completion request
    pub fn new(messages: Vec<Message>) -> Self {
        Self {
            messages,
            tools: vec![],
            temperature: None,
            max_tokens: None,
        }
    }

    /// Add tools to the request
    pub fn with_tools(mut self, tools: Vec<ToolSchema>) -> Self {
        self.tools = tools;
        self
    }

    /// Set temperature
    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }

    /// Set max tokens
    pub fn with_max_tokens(mut self, tokens: u32) -> Self {
        self.max_tokens = Some(tokens);
        self
    }
}

/// LLM client trait
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// Get provider name
    fn provider(&self) -> &str;

    /// Get model name
    fn model(&self) -> &str;

    /// Complete a chat request (non-streaming)
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;

    /// Complete a chat request with streaming response
    ///
    /// Returns a stream of chunks that can be processed as they arrive.
    /// The final chunk will contain the finish_reason and usage statistics.
    fn complete_stream(&self, request: CompletionRequest) -> StreamResult;

    /// Check if this client supports streaming
    fn supports_streaming(&self) -> bool {
        true
    }
}
