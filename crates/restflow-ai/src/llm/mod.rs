//! LLM module - Multi-provider LLM client abstraction

mod anthropic;
mod client;
mod openai;

pub use anthropic::AnthropicClient;
pub use client::{
    CompletionRequest, CompletionResponse, FinishReason, LlmClient, Message, Role, StreamChunk,
    StreamResult, TokenUsage, ToolCall, ToolCallDelta,
};
pub use openai::OpenAIClient;
