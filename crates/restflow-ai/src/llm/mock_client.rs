//! Deterministic mock LLM client for stress and reliability tests.

use std::collections::VecDeque;
use std::sync::Arc;

use async_stream::try_stream;
use async_trait::async_trait;
use tokio::sync::Mutex;
use tokio::time::{Duration, sleep};

use crate::error::{AiError, Result};

use super::{
    CompletionRequest, CompletionResponse, FinishReason, LlmClient, StreamChunk, StreamResult,
    TokenUsage, ToolCall,
};

/// Deterministic step for scripted mock completions.
#[derive(Debug, Clone)]
pub enum MockStepKind {
    /// Return a plain assistant message.
    Text(String),
    /// Return a tool call response.
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    /// Return an LLM error.
    Error(String),
    /// Return a timeout-like error after optional delay.
    Timeout,
}

/// Scripted completion step with optional delay.
#[derive(Debug, Clone)]
pub struct MockStep {
    pub delay_ms: u64,
    pub kind: MockStepKind,
}

impl MockStep {
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            delay_ms: 0,
            kind: MockStepKind::Text(content.into()),
        }
    }

    pub fn tool_call(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Self {
        Self {
            delay_ms: 0,
            kind: MockStepKind::ToolCall {
                id: id.into(),
                name: name.into(),
                arguments,
            },
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            delay_ms: 0,
            kind: MockStepKind::Error(message.into()),
        }
    }

    pub fn timeout(delay_ms: u64) -> Self {
        Self {
            delay_ms,
            kind: MockStepKind::Timeout,
        }
    }

    pub fn with_delay(mut self, delay_ms: u64) -> Self {
        self.delay_ms = delay_ms;
        self
    }
}

/// A deterministic mock LLM client driven by scripted steps.
#[derive(Debug, Clone, Default)]
pub struct MockLlmClient {
    model: String,
    script: Arc<Mutex<VecDeque<MockStep>>>,
}

impl MockLlmClient {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            script: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub fn from_steps(model: impl Into<String>, steps: Vec<MockStep>) -> Self {
        Self {
            model: model.into(),
            script: Arc::new(Mutex::new(VecDeque::from(steps))),
        }
    }

    pub async fn push_step(&self, step: MockStep) {
        self.script.lock().await.push_back(step);
    }

    async fn next_step(&self) -> Option<MockStep> {
        self.script.lock().await.pop_front()
    }

    fn usage_for(content_len: usize) -> TokenUsage {
        let completion_tokens = content_len as u32;
        TokenUsage {
            prompt_tokens: 1,
            completion_tokens,
            total_tokens: 1 + completion_tokens,
            cost_usd: Some(0.0),
        }
    }

    fn fallback_response(request: &CompletionRequest) -> CompletionResponse {
        let text = request
            .messages
            .iter()
            .rev()
            .find(|msg| matches!(msg.role, super::Role::User))
            .map(|msg| format!("mock-echo: {}", msg.content))
            .unwrap_or_else(|| "mock-ok".to_string());

        CompletionResponse {
            content: Some(text.clone()),
            tool_calls: Vec::new(),
            finish_reason: FinishReason::Stop,
            usage: Some(Self::usage_for(text.len())),
        }
    }
}

#[async_trait]
impl LlmClient for MockLlmClient {
    fn provider(&self) -> &str {
        "mock"
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let step = self.next_step().await;
        let Some(step) = step else {
            return Ok(Self::fallback_response(&request));
        };

        if step.delay_ms > 0 {
            sleep(Duration::from_millis(step.delay_ms)).await;
        }

        match step.kind {
            MockStepKind::Text(content) => Ok(CompletionResponse {
                usage: Some(Self::usage_for(content.len())),
                content: Some(content),
                tool_calls: Vec::new(),
                finish_reason: FinishReason::Stop,
            }),
            MockStepKind::ToolCall {
                id,
                name,
                arguments,
            } => Ok(CompletionResponse {
                usage: Some(Self::usage_for(0)),
                content: None,
                tool_calls: vec![ToolCall {
                    id,
                    name,
                    arguments,
                }],
                finish_reason: FinishReason::ToolCalls,
            }),
            MockStepKind::Error(message) => Err(AiError::Llm(message)),
            MockStepKind::Timeout => Err(AiError::Llm("mock timeout".to_string())),
        }
    }

    fn complete_stream(&self, request: CompletionRequest) -> StreamResult {
        let client = self.clone();
        Box::pin(try_stream! {
            let response = client.complete(request).await?;

            if let Some(content) = response.content
                && !content.is_empty()
            {
                yield StreamChunk::text(content);
            }

            yield StreamChunk::final_chunk(response.finish_reason, response.usage);
        })
    }

    fn supports_streaming(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use futures::TryStreamExt;

    use super::*;
    use crate::llm::{CompletionRequest, Message};

    #[tokio::test]
    async fn mock_client_returns_scripted_text() {
        let client = MockLlmClient::from_steps("mock-model", vec![MockStep::text("hello")]);

        let response = client
            .complete(CompletionRequest::new(vec![Message::user("ping")]))
            .await
            .expect("mock response should succeed");

        assert_eq!(response.content.as_deref(), Some("hello"));
        assert_eq!(response.finish_reason, FinishReason::Stop);
    }

    #[tokio::test]
    async fn mock_client_returns_scripted_tool_call() {
        let client = MockLlmClient::from_steps(
            "mock-model",
            vec![MockStep::tool_call(
                "call-1",
                "search",
                serde_json::json!({"q": "restflow"}),
            )],
        );

        let response = client
            .complete(CompletionRequest::new(vec![Message::user("use tool")]))
            .await
            .expect("tool call response should succeed");

        assert_eq!(response.finish_reason, FinishReason::ToolCalls);
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].name, "search");
    }

    #[tokio::test]
    async fn mock_client_supports_streaming() {
        let client = MockLlmClient::from_steps("mock-model", vec![MockStep::text("stream")]);

        let chunks = client
            .complete_stream(CompletionRequest::new(vec![Message::user("hi")]))
            .try_collect::<Vec<_>>()
            .await
            .expect("stream should succeed");

        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].text, "stream");
        assert!(
            chunks
                .last()
                .and_then(|chunk| chunk.finish_reason.as_ref())
                .is_some()
        );
    }
}
