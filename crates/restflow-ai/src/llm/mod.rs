//! LLM module - Multi-provider LLM client abstraction

mod anthropic;
mod claude_code;
mod client;
mod openai;

pub use anthropic::AnthropicClient;
pub use claude_code::ClaudeCodeClient;
pub use client::{
    CompletionRequest, CompletionResponse, FinishReason, LlmClient, Message, Role, StreamChunk,
    StreamResult, TokenUsage, ToolCall,
};
pub use openai::OpenAIClient;
