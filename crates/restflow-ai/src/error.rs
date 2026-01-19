//! Error types for the AI module

use thiserror::Error;

/// AI module error types
#[derive(Error, Debug)]
pub enum AiError {
    #[error("LLM error: {0}")]
    Llm(String),

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

/// Result type alias for AI operations
pub type Result<T> = std::result::Result<T, AiError>;
