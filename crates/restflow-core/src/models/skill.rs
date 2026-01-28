//! Skill model for AI agent prompt templates.
//! Skills are reusable prompt instructions that can be used by AI agents.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

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
    /// The markdown content of the skill (instructions for the AI)
    pub content: String,
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
            content,
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

/// Frontmatter structure for import/export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFrontmatter {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

impl Skill {
    /// Export the skill to markdown format with YAML frontmatter
    pub fn to_markdown(&self) -> String {
        let frontmatter = SkillFrontmatter {
            name: self.name.clone(),
            description: self.description.clone(),
            tags: self.tags.clone(),
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

        Ok(Self::new(
            id.to_string(),
            frontmatter.name,
            frontmatter.description,
            frontmatter.tags,
            content,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let markdown = skill.to_markdown();
        assert!(markdown.contains("name: Test Skill"));
        assert!(markdown.contains("description: A test skill"));
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
}
