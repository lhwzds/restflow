//! Error types for the tools module.

use thiserror::Error;

/// Tool-specific error types.
#[derive(Error, Debug)]
pub enum ToolError {
    #[error("tool error: {0}")]
    Tool(String),

    #[error("execution failed: {0}")]
    Execution(#[from] std::io::Error),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("security blocked: {0}")]
    SecurityBlocked(String),

    #[error("tool not found: {0}")]
    NotFound(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

/// Result type alias for tool operations.
pub type Result<T> = std::result::Result<T, ToolError>;
