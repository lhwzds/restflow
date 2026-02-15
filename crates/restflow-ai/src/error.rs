//! Error types for the AI module

use thiserror::Error;

/// AI module error types
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

    #[error("Tool error: {0}")]
    Tool(String),

    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Agent error: {0}")]
    Agent(String),

    #[error("Max iterations reached: {0}")]
    MaxIterations(usize),

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

/// Result type alias for AI operations
pub type Result<T> = std::result::Result<T, AiError>;
