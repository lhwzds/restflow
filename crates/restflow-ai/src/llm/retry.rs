use std::time::Duration;

use reqwest::Response;

use crate::error::AiError;

#[derive(Debug, Clone)]
pub struct LlmRetryConfig {
    pub max_retries: u32,
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub backoff_multiplier: f64,
}

impl Default for LlmRetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 200,
            max_delay_ms: 5_000,
            backoff_multiplier: 2.0,
        }
    }
}

impl LlmRetryConfig {
    pub fn delay_for(&self, attempt: u32, retry_after_secs: Option<u64>) -> Duration {
        if let Some(seconds) = retry_after_secs {
            return Duration::from_secs(seconds);
        }

        let multiplier = self
            .backoff_multiplier
            .powi(attempt.saturating_sub(1) as i32);
        let delay = (self.initial_delay_ms as f64 * multiplier) as u64;
        Duration::from_millis(delay.min(self.max_delay_ms))
    }
}

pub fn parse_retry_after(response: &Response) -> Option<u64> {
    response
        .headers()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
}

pub async fn response_to_error(response: Response, provider: &str) -> AiError {
    let status = response.status().as_u16();
    let retry_after = parse_retry_after(&response);
    let body = response.text().await.unwrap_or_default();

    // Truncate error body to prevent leaking large or sensitive responses.
    const MAX_ERROR_BODY: usize = 512;
    let message = if body.len() > MAX_ERROR_BODY {
        format!("{}... [truncated]", &body[..MAX_ERROR_BODY])
    } else {
        body
    };

    AiError::LlmHttp {
        provider: provider.to_string(),
        status,
        message,
        retry_after_secs: retry_after,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delay_progression() {
        let config = LlmRetryConfig::default();
        assert_eq!(config.delay_for(1, None), Duration::from_millis(200));
        assert_eq!(config.delay_for(2, None), Duration::from_millis(400));
        assert_eq!(config.delay_for(3, None), Duration::from_millis(800));
        assert_eq!(config.delay_for(4, None), Duration::from_millis(1600));
        assert_eq!(config.delay_for(5, None), Duration::from_millis(3200));
        assert_eq!(config.delay_for(6, None), Duration::from_millis(5000));
    }

    #[test]
    fn test_retry_after_overrides_backoff() {
        let config = LlmRetryConfig::default();
        assert_eq!(config.delay_for(3, Some(10)), Duration::from_secs(10));
    }

    #[test]
    fn test_ai_error_is_retryable() {
        let retryable = AiError::LlmHttp {
            provider: "Test".to_string(),
            status: 429,
            message: "rate limit".to_string(),
            retry_after_secs: None,
        };
        let non_retryable = AiError::LlmHttp {
            provider: "Test".to_string(),
            status: 401,
            message: "unauthorized".to_string(),
            retry_after_secs: None,
        };
        assert!(retryable.is_retryable());
        assert!(!non_retryable.is_retryable());
    }

    #[test]
    fn test_ai_error_llm_string_fallback() {
        let retryable = AiError::Llm("rate limit".to_string());
        let non_retryable = AiError::Llm("bad request".to_string());
        assert!(retryable.is_retryable());
        assert!(!non_retryable.is_retryable());
    }
}
