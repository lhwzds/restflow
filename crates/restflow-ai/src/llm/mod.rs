//! LLM module - Multi-provider LLM client abstraction

mod anthropic;
mod client;
mod openai;

pub use anthropic::AnthropicClient;
pub use client::{
    CompletionRequest, CompletionResponse, FinishReason, LlmClient, Message, Role, TokenUsage,
    ToolCall,
};
pub use openai::OpenAIClient;
