//! Context compaction utilities for working memory.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::error::Result;
use crate::llm::{CompletionRequest, LlmClient, Message, Role};

pub const COMPACTION_PROMPT: &str = include_str!("templates/compaction_prompt.md");

/// Compaction configuration.
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    /// Token threshold ratio to trigger compaction (default: 0.95).
    /// When context usage exceeds this ratio of model's context window.
    pub threshold_ratio: f32,
    /// Maximum tokens for recent user messages to preserve (default: 20_000).
    pub keep_recent_user_tokens: usize,
    /// Maximum tokens for the generated summary (default: 2_000).
    pub max_summary_tokens: usize,
    /// Whether to auto-compact when threshold is reached.
    pub auto_compact: bool,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            threshold_ratio: 0.95,
            keep_recent_user_tokens: 20_000,
            max_summary_tokens: 2_000,
            auto_compact: true,
        }
    }
}

/// Storage adapter for persisting compaction summaries.
#[async_trait]
pub trait CompactionStorage: Send + Sync {
    async fn add_message(&self, session_id: &str, message: Message) -> Result<String>;
    async fn update_session_summary(&self, session_id: &str, summary_message_id: &str)
        -> Result<()>;
}

/// Events emitted during compaction.
#[derive(Debug, Clone)]
pub enum CompactionEvent {
    Started,
    Completed(CompactionResult),
    Failed(String),
}

/// Categorized messages for compaction.
#[derive(Debug, Default)]
pub struct CategorizedMessages {
    /// System messages (always preserved).
    pub system: Vec<Message>,
    /// User messages (selectively preserved).
    pub user: Vec<Message>,
    /// Assistant messages (included in summary).
    pub assistant: Vec<Message>,
    /// Tool calls and results (included in summary).
    pub tool_interactions: Vec<Message>,
    /// Ordered messages for summary formatting.
    ordered: Vec<Message>,
}

impl CategorizedMessages {
    /// Categorize messages from working memory.
    pub fn from_messages(messages: &[Message]) -> Self {
        let mut result = Self::default();
        for msg in messages {
            result.ordered.push(msg.clone());
            match msg.role {
                Role::System => result.system.push(msg.clone()),
                Role::User => result.user.push(msg.clone()),
                Role::Assistant => result.assistant.push(msg.clone()),
                Role::Tool => result.tool_interactions.push(msg.clone()),
            }
        }
        result
    }

    /// Format conversation for summarization.
    pub fn format_for_summary(&self) -> String {
        let mut output = String::new();
        for msg in &self.ordered {
            match msg.role {
                Role::User => {
                    output.push_str("User: ");
                    output.push_str(&msg.content);
                    output.push_str("\n\n");
                }
                Role::Assistant => {
                    if let Some(tool_calls) = msg.tool_calls.as_ref() {
                        for call in tool_calls {
                            output.push_str("Assistant (tool call): ");
                            output.push_str(&call.name);
                            output.push(' ');
                            output.push_str(&call.arguments.to_string());
                            output.push_str("\n\n");
                        }
                    }
                    if !msg.content.is_empty() {
                        output.push_str("Assistant: ");
                        output.push_str(&msg.content);
                        output.push_str("\n\n");
                    }
                }
                Role::Tool => {
                    output.push_str("Tool result");
                    if let Some(tool_call_id) = msg.tool_call_id.as_ref() {
                        output.push_str(" (id: ");
                        output.push_str(tool_call_id);
                        output.push(')');
                    }
                    output.push_str(": ");
                    output.push_str(&msg.content);
                    output.push_str("\n\n");
                }
                Role::System => {}
            }
        }
        output
    }
}

/// Result of compaction operation.
#[derive(Debug, Clone)]
pub struct CompactionResult {
    /// New message history after compaction.
    pub new_history: Vec<Message>,
    /// Generated summary text.
    pub summary: String,
    /// Number of messages compacted.
    pub compacted_count: usize,
    /// Estimated tokens saved.
    pub tokens_saved: usize,
    /// Summary message ID stored in persistence layer, if available.
    pub summary_message_id: Option<String>,
    /// Estimated tokens before compaction.
    pub tokens_before: usize,
    /// Estimated tokens after compaction.
    pub tokens_after: usize,
}

/// Context compactor for working memory.
pub struct ContextCompactor {
    config: CompactionConfig,
}

impl ContextCompactor {
    pub fn new(config: CompactionConfig) -> Self {
        Self { config }
    }

    pub fn with_default_config() -> Self {
        Self::new(CompactionConfig::default())
    }

    /// Check if compaction is needed.
    pub fn needs_compaction(&self, messages: &[Message], context_window: usize) -> bool {
        let current_tokens = estimate_total_tokens(messages);
        let threshold = (context_window as f32 * self.config.threshold_ratio) as usize;
        current_tokens > threshold
    }

    /// Perform compaction.
    pub async fn compact(
        &self,
        messages: Vec<Message>,
        llm: &dyn LlmClient,
    ) -> Result<CompactionResult> {
        let original_count = messages.len();
        let original_tokens = estimate_total_tokens(&messages);

        let categorized = CategorizedMessages::from_messages(&messages);
        let recent_user =
            select_recent_user_messages(&categorized.user, self.config.keep_recent_user_tokens);
        let conversation = categorized.format_for_summary();

        let summary = generate_summary(llm, &conversation, &self.config).await?;

        let new_history = build_compacted_history(categorized.system, recent_user, &summary);
        let new_history_len = new_history.len();
        let new_tokens = estimate_total_tokens(&new_history);

        Ok(CompactionResult {
            new_history,
            summary,
            compacted_count: original_count.saturating_sub(new_history_len),
            tokens_saved: original_tokens.saturating_sub(new_tokens),
            summary_message_id: None,
            tokens_before: original_tokens,
            tokens_after: new_tokens,
        })
    }

    /// Perform compaction asynchronously and persist the summary.
    pub fn compact_async(
        &self,
        messages: Vec<Message>,
        summarizer: Arc<dyn LlmClient>,
        session_id: String,
        storage: Arc<dyn CompactionStorage>,
        event_tx: Option<mpsc::Sender<CompactionEvent>>,
    ) -> JoinHandle<Result<CompactionResult>> {
        let config = self.config.clone();
        tokio::spawn(async move {
            if let Some(tx) = event_tx.as_ref() {
                let _ = tx.send(CompactionEvent::Started).await;
            }

            let result: Result<CompactionResult> = async {
                let tokens_before = estimate_total_tokens(&messages);
                let categorized = CategorizedMessages::from_messages(&messages);
                let conversation = categorized.format_for_summary();
                let summary = generate_summary(&*summarizer, &conversation, &config).await?;

                let summary_message = build_summary_message(&summary);
                let tokens_after = estimate_message_tokens(&summary_message);
                let summary_message_id =
                    storage.add_message(&session_id, summary_message).await?;
                storage
                    .update_session_summary(&session_id, &summary_message_id)
                    .await?;

                Ok(CompactionResult {
                    new_history: Vec::new(),
                    summary,
                    compacted_count: messages.len(),
                    tokens_saved: tokens_before.saturating_sub(tokens_after),
                    summary_message_id: Some(summary_message_id),
                    tokens_before,
                    tokens_after,
                })
            }
            .await;

            match &result {
                Ok(compaction) => {
                    if let Some(tx) = event_tx.as_ref() {
                        let _ = tx.send(CompactionEvent::Completed(compaction.clone())).await;
                    }
                }
                Err(err) => {
                    if let Some(tx) = event_tx.as_ref() {
                        let _ = tx.send(CompactionEvent::Failed(err.to_string())).await;
                    }
                }
            }

            result
        })
    }
}

/// Select recent user messages within token budget.
pub fn select_recent_user_messages(messages: &[Message], max_tokens: usize) -> Vec<Message> {
    let mut selected = Vec::new();
    let mut remaining_tokens = max_tokens;

    for msg in messages.iter().rev() {
        let msg_tokens = estimate_message_tokens(msg);
        if msg_tokens <= remaining_tokens {
            selected.push(msg.clone());
            remaining_tokens = remaining_tokens.saturating_sub(msg_tokens);
        } else if remaining_tokens > 0 {
            let truncated = truncate_message(msg, remaining_tokens);
            selected.push(truncated);
            break;
        } else {
            break;
        }
    }

    selected.reverse();
    selected
}

/// Generate summary using AI.
pub async fn generate_summary(
    llm: &dyn LlmClient,
    conversation: &str,
    config: &CompactionConfig,
) -> Result<String> {
    let prompt = format!(
        "{}\n\n---\n\n## Conversation to Summarize\n\n{}",
        COMPACTION_PROMPT, conversation
    );

    let request = CompletionRequest::new(vec![Message::user(prompt)])
        .with_max_tokens(config.max_summary_tokens as u32);

    let response = llm.complete(request).await?;
    Ok(response.content.unwrap_or_default())
}

fn build_summary_message(summary: &str) -> Message {
    Message {
        role: Role::Assistant,
        content: format!(
            "[Conversation Summary - Previous context has been compacted]\n\n{}",
            summary
        ),
        tool_calls: None,
        tool_call_id: None,
        name: None,
    }
}

/// Build compacted history.
pub fn build_compacted_history(
    system_messages: Vec<Message>,
    recent_user_messages: Vec<Message>,
    summary: &str,
) -> Vec<Message> {
    let mut history = Vec::new();
    history.extend(system_messages);

    let summary_message = build_summary_message(summary);
    history.push(summary_message);
    history.extend(recent_user_messages);
    history
}

fn estimate_message_tokens(msg: &Message) -> usize {
    let content_len = msg.content.len();
    let tool_call_len = msg
        .tool_calls
        .as_ref()
        .map(|calls| {
            calls
                .iter()
                .map(|c| c.name.len() + c.arguments.to_string().len())
                .sum::<usize>()
        })
        .unwrap_or(0);

    (content_len + tool_call_len) / 4 + 1
}

fn truncate_message(msg: &Message, max_tokens: usize) -> Message {
    let max_chars = max_tokens * 4;
    let content = if msg.content.len() > max_chars {
        let half = max_chars / 2;
        format!(
            "{}...\n[{} tokens truncated]\n...{}",
            &msg.content[..half],
            (msg.content.len() - max_chars) / 4,
            &msg.content[msg.content.len() - half..]
        )
    } else {
        msg.content.clone()
    };

    Message {
        role: msg.role.clone(),
        content,
        tool_calls: msg.tool_calls.clone(),
        tool_call_id: msg.tool_call_id.clone(),
        name: msg.name.clone(),
    }
}

fn estimate_total_tokens(messages: &[Message]) -> usize {
    messages.iter().map(estimate_message_tokens).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{CompletionResponse, FinishReason, TokenUsage};
    use async_trait::async_trait;

    struct MockLlm {
        response: String,
    }

    #[async_trait]
    impl LlmClient for MockLlm {
        fn provider(&self) -> &str {
            "mock"
        }

        fn model(&self) -> &str {
            "mock-model"
        }

        async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
            Ok(CompletionResponse {
                content: Some(self.response.clone()),
                tool_calls: vec![],
                finish_reason: FinishReason::Stop,
                usage: Some(TokenUsage::default()),
            })
        }

        fn complete_stream(&self, _request: CompletionRequest) -> crate::llm::StreamResult {
            unimplemented!("streaming not needed for tests")
        }
    }

    #[test]
    fn test_select_recent_user_messages() {
        let messages = vec![
            Message::user("Short"),
            Message::user("Another short"),
            Message::user("A longer message that should be included"),
        ];

        let selected = select_recent_user_messages(&messages, 100);
        assert_eq!(selected.len(), 3);

        let limited = select_recent_user_messages(&messages, 2);
        assert_eq!(limited.len(), 1);
    }

    #[tokio::test]
    async fn test_compaction_flow() {
        let messages = vec![
            Message::system("System"),
            Message::user("User message"),
            Message::assistant("Assistant response"),
        ];

        let config = CompactionConfig {
            max_summary_tokens: 100,
            ..CompactionConfig::default()
        };
        let compactor = ContextCompactor::new(config);

        let llm = MockLlm {
            response: "Summary".to_string(),
        };

        let result = compactor.compact(messages, &llm).await.unwrap();
        assert!(!result.summary.is_empty());
        assert!(!result.new_history.is_empty());
    }
}
