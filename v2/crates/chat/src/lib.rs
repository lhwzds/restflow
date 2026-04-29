//! # codocia
//!
//! Chat owns sessions, turns, and message history.
//!
//! ## Owns
//! - Session
//! - Message
//! - Role
//! - chat history composition
//!
//! ## Must Not
//! - own durable background runs
//! - render TUI layout
//! - decide model catalog policy
//!
//! ## Inputs
//! - user messages
//! - assistant events
//! - tool events
//! - skill catalog
//!
//! ## Outputs
//! - session history
//! - message lists
//! - agent run input
//!
//! ## Depends On
//! - agent
//! - event
//! - skill
//! - store
//!
//! ## Verify
//! - cargo check -p chat

use agent::RunInput;
use serde::{Deserialize, Serialize};
use skill::{Catalog, resolve_context};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
    Tool,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub messages: Vec<Message>,
}

impl Session {
    pub fn push(&mut self, message: Message) {
        self.messages.push(message);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnRequest {
    pub message: String,
    pub assigned_skills: Vec<String>,
}

impl TurnRequest {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            assigned_skills: Vec::new(),
        }
    }

    pub fn with_assigned_skills(mut self, skills: impl IntoIterator<Item = String>) -> Self {
        self.assigned_skills = skills.into_iter().collect();
        self
    }

    pub fn to_agent_input(&self, catalog: &Catalog) -> RunInput {
        RunInput::new(self.message.clone()).with_skill_context(resolve_context(
            catalog,
            &self.assigned_skills,
            &self.message,
        ))
    }
}

pub fn build_agent_input(catalog: &Catalog, request: &TurnRequest) -> RunInput {
    request.to_agent_input(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;
    use skill::{Skill, Source};

    #[test]
    fn turn_request_resolves_skill_context_for_agent() {
        let mut catalog = Catalog::new();
        catalog.insert(
            Skill::new("team", "Team", Source::System)
                .with_description("Coordinate subagents.")
                .with_content("Use workers for independent tasks."),
        );
        let request =
            TurnRequest::new("please use @team").with_assigned_skills(["team".to_string()]);

        let input = build_agent_input(&catalog, &request);

        assert_eq!(input.message, "please use @team");
        assert_eq!(input.skill_context.assigned.len(), 1);
        assert_eq!(input.skill_context.mentioned.len(), 1);
        assert!(input.skill_context.issues.is_empty());
    }
}
