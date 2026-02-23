//! LLM module - Multi-provider LLM client abstraction

mod anthropic;
mod claude_code;
pub(crate) mod cli_utils;
mod client;
mod codex;
mod factory;
mod gemini_cli;
#[cfg(any(test, feature = "test-utils"))]
mod mock_client;
mod openai;
mod opencode;
mod pricing;
mod retry;
mod swappable;
mod switcher;

pub use anthropic::AnthropicClient;
pub use claude_code::ClaudeCodeClient;
pub use client::{
    CompletionRequest, CompletionResponse, FinishReason, LlmClient, Message, Role, StreamChunk,
    StreamResult, TokenUsage, ToolCall, ToolCallDelta,
};
pub use codex::CodexClient;
pub use factory::{DefaultLlmClientFactory, LlmClientFactory, LlmProvider, ModelSpec};
pub use gemini_cli::GeminiCliClient;
#[cfg(any(test, feature = "test-utils"))]
pub use mock_client::{MockLlmClient, MockStep, MockStepKind};
pub use openai::OpenAIClient;
pub use opencode::OpenCodeClient;
pub use retry::{LlmRetryConfig, RetryingLlmClient};
pub use swappable::SwappableLlm;
pub use switcher::LlmSwitcherImpl;
