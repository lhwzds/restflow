//! LLM client trait and types

use async_trait::async_trait;
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
}

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

    /// Complete a chat request
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;
}
