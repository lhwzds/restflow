//! Skill model for AI agent prompt templates.
//! Skills are reusable prompt instructions that can be used by AI agents.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::models::skill_folder::{SkillGating, SkillReference, SkillScript};
use crate::models::StorageMode;

/// Skill lifecycle status used for discovery and planning.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum SkillStatus {
    #[default]
    Active,
    Completed,
    Archived,
    Draft,
}

/// A skill represents a reusable AI prompt template
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Skill {
    /// Unique identifier for the skill
    pub id: String,
    /// Display name of the skill
    pub name: String,
    /// Optional description of what the skill does
    pub description: Option<String>,
    /// Optional tags for categorization
    pub tags: Option<Vec<String>>,
    /// Optional trigger phrases that auto-activate this skill from user input
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<String>,
    /// The markdown content of the skill (instructions for the AI)
    pub content: String,
    /// Optional folder path for skills stored on disk
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder_path: Option<String>,
    /// Optional suggested tools for the skill
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggested_tools: Vec<String>,
    /// Optional scripts defined by the skill
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<SkillScript>,
    /// Optional references defined by the skill
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<SkillReference>,
    /// Optional gating requirements for the skill
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gating: Option<SkillGating>,
    /// Optional version for the skill definition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Optional author for the skill definition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Optional license for the skill definition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Optional content hash for change detection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// Lifecycle status for the skill
    #[serde(default)]
    pub status: SkillStatus,
    /// Automatically mark skill as completed after a successful execute call
    #[serde(default)]
    pub auto_complete: bool,
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

impl Skill {
    /// Create a new skill with the given parameters
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
            storage_mode: StorageMode::DatabaseOnly,
            is_synced: false,
            created_at: now,
            updated_at: now,
        }
    }

    /// Update the skill's mutable fields
    pub fn update(
        &mut self,
        name: Option<String>,
        description: Option<Option<String>>,
        tags: Option<Option<Vec<String>>>,
        content: Option<String>,
    ) {
        if let Some(n) = name {
            self.name = n;
        }
        if let Some(d) = description {
            self.description = d;
        }
        if let Some(t) = tags {
            self.tags = t;
        }
        if let Some(c) = content {
            self.content = c;
        }
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }
}

impl Default for Skill {
    fn default() -> Self {
        Self::new(String::new(), String::new(), None, None, String::new())
    }
}

/// Frontmatter structure for import/export
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

impl Skill {
    /// Export the skill to markdown format with YAML frontmatter
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
        format!("---\n{}---\n\n{}", yaml, self.content)
    }

    /// Parse a skill from markdown with YAML frontmatter
    pub fn from_markdown(id: &str, markdown: &str) -> anyhow::Result<Self> {
        // Check if the markdown starts with frontmatter
        if !markdown.starts_with("---") {
            return Err(anyhow::anyhow!(
                "Invalid markdown format: missing frontmatter"
            ));
        }

        // Find the end of frontmatter
        let rest = &markdown[3..];
        let end_index = rest
            .find("---")
            .ok_or_else(|| anyhow::anyhow!("Invalid markdown format: frontmatter not closed"))?;

        let frontmatter_str = &rest[..end_index].trim();
        let content = rest[end_index + 3..].trim().to_string();

        // Parse the YAML frontmatter
        let frontmatter: SkillFrontmatter = serde_yaml::from_str(frontmatter_str)?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::skill_folder::SkillReference;

    #[test]
    fn test_skill_new() {
        let skill = Skill::new(
            "test-skill".to_string(),
            "Test Skill".to_string(),
            Some("A test skill".to_string()),
            Some(vec!["test".to_string(), "example".to_string()]),
            "# Test Content".to_string(),
        );

        assert_eq!(skill.id, "test-skill");
        assert_eq!(skill.name, "Test Skill");
        assert_eq!(skill.description, Some("A test skill".to_string()));
        assert_eq!(
            skill.tags,
            Some(vec!["test".to_string(), "example".to_string()])
        );
        assert_eq!(skill.content, "# Test Content");
    }

    #[test]
    fn test_skill_to_markdown() {
        let skill = Skill::new(
            "test-skill".to_string(),
            "Test Skill".to_string(),
            Some("A test skill".to_string()),
            Some(vec!["git".to_string(), "workflow".to_string()]),
            "# Test Content\n\nSome instructions here.".to_string(),
        );
        let mut skill = skill;
        skill.triggers = vec!["code review".to_string(), "review pr".to_string()];

        let markdown = skill.to_markdown();
        assert!(markdown.contains("name: Test Skill"));
        assert!(markdown.contains("description: A test skill"));
        assert!(markdown.contains("triggers:"));
        assert!(markdown.contains("- code review"));
        assert!(markdown.contains("# Test Content"));
    }

    #[test]
    fn test_skill_from_markdown() {
        let markdown = r#"---
name: Commit Message Generator
description: Generate commit messages from git diff
tags:
  - git
  - workflow
triggers:
  - code review
  - review PR
---

# Commit Message Generator

When generating commit messages, follow these steps..."#;

        let skill = Skill::from_markdown("commit-message", markdown).unwrap();
        assert_eq!(skill.id, "commit-message");
        assert_eq!(skill.name, "Commit Message Generator");
        assert_eq!(
            skill.description,
            Some("Generate commit messages from git diff".to_string())
        );
        assert_eq!(
            skill.tags,
            Some(vec!["git".to_string(), "workflow".to_string()])
        );
        assert_eq!(
            skill.triggers,
            vec!["code review".to_string(), "review PR".to_string()]
        );
        assert!(skill.content.contains("# Commit Message Generator"));
    }

    #[test]
    fn test_skill_from_markdown_minimal() {
        let markdown = r#"---
name: Simple Skill
---

# Simple instructions"#;

        let skill = Skill::from_markdown("simple", markdown).unwrap();
        assert_eq!(skill.id, "simple");
        assert_eq!(skill.name, "Simple Skill");
        assert_eq!(skill.description, None);
        assert_eq!(skill.tags, None);
    }

    #[test]
    fn test_skill_from_markdown_invalid() {
        let markdown = "# No frontmatter";
        let result = Skill::from_markdown("test", markdown);
        assert!(result.is_err());
    }

    #[test]
    fn test_skill_status_default() {
        let skill = Skill::default();
        assert_eq!(skill.status, SkillStatus::Active);
        assert!(!skill.auto_complete);
    }

    #[test]
    fn test_skill_status_from_frontmatter() {
        let markdown = r#"---
name: Statused Skill
status: completed
auto_complete: true
---

Done"#;
        let skill = Skill::from_markdown("statused", markdown).unwrap();
        assert_eq!(skill.status, SkillStatus::Completed);
        assert!(skill.auto_complete);
    }

    #[test]
    fn test_skill_reference_roundtrip_with_title_and_summary() {
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
}
