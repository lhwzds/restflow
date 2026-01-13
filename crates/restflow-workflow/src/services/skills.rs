//! Skills service layer for business logic.

use crate::{models::Skill, AppCore};
use anyhow::{Context, Result};
use std::sync::Arc;

/// List all skills
pub async fn list_skills(core: &Arc<AppCore>) -> Result<Vec<Skill>> {
    core.storage.skills.list().context("Failed to list skills")
}

/// Get a skill by ID
pub async fn get_skill(core: &Arc<AppCore>, id: &str) -> Result<Option<Skill>> {
    core.storage
        .skills
        .get(id)
        .with_context(|| format!("Failed to get skill {}", id))
}

/// Create a new skill
pub async fn create_skill(core: &Arc<AppCore>, skill: Skill) -> Result<()> {
    core.storage
        .skills
        .create(&skill)
        .with_context(|| format!("Failed to create skill {}", skill.id))
}

/// Update an existing skill
pub async fn update_skill(core: &Arc<AppCore>, id: &str, skill: &Skill) -> Result<()> {
    core.storage
        .skills
        .update(id, skill)
        .with_context(|| format!("Failed to update skill {}", id))
}

/// Delete a skill
pub async fn delete_skill(core: &Arc<AppCore>, id: &str) -> Result<()> {
    core.storage
        .skills
        .delete(id)
        .with_context(|| format!("Failed to delete skill {}", id))
}

/// Check if a skill exists
pub async fn skill_exists(core: &Arc<AppCore>, id: &str) -> Result<bool> {
    core.storage
        .skills
        .exists(id)
        .with_context(|| format!("Failed to check skill {}", id))
}

/// Export a skill to markdown format
pub fn export_skill_to_markdown(skill: &Skill) -> String {
    skill.to_markdown()
}

/// Import a skill from markdown format
pub fn import_skill_from_markdown(id: &str, markdown: &str) -> Result<Skill> {
    Skill::from_markdown(id, markdown).context("Failed to parse markdown")
}
