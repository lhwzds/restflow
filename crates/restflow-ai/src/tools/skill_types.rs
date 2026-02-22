//! Skill model types and provider trait.

use serde::{Deserialize, Serialize};

/// Skill info for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
}

/// Skill content for reading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillContent {
    pub id: String,
    pub name: String,
    pub content: String,
}

/// Skill record for create/update operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRecord {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub content: String,
}

/// Skill update payload
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillUpdate {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub tags: Option<Option<Vec<String>>>,
    pub content: Option<String>,
}

/// Provider trait for accessing skills (implemented in restflow-core)
pub trait SkillProvider: Send + Sync {
    /// List all available skills
    fn list_skills(&self) -> Vec<SkillInfo>;
    /// Get skill content by ID
    fn get_skill(&self, id: &str) -> Option<SkillContent>;
    /// Create a new skill
    fn create_skill(&self, skill: SkillRecord) -> Result<SkillRecord, String>;
    /// Update an existing skill
    fn update_skill(&self, id: &str, update: SkillUpdate) -> Result<SkillRecord, String>;
    /// Delete a skill
    fn delete_skill(&self, id: &str) -> Result<bool, String>;
    /// Export a skill to markdown
    fn export_skill(&self, id: &str) -> Result<String, String>;
    /// Import a skill from markdown
    fn import_skill(
        &self,
        id: &str,
        markdown: &str,
        overwrite: bool,
    ) -> Result<SkillRecord, String>;
}
