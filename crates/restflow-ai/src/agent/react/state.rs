//! Agent state machine for ReAct loop.

use crate::llm::Message;

/// Current state of the agent execution
#[derive(Debug, Clone)]
pub enum AgentState {
    Ready,
    Thinking,
    Acting { tool: String },
    Observing,
    Completed { output: String },
    Failed { error: String },
}

/// Conversation history for agent
#[derive(Debug, Clone, Default)]
pub struct ConversationHistory {
    messages: Vec<Message>,
    max_messages: usize,
}

impl ConversationHistory {
    pub fn new(max_messages: usize) -> Self {
        Self {
            messages: Vec::new(),
            max_messages,
        }
    }

    pub fn add(&mut self, message: Message) {
        self.messages.push(message);
        self.trim_if_needed();
    }

    /// Prepend a message at the beginning (used for system prompt).
    pub fn prepend(&mut self, message: Message) {
        self.messages.insert(0, message);
        self.trim_if_needed();
    }

    fn trim_if_needed(&mut self) {
        if self.messages.len() > self.max_messages {
            let system = self.messages.first().cloned();
            let keep_from = self.messages.len() - self.max_messages + 1;
            self.messages = self.messages[keep_from..].to_vec();
            if let Some(sys) = system {
                self.messages.insert(0, sys);
            }
        }
    }

    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    pub fn into_messages(self) -> Vec<Message> {
        self.messages
    }
}
