use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Response;

use crate::error::AiError;
use crate::llm::client::{CompletionRequest, CompletionResponse, LlmClient, StreamResult};

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
        // Find safe character boundary to avoid panic on multi-byte UTF-8
        let truncate_at = body
            .char_indices()
            .take_while(|(idx, _)| *idx < MAX_ERROR_BODY)
            .last()
            .map(|(idx, c)| idx + c.len_utf8())
            .unwrap_or(0);
        format!("{}... [truncated]", &body[..truncate_at])
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

/// Decorator that adds retry logic around any `LlmClient`.
///
/// Wraps a `complete()` call with exponential backoff and retryable-error
/// detection. `complete_stream()` retries only when failure happens before
/// any chunk is received.
pub struct RetryingLlmClient {
    inner: Arc<dyn LlmClient>,
    config: LlmRetryConfig,
}

impl RetryingLlmClient {
    pub fn new(inner: Arc<dyn LlmClient>, config: LlmRetryConfig) -> Self {
        Self { inner, config }
    }

    pub fn with_default_config(inner: Arc<dyn LlmClient>) -> Self {
        Self::new(inner, LlmRetryConfig::default())
    }
}

#[async_trait]
impl LlmClient for RetryingLlmClient {
    fn provider(&self) -> &str {
        self.inner.provider()
    }

    fn model(&self) -> &str {
        self.inner.model()
    }

    fn supports_streaming(&self) -> bool {
        self.inner.supports_streaming()
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> crate::error::Result<CompletionResponse> {
        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            let req = request.clone();
            match self.inner.complete(req).await {
                Ok(response) => return Ok(response),
                Err(error) => {
                    if !error.is_retryable() || attempt == self.config.max_retries {
                        return Err(error);
                    }
                    let delay = self.config.delay_for(attempt + 1, error.retry_after());
                    tracing::warn!(
                        provider = self.inner.provider(),
                        model = self.inner.model(),
                        attempt = attempt + 1,
                        delay_ms = delay.as_millis() as u64,
                        error = %error,
                        "Retrying LLM request"
                    );
                    tokio::time::sleep(delay).await;
                    last_error = Some(error);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            AiError::Llm(format!(
                "{}/{} request failed after retries",
                self.inner.provider(),
                self.inner.model()
            ))
        }))
    }

    fn complete_stream(&self, request: CompletionRequest) -> StreamResult {
        let inner = Arc::clone(&self.inner);
        let config = self.config.clone();

        Box::pin(async_stream::stream! {
            let mut retry_attempts = 0u32;

            'retry_loop: loop {
                let mut saw_any_chunk = false;
                let mut stream = inner.complete_stream(request.clone());

                while let Some(item) = stream.next().await {
                    match item {
                        Ok(chunk) => {
                            saw_any_chunk = true;
                            yield Ok(chunk);
                        }
                        Err(error) => {
                            let can_retry = !saw_any_chunk
                                && error.is_retryable()
                                && retry_attempts < config.max_retries;

                            if can_retry {
                                retry_attempts += 1;
                                let delay = config.delay_for(retry_attempts, error.retry_after());
                                tracing::warn!(
                                    provider = inner.provider(),
                                    model = inner.model(),
                                    attempt = retry_attempts,
                                    delay_ms = delay.as_millis() as u64,
                                    error = %error,
                                    "Retrying LLM streaming request before first chunk"
                                );
                                tokio::time::sleep(delay).await;
                                continue 'retry_loop;
                            }

                            yield Err(error);
                            return;
                        }
                    }
                }

                return;
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Result;
    use crate::llm::client::{FinishReason, Message, StreamChunk, TokenUsage};
    use futures::stream;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockRetryClient {
        stream_calls: AtomicUsize,
        stream_results: Mutex<Vec<Vec<Result<StreamChunk>>>>,
    }

    impl MockRetryClient {
        fn new(stream_results: Vec<Vec<Result<StreamChunk>>>) -> Self {
            Self {
                stream_calls: AtomicUsize::new(0),
                stream_results: Mutex::new(stream_results.into_iter().rev().collect()),
            }
        }

        fn stream_call_count(&self) -> usize {
            self.stream_calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl LlmClient for MockRetryClient {
        fn provider(&self) -> &str {
            "mock"
        }

        fn model(&self) -> &str {
            "mock-model"
        }

        async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
            Ok(CompletionResponse {
                content: Some("ok".to_string()),
                tool_calls: vec![],
                finish_reason: FinishReason::Stop,
                usage: Some(TokenUsage {
                    prompt_tokens: 1,
                    completion_tokens: 1,
                    total_tokens: 2,
                    cost_usd: None,
                }),
            })
        }

        fn complete_stream(&self, _request: CompletionRequest) -> StreamResult {
            self.stream_calls.fetch_add(1, Ordering::SeqCst);
            let next = self
                .stream_results
                .lock()
                .unwrap()
                .pop()
                .unwrap_or_default();
            Box::pin(stream::iter(next))
        }
    }

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

    #[tokio::test]
    async fn test_complete_stream_retries_before_first_chunk() {
        let client = Arc::new(MockRetryClient::new(vec![
            vec![Err(AiError::Llm("timeout while connecting".to_string()))],
            vec![Ok(StreamChunk::text("hello"))],
        ]));
        let config = LlmRetryConfig {
            max_retries: 1,
            initial_delay_ms: 0,
            max_delay_ms: 0,
            backoff_multiplier: 1.0,
        };
        let retrying = RetryingLlmClient::new(client.clone(), config);
        let request = CompletionRequest::new(vec![Message::user("ping")]);

        let mut stream = retrying.complete_stream(request);
        let first = stream.next().await.expect("first stream item").expect("chunk");
        assert_eq!(first.text, "hello");
        assert!(stream.next().await.is_none());
        assert_eq!(client.stream_call_count(), 2);
    }

    #[tokio::test]
    async fn test_complete_stream_does_not_retry_after_first_chunk() {
        let client = Arc::new(MockRetryClient::new(vec![vec![
            Ok(StreamChunk::text("partial")),
            Err(AiError::Llm("timeout while reading stream".to_string())),
        ]]));
        let config = LlmRetryConfig {
            max_retries: 3,
            initial_delay_ms: 0,
            max_delay_ms: 0,
            backoff_multiplier: 1.0,
        };
        let retrying = RetryingLlmClient::new(client.clone(), config);
        let request = CompletionRequest::new(vec![Message::user("ping")]);

        let mut stream = retrying.complete_stream(request);
        let first = stream.next().await.expect("first stream item").expect("chunk");
        assert_eq!(first.text, "partial");

        let second = stream.next().await.expect("second stream item");
        assert!(second.is_err());
        assert_eq!(client.stream_call_count(), 1);
    }
}
