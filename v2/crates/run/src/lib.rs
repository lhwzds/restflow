//! # codocia
//!
//! Run owns durable task and run execution concepts.
//!
//! ## Owns
//! - Task
//! - Run
//! - run status
//! - durable execution vocabulary
//!
//! ## Must Not
//! - become a second agent loop
//! - own skill catalog
//! - create a separate team runtime
//!
//! ## Inputs
//! - task definitions
//! - agent execution events
//! - checkpoint state
//! - skill catalog
//!
//! ## Outputs
//! - run status
//! - run history
//! - run artifacts
//! - agent run input
//!
//! ## Depends On
//! - agent
//! - chat
//! - event
//! - skill
//! - store
//!
//! ## Verify
//! - cargo check -p run

use agent::RunInput;
use serde::{Deserialize, Serialize};
use skill::{Catalog, resolve_context};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub id: String,
    pub task_id: String,
    pub status: Status,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pending,
    Running,
    Done,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRequest {
    pub task: Task,
    pub message: String,
    pub assigned_skills: Vec<String>,
}

impl TaskRequest {
    pub fn new(task: Task, message: impl Into<String>) -> Self {
        Self {
            task,
            message: message.into(),
            assigned_skills: Vec::new(),
        }
    }

    pub fn with_assigned_skills(mut self, skills: impl IntoIterator<Item = String>) -> Self {
        self.assigned_skills = skills.into_iter().collect();
        self
    }

    pub fn to_agent_input(&self, catalog: &Catalog) -> RunInput {
        let message = format!("Task: {}\n\n{}", self.task.title, self.message);
        RunInput::new(message).with_skill_context(resolve_context(
            catalog,
            &self.assigned_skills,
            &self.message,
        ))
    }
}

pub fn build_agent_input(catalog: &Catalog, request: &TaskRequest) -> RunInput {
    request.to_agent_input(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;
    use skill::{Skill, Source};

    #[test]
    fn task_request_resolves_skill_context_for_agent() {
        let mut catalog = Catalog::new();
        catalog.insert(
            Skill::new("review", "Review", Source::System)
                .with_description("Review code.")
                .with_content("Report findings first."),
        );
        let task = Task {
            id: "task-1".to_string(),
            title: "Review branch".to_string(),
        };
        let request =
            TaskRequest::new(task, "use @review").with_assigned_skills(["review".to_string()]);

        let input = build_agent_input(&catalog, &request);

        assert!(input.message.starts_with("Task: Review branch"));
        assert_eq!(input.skill_context.assigned.len(), 1);
        assert_eq!(input.skill_context.mentioned.len(), 1);
        assert!(input.skill_context.issues.is_empty());
    }
}
