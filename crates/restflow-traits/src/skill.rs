//! Skill model types and provider trait.

use serde::{Deserialize, Deserializer, Serialize};

/// Skill source used by the unified skill catalog.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum SkillSource {
    /// Shipped with RestFlow and read-only.
    System,
    /// Created or imported by the user.
    #[default]
    User,
    /// Installed from a remote/package/binary source.
    External,
}

impl<'de> Deserialize<'de> for SkillSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let raw_source = match value {
            serde_json::Value::String(source) => source,
            serde_json::Value::Object(mut object) => object
                .remove("type")
                .and_then(|value| value.as_str().map(str::to_string))
                .ok_or_else(|| serde::de::Error::custom("skill source object missing type"))?,
            _ => {
                return Err(serde::de::Error::custom(
                    "skill source must be a string or object",
                ));
            }
        };

        match raw_source.as_str() {
            "system" | "builtin" => Ok(Self::System),
            "user" | "local" => Ok(Self::User),
            "external" | "marketplace" | "github" | "git_hub" | "git" => Ok(Self::External),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &[
                    "system",
                    "user",
                    "external",
                    "builtin",
                    "local",
                    "marketplace",
                    "github",
                    "git_hub",
                    "git",
                ],
            )),
        }
    }
}

impl std::fmt::Display for SkillSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::System => "system",
            Self::User => "user",
            Self::External => "external",
        };
        f.write_str(value)
    }
}

/// Skill info for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub source: SkillSource,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
}

/// Skill content for reading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillContent {
    pub id: String,
    pub name: String,
    pub content: String,
    #[serde(default)]
    pub source: SkillSource,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
}

/// Skill record for create/update operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRecord {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub content: String,
    #[serde(default)]
    pub source: SkillSource,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_skill_source_serializes_as_system() {
        let value = serde_json::to_value(SkillSource::System).unwrap();
        assert_eq!(value, serde_json::json!("system"));
    }

    #[test]
    fn legacy_skill_source_shapes_deserialize() {
        let local: SkillSource = serde_json::from_str(r#"{"type":"local"}"#).unwrap();
        let builtin: SkillSource = serde_json::from_str(r#"{"type":"builtin"}"#).unwrap();
        let github: SkillSource = serde_json::from_str(r#"{"type":"git_hub"}"#).unwrap();

        assert_eq!(local, SkillSource::User);
        assert_eq!(builtin, SkillSource::System);
        assert_eq!(github, SkillSource::External);
    }
}
