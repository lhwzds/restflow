//! Skill metadata model for file-based skill definitions.

use crate::models::skill::StorageMode;
use crate::models::skill_folder::{SkillGating, SkillReference, SkillScript};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Skill metadata stored in the database for file-based skills.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SkillMeta {
    /// Unique identifier for the skill
    pub id: String,
    /// Display name of the skill
    pub name: String,
    /// Optional description of the skill
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional tags for categorization
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Optional folder path for file-based skills (e.g. "skills/coding-assistant")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder_path: Option<String>,
    /// Optional content hash for change detection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// Optional scripts defined by the skill
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<SkillScript>,
    /// Optional references defined by the skill
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<SkillReference>,
    /// Optional suggested tools for the skill
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggested_tools: Vec<String>,
    /// Optional gating requirements for the skill
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gating: Option<SkillGating>,
    /// Optional version for the skill definition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Optional author for the skill definition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Storage mode for the skill definition
    #[serde(default)]
    pub storage_mode: StorageMode,
    /// Whether the skill is synced between storage modes
    #[serde(default)]
    pub is_synced: bool,
    /// Timestamp when the skill was created (milliseconds since epoch)
    #[ts(type = "number")]
    pub created_at: i64,
    /// Timestamp when the skill was last updated (milliseconds since epoch)
    #[ts(type = "number")]
    pub updated_at: i64,
}

impl SkillMeta {
    /// Create a new skill metadata entry with required fields.
    pub fn new(id: String, name: String) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id,
            name,
            description: None,
            tags: None,
            folder_path: None,
            content_hash: None,
            scripts: Vec::new(),
            references: Vec::new(),
            suggested_tools: Vec::new(),
            gating: None,
            version: None,
            author: None,
            storage_mode: StorageMode::DatabaseOnly,
            is_synced: false,
            created_at: now,
            updated_at: now,
        }
    }

    /// Update mutable metadata fields.
    pub fn touch(&mut self) {
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_meta_new() {
        let meta = SkillMeta::new("skill-001".to_string(), "Skill 001".to_string());

        assert_eq!(meta.id, "skill-001");
        assert_eq!(meta.name, "Skill 001");
        assert_eq!(meta.storage_mode, StorageMode::DatabaseOnly);
        assert!(!meta.is_synced);
    }
}
