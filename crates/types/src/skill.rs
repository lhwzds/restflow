//! Skill model types and provider trait.

use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;

/// Skill source used by the unified skill catalog.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default, Type)]
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

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SkillScript {
    pub id: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SkillReference {
    pub id: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SkillGating {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bins: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os: Option<Vec<String>>,
}

/// Skill lifecycle status used for discovery and planning.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SkillStatus {
    #[default]
    Active,
    Completed,
    Archived,
    Draft,
}

/// A skill represents a reusable AI prompt template.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default)]
    pub executable: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<String>,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder_path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggested_tools: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<SkillScript>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<SkillReference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gating: Option<SkillGating>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    #[serde(default)]
    pub status: SkillStatus,
    #[serde(default)]
    pub auto_complete: bool,
    #[serde(default)]
    pub source: SkillSource,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Skill {
    pub fn new(
        id: String,
        name: String,
        description: Option<String>,
        tags: Option<Vec<String>>,
        content: String,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id,
            name,
            description,
            tags,
            kind: None,
            executable: false,
            triggers: Vec::new(),
            content,
            folder_path: None,
            suggested_tools: Vec::new(),
            scripts: Vec::new(),
            references: Vec::new(),
            gating: None,
            version: None,
            author: None,
            license: None,
            content_hash: None,
            status: SkillStatus::Active,
            auto_complete: false,
            source: SkillSource::User,
            read_only: false,
            source_ref: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn update(
        &mut self,
        name: Option<String>,
        description: Option<Option<String>>,
        tags: Option<Option<Vec<String>>>,
        content: Option<String>,
    ) {
        if let Some(name) = name {
            self.name = name;
        }
        if let Some(description) = description {
            self.description = description;
        }
        if let Some(tags) = tags {
            self.tags = tags;
        }
        if let Some(content) = content {
            self.content = content;
        }
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }

    pub fn to_markdown(&self) -> String {
        let frontmatter = SkillFrontmatter {
            name: self.name.clone(),
            description: self.description.clone(),
            tags: self.tags.clone(),
            triggers: if self.triggers.is_empty() {
                None
            } else {
                Some(self.triggers.clone())
            },
            suggested_tools: if self.suggested_tools.is_empty() {
                None
            } else {
                Some(self.suggested_tools.clone())
            },
            scripts: if self.scripts.is_empty() {
                None
            } else {
                Some(self.scripts.clone())
            },
            references: if self.references.is_empty() {
                None
            } else {
                Some(self.references.clone())
            },
            gating: self.gating.clone(),
            version: self.version.clone(),
            author: self.author.clone(),
            license: self.license.clone(),
            status: if self.status == SkillStatus::Active {
                None
            } else {
                Some(self.status.clone())
            },
            auto_complete: if self.auto_complete { Some(true) } else { None },
        };

        let yaml = serde_yaml::to_string(&frontmatter).unwrap_or_default();
        format!("---\n{}\n---\n\n{}", yaml, self.content)
    }

    pub fn from_markdown(id: &str, markdown: &str) -> anyhow::Result<Self> {
        if !markdown.starts_with("---") {
            anyhow::bail!("Invalid markdown format: missing frontmatter");
        }

        let lines: Vec<&str> = markdown.lines().collect();
        let end_line_offset = lines
            .iter()
            .skip(1)
            .position(|line| line.trim() == "---")
            .map(|index| index + 1)
            .ok_or_else(|| anyhow::anyhow!("Invalid markdown format: frontmatter not closed"))?;

        let frontmatter_lines = &lines[1..end_line_offset];
        let frontmatter_str = frontmatter_lines.join("\n");
        let content_start = lines[..=end_line_offset].join("\n").len() + "\n".len();
        let content = markdown[content_start..].trim().to_string();
        let frontmatter: SkillFrontmatter = serde_yaml::from_str(&frontmatter_str)?;

        let mut skill = Self::new(
            id.to_string(),
            frontmatter.name,
            frontmatter.description,
            frontmatter.tags,
            content,
        );
        skill.suggested_tools = frontmatter.suggested_tools.unwrap_or_default();
        skill.triggers = frontmatter.triggers.unwrap_or_default();
        skill.scripts = frontmatter.scripts.unwrap_or_default();
        skill.references = frontmatter.references.unwrap_or_default();
        skill.gating = frontmatter.gating;
        skill.version = frontmatter.version;
        skill.author = frontmatter.author;
        skill.license = frontmatter.license;
        skill.status = frontmatter.status.unwrap_or_default();
        skill.auto_complete = frontmatter.auto_complete.unwrap_or(false);

        Ok(skill)
    }
}

impl Default for Skill {
    fn default() -> Self {
        Self::new(String::new(), String::new(), None, None, String::new())
    }
}

/// Frontmatter structure for import/export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFrontmatter {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triggers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scripts: Option<Vec<SkillScript>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub references: Option<Vec<SkillReference>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gating: Option<SkillGating>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<SkillStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_complete: Option<bool>,
}

/// Skill metadata stored separately from markdown content.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SkillMeta {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<SkillScript>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<SkillReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggested_tools: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gating: Option<SkillGating>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default)]
    pub executable: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggested_tools: Vec<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default)]
    pub executable: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggested_tools: Vec<String>,
    #[serde(default)]
    pub source: SkillSource,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
}

/// Provider trait for accessing skills (implemented in runtime)
pub trait SkillProvider: Send + Sync {
    /// List all available skills
    fn list_skills(&self) -> Vec<SkillInfo>;
    /// Get skill content by ID
    fn get_skill(&self, id: &str) -> Option<SkillContent>;
    /// Export a skill to markdown
    fn export_skill(&self, id: &str) -> Result<String, String>;
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

    #[test]
    fn skill_markdown_round_trips_references() {
        let mut skill = Skill::new(
            "reference-skill".to_string(),
            "Reference Skill".to_string(),
            None,
            None,
            "# Root content".to_string(),
        );
        skill.references = vec![SkillReference {
            id: "ref-1".to_string(),
            path: "references/ref-1.md".to_string(),
            title: Some("Reference One".to_string()),
            summary: Some("One line summary".to_string()),
        }];

        let markdown = skill.to_markdown();
        let parsed = Skill::from_markdown("reference-skill", &markdown).unwrap();

        assert_eq!(parsed.references.len(), 1);
        let reference = &parsed.references[0];
        assert_eq!(reference.id, "ref-1");
        assert_eq!(reference.path, "references/ref-1.md");
        assert_eq!(reference.title.as_deref(), Some("Reference One"));
        assert_eq!(reference.summary.as_deref(), Some("One line summary"));
    }

    #[test]
    fn skill_frontmatter_with_yaml_separator_in_value() {
        let markdown = r#"---
name: Test Skill
description: "Supports --- separator"
tags:
  - test
---

# Content"#;

        let skill = Skill::from_markdown("test", markdown).unwrap();

        assert_eq!(skill.name, "Test Skill");
        assert_eq!(
            skill.description,
            Some("Supports --- separator".to_string())
        );
        assert!(skill.content.contains("# Content"));
    }
}
