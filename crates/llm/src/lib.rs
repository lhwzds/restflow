//! LLM module - Multi-provider LLM client abstraction.

pub mod cli;
mod client;
pub mod error {
    use thiserror::Error;

    #[derive(Error, Debug)]
    pub enum AiError {
        #[error("LLM error: {0}")]
        Llm(String),

        #[error("{provider} API error ({status}): {message}")]
        LlmHttp {
            provider: String,
            status: u16,
            message: String,
            retry_after_secs: Option<u64>,
        },

        #[error("Invalid response format: {0}")]
        InvalidFormat(String),

        #[error("HTTP error: {0}")]
        Http(#[from] reqwest::Error),

        #[error("JSON error: {0}")]
        Json(#[from] serde_json::Error),

        #[error("IO error: {0}")]
        Io(#[from] std::io::Error),
    }

    impl AiError {
        pub fn is_retryable(&self) -> bool {
            match self {
                Self::LlmHttp { status, .. } => matches!(status, 429 | 500 | 502 | 503 | 504),
                Self::Http(err) => err.is_timeout() || err.is_connect(),
                Self::Llm(message) => {
                    let lower = message.to_lowercase();
                    lower.contains("timeout")
                        || lower.contains("rate limit")
                        || lower.contains("429")
                        || lower.contains("503")
                        || lower.contains("usage limit")
                        || lower.contains("quota")
                        || lower.contains("rollout")
                        || lower.contains("state db")
                }
                _ => false,
            }
        }

        pub fn retry_after(&self) -> Option<u64> {
            match self {
                Self::LlmHttp {
                    retry_after_secs, ..
                } => *retry_after_secs,
                _ => None,
            }
        }
    }

    pub type Result<T> = std::result::Result<T, AiError>;
}
mod factory;
pub mod http;
#[cfg(any(test, feature = "test-utils"))]
mod mock_client;
pub mod pricing;
mod retry;
mod swappable;
mod switcher;

pub use cli::{CodexClient, GeminiCliClient, OpenCodeClient};
pub use client::{
    CompletionRequest, CompletionResponse, FinishReason, LlmClient, Message, Role, StreamChunk,
    StreamResult, TokenUsage, ToolCall, ToolCallDelta,
};
pub use error::{AiError, Result};
pub use factory::{DefaultLlmClientFactory, LlmClientFactory};
pub use http::{AnthropicClient, DeepSeekClient, OpenAIClient};
#[cfg(any(test, feature = "test-utils"))]
pub use mock_client::{MockLlmClient, MockStep, MockStepKind};
pub use retry::{LlmRetryConfig, RetryingLlmClient};
pub use swappable::SwappableLlm;
pub use switcher::LlmSwitcherImpl;
pub use types::{ClientKind, LlmProvider, ModelSpec};
