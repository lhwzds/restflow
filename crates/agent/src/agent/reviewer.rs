use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;

use crate::error::{AiError, Result};
use crate::llm::{CompletionRequest, LlmClient, Message, ToolCall};

const REVIEWER_MAX_ENTRY_CHARS: usize = 8_000;
const REVIEWER_MAX_TRANSCRIPT_CHARS: usize = 40_000;
const REVIEWER_MAX_OUTPUT_TOKENS: u32 = 512;

const REVIEWER_SYSTEM_PROMPT: &str = include_str!("../../prompts/agents/tool_call_reviewer.md");

#[derive(Debug, Clone)]
pub struct ToolReviewRequest {
    pub messages: Vec<Message>,
    pub tool_call: ToolCall,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolReviewDecision {
    Allow,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolReviewOutcome {
    pub decision: ToolReviewDecision,
    pub reason: Option<String>,
}

impl ToolReviewOutcome {
    pub fn allow(reason: Option<String>) -> Self {
        Self {
            decision: ToolReviewDecision::Allow,
            reason,
        }
    }

    pub fn deny(reason: Option<String>) -> Self {
        Self {
            decision: ToolReviewDecision::Deny,
            reason,
        }
    }

    pub fn is_allowed(&self) -> bool {
        self.decision == ToolReviewDecision::Allow
    }
}

#[async_trait]
pub trait ToolCallReviewer: Send + Sync {
    async fn review_tool_call(&self, request: ToolReviewRequest) -> Result<ToolReviewOutcome>;
}

#[derive(Clone)]
pub struct LlmToolCallReviewer {
    llm: Arc<dyn LlmClient>,
}

impl LlmToolCallReviewer {
    pub fn new(llm: Arc<dyn LlmClient>) -> Self {
        Self { llm }
    }
}

#[async_trait]
impl ToolCallReviewer for LlmToolCallReviewer {
    async fn review_tool_call(&self, request: ToolReviewRequest) -> Result<ToolReviewOutcome> {
        let prompt = build_review_prompt(&request);
        let response = self
            .llm
            .complete(build_review_completion_request(prompt.clone()))
            .await?;

        match parse_review_response(response.content.as_deref()) {
            Ok(outcome) => Ok(outcome),
            Err(first_error) => {
                let retry_prompt = build_review_retry_prompt(&prompt, response.content.as_deref());
                let retry = self
                    .llm
                    .complete(build_review_completion_request(retry_prompt))
                    .await?;
                parse_review_response(retry.content.as_deref()).map_err(|retry_error| {
                    AiError::Llm(format!("{first_error}; retry also failed: {retry_error}"))
                })
            }
        }
    }
}

fn build_review_completion_request(prompt: String) -> CompletionRequest {
    CompletionRequest::new(vec![
        Message::system(REVIEWER_SYSTEM_PROMPT),
        Message::user(prompt),
    ])
    .with_temperature(0.0)
    .with_max_tokens(REVIEWER_MAX_OUTPUT_TOKENS)
}

fn build_review_retry_prompt(original_prompt: &str, invalid_response: Option<&str>) -> String {
    let invalid_response = invalid_response
        .map(str::trim)
        .filter(|content| !content.is_empty())
        .unwrap_or("<empty>");
    format!(
        "{original_prompt}\n\nThe previous reviewer response was invalid and was rejected:\n{invalid_response}\n\nReturn exactly one valid JSON object and nothing else."
    )
}

#[derive(Debug, Deserialize)]
struct ReviewResponse {
    decision: String,
    reason: Option<String>,
}

fn parse_review_response(content: Option<&str>) -> Result<ToolReviewOutcome> {
    let content = content
        .map(str::trim)
        .filter(|content| !content.is_empty())
        .ok_or_else(|| AiError::Llm("Reviewer returned an empty response".to_string()))?;

    let json_text = if let (Some(start), Some(end)) = (content.find('{'), content.rfind('}')) {
        &content[start..=end]
    } else {
        content
    };
    let parsed: ReviewResponse = serde_json::from_str(json_text)
        .map_err(|error| AiError::Llm(format!("Reviewer returned invalid JSON: {error}")))?;

    let reason = parsed
        .reason
        .map(|reason| reason.trim().to_string())
        .filter(|reason| !reason.is_empty());
    match parsed.decision.trim().to_ascii_lowercase().as_str() {
        "allow" => Ok(ToolReviewOutcome::allow(reason)),
        "deny" => Ok(ToolReviewOutcome::deny(reason)),
        other => Err(AiError::Llm(format!(
            "Reviewer returned unsupported decision '{other}'"
        ))),
    }
}

fn build_review_prompt(request: &ToolReviewRequest) -> String {
    let transcript = compact_review_transcript(&request.messages);
    let action_json = serde_json::to_string_pretty(&tool_call_json(&request.tool_call))
        .unwrap_or_else(|_| "{}".to_string());
    format!(
        "Review the planned RestFlow tool operation.\n\n>>> TRANSCRIPT START\n{transcript}\n>>> TRANSCRIPT END\n\n>>> PLANNED TOOL CALL START\n{action_json}\n>>> PLANNED TOOL CALL END\n"
    )
}

fn tool_call_json(tool_call: &ToolCall) -> Value {
    serde_json::json!({
        "id": tool_call.id,
        "name": tool_call.name,
        "arguments": tool_call.arguments,
    })
}

fn compact_review_transcript(messages: &[Message]) -> String {
    if messages.is_empty() {
        return "<no retained transcript entries>".to_string();
    }

    let mut selected = Vec::new();
    let mut total_chars = 0usize;

    for (index, message) in messages.iter().enumerate().rev() {
        let rendered = render_transcript_entry(index + 1, message);
        let entry_len = rendered.len();
        if !selected.is_empty() && total_chars + entry_len > REVIEWER_MAX_TRANSCRIPT_CHARS {
            break;
        }
        total_chars += entry_len;
        selected.push(rendered);
    }

    selected.reverse();
    if messages.len() > selected.len() {
        selected.insert(
            0,
            "Some earlier conversation entries were omitted.".to_string(),
        );
    }
    selected.join("\n")
}

fn render_transcript_entry(index: usize, message: &Message) -> String {
    let role = match message.role {
        crate::llm::Role::System => "system",
        crate::llm::Role::User => "user",
        crate::llm::Role::Assistant => "assistant",
        crate::llm::Role::Tool => "tool",
    };
    let mut body = truncate_middle(&message.content, REVIEWER_MAX_ENTRY_CHARS);
    if let Some(tool_calls) = &message.tool_calls {
        let calls = serde_json::to_string(tool_calls).unwrap_or_default();
        if !calls.is_empty() {
            if !body.is_empty() {
                body.push('\n');
            }
            body.push_str("tool_calls: ");
            body.push_str(&truncate_middle(&calls, REVIEWER_MAX_ENTRY_CHARS));
        }
    }
    format!("[{index}] {role}: {body}")
}

fn truncate_middle(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }

    let marker = "<truncated>";
    let available = max_chars.saturating_sub(marker.len());
    let head_chars = available / 2;
    let tail_chars = available.saturating_sub(head_chars);
    let head = value.chars().take(head_chars).collect::<String>();
    let tail = value
        .chars()
        .rev()
        .take(tail_chars)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("{head}{marker}{tail}")
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::llm::{CompletionResponse, FinishReason, StreamResult};

    #[test]
    fn review_response_accepts_json_wrapper() {
        let outcome = parse_review_response(Some(
            "Review complete:\n{\"decision\":\"deny\",\"reason\":\"outside scope\"}",
        ))
        .expect("wrapped JSON should parse");

        assert_eq!(outcome.decision, ToolReviewDecision::Deny);
        assert_eq!(outcome.reason.as_deref(), Some("outside scope"));
    }

    #[test]
    fn compact_transcript_keeps_recent_context() {
        let messages = vec![
            Message::user("first"),
            Message::assistant("middle"),
            Message::tool_result("call-1", "latest tool evidence"),
        ];

        let transcript = compact_review_transcript(&messages);

        assert!(transcript.contains("[1] user: first"));
        assert!(transcript.contains("[3] tool: latest tool evidence"));
    }

    #[tokio::test]
    async fn llm_reviewer_retries_once_after_invalid_json() {
        let llm = Arc::new(SequencedReviewerLlm::new(vec![
            "{\"decision\":\"allow\"".to_string(),
            "{\"decision\":\"allow\",\"reason\":\"safe read\"}".to_string(),
        ]));
        let reviewer = LlmToolCallReviewer::new(llm.clone());

        let outcome = reviewer
            .review_tool_call(ToolReviewRequest {
                messages: vec![Message::user("run pwd")],
                tool_call: ToolCall {
                    id: "call-1".to_string(),
                    name: "bash".to_string(),
                    arguments: serde_json::json!({"command": "pwd"}),
                },
            })
            .await
            .expect("retry should recover valid JSON");

        assert_eq!(outcome.decision, ToolReviewDecision::Allow);
        assert_eq!(outcome.reason.as_deref(), Some("safe read"));
        let prompts = llm.prompts();
        assert_eq!(prompts.len(), 2);
        assert!(prompts[1].contains("previous reviewer response was invalid"));
    }

    struct SequencedReviewerLlm {
        responses: Mutex<Vec<String>>,
        prompts: Mutex<Vec<String>>,
    }

    impl SequencedReviewerLlm {
        fn new(responses: Vec<String>) -> Self {
            Self {
                responses: Mutex::new(responses),
                prompts: Mutex::new(Vec::new()),
            }
        }

        fn prompts(&self) -> Vec<String> {
            self.prompts.lock().expect("prompts lock").clone()
        }
    }

    #[async_trait]
    impl LlmClient for SequencedReviewerLlm {
        fn provider(&self) -> &str {
            "test"
        }

        fn model(&self) -> &str {
            "test-reviewer"
        }

        async fn complete(
            &self,
            request: CompletionRequest,
        ) -> crate::llm::Result<CompletionResponse> {
            if let Some(prompt) = request.messages.last() {
                self.prompts
                    .lock()
                    .expect("prompts lock")
                    .push(prompt.content.clone());
            }
            let content = self.responses.lock().expect("responses lock").remove(0);
            Ok(CompletionResponse {
                content: Some(content),
                tool_calls: Vec::new(),
                finish_reason: FinishReason::Stop,
                usage: None,
                reasoning_content: None,
            })
        }

        fn complete_stream(&self, _request: CompletionRequest) -> StreamResult {
            Box::pin(futures::stream::empty())
        }
    }
}
