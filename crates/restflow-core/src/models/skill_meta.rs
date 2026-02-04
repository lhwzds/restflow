use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::models::{SkillGating, SkillReference, SkillScript, StorageMode};

/// Skill metadata stored in the database (content lives on disk).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SkillMeta {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Optional folder path for skills stored on disk
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
    /// Storage mode for the skill
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
