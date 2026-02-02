//! Agent state machine for the ReAct loop.

use restflow_ai::llm::Message;

/// Current state of the agent execution.
#[derive(Debug, Clone)]
pub enum AgentState {
    Ready,
    Thinking,
    Acting { tool: String },
    Observing,
    Completed { output: String },
    Failed { error: String },
}

/// Conversation history for the agent.
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
        self.trim();
    }

    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    pub fn into_messages(self) -> Vec<Message> {
        self.messages
    }

    fn trim(&mut self) {
        if self.max_messages == 0 || self.messages.len() <= self.max_messages {
            return;
        }

        let system = self.messages.first().cloned();
        let keep_from = self.messages.len().saturating_sub(self.max_messages - 1);
        self.messages = self.messages[keep_from..].to_vec();

        if let Some(system_message) = system {
            if !self.messages.is_empty() && self.messages[0].role != restflow_ai::llm::Role::System
            {
                self.messages.insert(0, system_message);
            }
        }
    }
}
