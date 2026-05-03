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
            .complete(
                CompletionRequest::new(vec![
                    Message::system(REVIEWER_SYSTEM_PROMPT),
                    Message::user(prompt),
                ])
                .with_max_tokens(REVIEWER_MAX_OUTPUT_TOKENS),
            )
            .await?;

        parse_review_response(response.content.as_deref())
    }
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
    use super::*;

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
}
