//! LLM module - Multi-provider LLM client abstraction.

pub mod cli;
mod client;
mod factory;
pub mod http;
#[cfg(any(test, feature = "test-utils"))]
mod mock_client;
pub mod pricing;
mod retry;
mod swappable;
mod switcher;

pub use cli::{ClaudeCodeClient, CodexClient, GeminiCliClient, OpenCodeClient};
pub use client::{
    CompletionRequest, CompletionResponse, FinishReason, LlmClient, Message, Role, StreamChunk,
    StreamResult, TokenUsage, ToolCall, ToolCallDelta,
};
pub use factory::{DefaultLlmClientFactory, LlmClientFactory};
pub use http::{AnthropicClient, OpenAIClient};
#[cfg(any(test, feature = "test-utils"))]
pub use mock_client::{MockLlmClient, MockStep, MockStepKind};
pub use restflow_models::{LlmProvider, ModelSpec};
pub use retry::{LlmRetryConfig, RetryingLlmClient};
pub use swappable::SwappableLlm;
pub use switcher::LlmSwitcherImpl;
