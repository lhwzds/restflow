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

/// Result type alias for AI operations
pub type Result<T> = std::result::Result<T, AiError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_errors_retryable() {
        let codex_err = AiError::Llm(
            "Codex CLI error: state db missing rollout path for thread 019c5096".to_string(),
        );
        assert!(codex_err.is_retryable());

        let usage_err = AiError::Llm("Usage limit exceeded".to_string());
        assert!(usage_err.is_retryable());

        let quota_err = AiError::Llm("API quota exhausted".to_string());
        assert!(quota_err.is_retryable());
    }

    #[test]
    fn test_non_retryable_errors() {
        let auth_err = AiError::Llm("Authentication failed".to_string());
        assert!(!auth_err.is_retryable());

        let tool_err = AiError::ToolNotFound("bash".to_string());
        assert!(!tool_err.is_retryable());

        let format_err = AiError::InvalidFormat("bad json".to_string());
        assert!(!format_err.is_retryable());
    }

    #[test]
    fn test_http_status_retryable() {
        for status in [429, 500, 502, 503, 504] {
            let err = AiError::LlmHttp {
                provider: "test".to_string(),
                status,
                message: "error".to_string(),
                retry_after_secs: None,
            };
            assert!(err.is_retryable(), "status {} should be retryable", status);
        }

        for status in [400, 401, 403, 404, 422] {
            let err = AiError::LlmHttp {
                provider: "test".to_string(),
                status,
                message: "error".to_string(),
                retry_after_secs: None,
            };
            assert!(
                !err.is_retryable(),
                "status {} should not be retryable",
                status
            );
        }
    }
}
