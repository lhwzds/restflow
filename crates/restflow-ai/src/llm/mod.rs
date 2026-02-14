//! LLM module - Multi-provider LLM client abstraction

mod anthropic;
mod claude_code;
mod client;
mod codex;
mod factory;
mod gemini_cli;
mod mock_client;
mod openai;
mod opencode;
mod pricing;
mod retry;
mod swappable;

pub use anthropic::AnthropicClient;
pub use claude_code::ClaudeCodeClient;
pub use client::{
    CompletionRequest, CompletionResponse, FinishReason, LlmClient, Message, Role, StreamChunk,
    StreamResult, TokenUsage, ToolCall, ToolCallDelta,
};
pub use codex::CodexClient;
pub use factory::{DefaultLlmClientFactory, LlmClientFactory, LlmProvider, ModelSpec};
pub use gemini_cli::GeminiCliClient;
pub use mock_client::{MockLlmClient, MockStep, MockStepKind};
pub use openai::OpenAIClient;
pub use opencode::OpenCodeClient;
pub use retry::LlmRetryConfig;
pub use swappable::SwappableLlm;
