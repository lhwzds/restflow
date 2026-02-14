//! Conversation history processors applied before LLM calls.

use std::collections::BTreeSet;
use std::fmt;
use std::sync::Arc;

use crate::llm::{Message, Role};

/// Processor hook for transforming conversation history before each LLM request.
pub trait HistoryProcessor: Send + Sync {
    /// Stable processor name for diagnostics.
    fn name(&self) -> &'static str;

    /// Transform outgoing messages.
    fn process(&self, messages: &[Message]) -> Vec<Message>;
}

/// Ordered chain of history processors.
#[derive(Clone, Default)]
pub struct HistoryPipeline {
    processors: Vec<Arc<dyn HistoryProcessor>>,
}

impl HistoryPipeline {
    /// Create an empty history pipeline.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register one processor at the end of the chain.
    pub fn push(mut self, processor: Arc<dyn HistoryProcessor>) -> Self {
        self.processors.push(processor);
        self
    }

    /// Register one processor at the end of the chain.
    pub fn add(&mut self, processor: Arc<dyn HistoryProcessor>) {
        self.processors.push(processor);
    }

    /// Get current processor count.
    pub fn len(&self) -> usize {
        self.processors.len()
    }

    /// Check whether the pipeline is empty.
    pub fn is_empty(&self) -> bool {
        self.processors.is_empty()
    }

    /// Apply the full chain in order.
    pub fn apply(&self, input: Vec<Message>) -> Vec<Message> {
        if self.processors.is_empty() {
            return input;
        }

        self.processors.iter().fold(input, |messages, processor| {
            let processed = processor.process(&messages);
            if processed.is_empty() {
                messages
            } else {
                processed
            }
        })
    }
}

impl fmt::Debug for HistoryPipeline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&'static str> = self.processors.iter().map(|p| p.name()).collect();
        f.debug_struct("HistoryPipeline")
            .field("processors", &names)
            .finish()
    }
}

/// Keep the latest N non-preserved messages and collapse older entries.
#[derive(Debug, Clone)]
pub struct TrimOldMessagesProcessor {
    keep_recent: usize,
    trim_notice: String,
}

impl TrimOldMessagesProcessor {
    pub const DEFAULT_TRIM_NOTICE: &str = "[Earlier conversation trimmed]";

    pub fn new(keep_recent: usize) -> Self {
        Self {
            keep_recent,
            trim_notice: Self::DEFAULT_TRIM_NOTICE.to_string(),
        }
    }

    pub fn with_trim_notice(mut self, trim_notice: impl Into<String>) -> Self {
        self.trim_notice = trim_notice.into();
        self
    }
}

impl HistoryProcessor for TrimOldMessagesProcessor {
    fn name(&self) -> &'static str {
        "trim_old_messages"
    }

    fn process(&self, messages: &[Message]) -> Vec<Message> {
        if messages.is_empty() {
            return Vec::new();
        }

        let mut preserved = BTreeSet::new();
        if let Some(system_idx) = messages.iter().position(|m| m.role == Role::System) {
            preserved.insert(system_idx);
        }
        if let Some(user_idx) = messages.iter().position(|m| m.role == Role::User) {
            preserved.insert(user_idx);
        }

        let remaining: Vec<usize> = (0..messages.len())
            .filter(|idx| !preserved.contains(idx))
            .collect();

        if remaining.len() <= self.keep_recent {
            return messages.to_vec();
        }

        let tail_start = remaining.len() - self.keep_recent;
        let tail: BTreeSet<usize> = remaining[tail_start..].iter().copied().collect();
        let removed_count = remaining.len() - tail.len();
        let first_tail = tail.iter().next().copied();

        let mut output = Vec::with_capacity(preserved.len() + tail.len() + 1);
        let mut inserted_notice = false;

        for (idx, message) in messages.iter().enumerate() {
            let keep = preserved.contains(&idx) || tail.contains(&idx);
            if !keep {
                continue;
            }

            if !inserted_notice
                && removed_count > 0
                && !self.trim_notice.is_empty()
                && first_tail.is_some_and(|first| idx == first)
            {
                output.push(Message::system(self.trim_notice.clone()));
                inserted_notice = true;
            }

            output.push(message.clone());
        }

        if !inserted_notice && removed_count > 0 && !self.trim_notice.is_empty() {
            output.push(Message::system(self.trim_notice.clone()));
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_processor_preserves_first_system_and_user() {
        let messages = vec![
            Message::system("system"),
            Message::user("first-user"),
            Message::assistant("a1"),
            Message::tool_result("call_1", "t1"),
            Message::assistant("a2"),
            Message::tool_result("call_2", "t2"),
        ];

        let processor = TrimOldMessagesProcessor::new(2);
        let processed = processor.process(&messages);

        assert_eq!(processed[0].role, Role::System);
        assert_eq!(processed[0].content, "system");
        assert_eq!(processed[1].role, Role::User);
        assert_eq!(processed[1].content, "first-user");
        assert_eq!(processed[2].role, Role::System);
        assert_eq!(processed[2].content, "[Earlier conversation trimmed]");
        assert_eq!(processed[3].content, "a2");
        assert_eq!(processed[4].content, "t2");
        assert_eq!(processed.len(), 5);
    }

    #[test]
    fn trim_processor_keeps_history_when_within_budget() {
        let messages = vec![
            Message::system("system"),
            Message::user("first-user"),
            Message::assistant("a1"),
        ];

        let processor = TrimOldMessagesProcessor::new(4);
        let processed = processor.process(&messages);
        assert_eq!(processed.len(), messages.len());
    }

    #[test]
    fn pipeline_applies_processors_in_order() {
        let mut pipeline = HistoryPipeline::new();
        pipeline.add(Arc::new(TrimOldMessagesProcessor::new(1)));
        pipeline.add(Arc::new(
            TrimOldMessagesProcessor::new(1).with_trim_notice("[Custom Notice]"),
        ));

        let messages = vec![
            Message::system("system"),
            Message::user("first-user"),
            Message::assistant("a1"),
            Message::assistant("a2"),
        ];

        let processed = pipeline.apply(messages);
        assert_eq!(processed[2].content, "[Custom Notice]");
    }
}
