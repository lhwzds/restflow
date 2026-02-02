//! Skill loading and injection system.

mod loader;

pub use loader::SkillLoader;

/// Processed skill ready for injection
#[derive(Debug, Clone)]
pub struct ProcessedSkill {
    pub name: String,
    pub content: String,
    pub variables: Vec<(String, String)>,
}

impl ProcessedSkill {
    /// Format skill for system prompt injection
    pub fn format_for_prompt(&self) -> String {
        format!("## Skill: {}\n\n{}", self.name, self.content)
    }
}
