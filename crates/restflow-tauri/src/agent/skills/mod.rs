//! Skill loading and injection system for the unified agent.

mod loader;

pub use loader::SkillLoader;

/// Processed skill ready for prompt injection.
#[derive(Debug, Clone)]
pub struct ProcessedSkill {
    pub name: String,
    pub content: String,
    pub variables: Vec<(String, String)>,
}

impl ProcessedSkill {
    /// Format the skill for system prompt injection.
    pub fn format_for_prompt(&self) -> String {
        format!("## Skill: {}\n\n{}", self.name, self.content)
    }
}
