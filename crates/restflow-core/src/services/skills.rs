//! Skills service layer for business logic.

use crate::{
    AppCore,
    models::{Skill, ValidationError},
};
use anyhow::{Context, Result};
use regex::Regex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

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

/// Get full content for a skill reference by skill_id and ref_id
pub async fn get_skill_reference(
    core: &Arc<AppCore>,
    skill_id: &str,
    ref_id: &str,
) -> Result<Option<String>> {
    let skill = get_skill(core, skill_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Skill not found: {}", skill_id))?;

    let reference = skill
        .references
        .iter()
        .find(|reference| reference.id == ref_id)
        .ok_or_else(|| {
            anyhow::anyhow!("Reference '{}' not found in skill '{}'", ref_id, skill_id)
        })?;

    if let Some(reference_skill) = get_skill(core, &reference.id).await? {
        return Ok(Some(reference_skill.content));
    }

    let kv_store_key = format!("skill-ref:{}:{}", skill_id, ref_id);
    if let Some(content) = core
        .storage
        .kv_store
        .quick_get(&kv_store_key, None)?
    {
        return Ok(Some(content));
    }

    if !reference.path.trim().is_empty() {
        let path = resolve_reference_path(&skill, &reference.path);
        if let Ok(content) = tokio::fs::read_to_string(path).await {
            return Ok(Some(content));
        }
    }

    Ok(None)
}

fn resolve_reference_path(skill: &Skill, reference_path: &str) -> PathBuf {
    let path = Path::new(reference_path);
    if path.is_absolute() {
        return path.to_path_buf();
    }

    if let Some(folder_path) = &skill.folder_path {
        return Path::new(folder_path).join(path);
    }

    path.to_path_buf()
}

/// Export a skill to markdown format
pub fn export_skill_to_markdown(skill: &Skill) -> String {
    skill.to_markdown()
}

/// Import a skill from markdown format
pub fn import_skill_from_markdown(id: &str, markdown: &str) -> Result<Skill> {
    Skill::from_markdown(id, markdown).context("Failed to parse markdown")
}

/// Validate a skill with Basic and Standard conformance checks.
pub fn validate_skill(skill: &Skill) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    if skill.name.trim().is_empty() {
        errors.push(ValidationError::new("name", "Skill name cannot be empty"));
    }

    if skill.content.trim().is_empty() {
        errors.push(ValidationError::new(
            "content",
            "Skill content cannot be empty",
        ));
    }

    if let Some(tags) = &skill.tags {
        for (index, tag) in tags.iter().enumerate() {
            if tag.trim().is_empty() {
                errors.push(ValidationError::new(
                    format!("tags[{index}]"),
                    "Tag cannot be empty",
                ));
            }
        }
    }

    for (index, trigger) in skill.triggers.iter().enumerate() {
        if trigger.trim().is_empty() {
            errors.push(ValidationError::new(
                format!("triggers[{index}]"),
                "Trigger cannot be empty",
            ));
        }
    }

    static VARIABLE_REGEX: OnceLock<Regex> = OnceLock::new();
    static VARIABLE_NAME_REGEX: OnceLock<Regex> = OnceLock::new();
    let variable_regex =
        VARIABLE_REGEX.get_or_init(|| Regex::new(r"\{\{\s*([^{}]+?)\s*\}\}").unwrap());
    let variable_name_regex =
        VARIABLE_NAME_REGEX.get_or_init(|| Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap());
    for captures in variable_regex.captures_iter(&skill.content) {
        let variable_name = captures[1].trim();
        if !variable_name_regex.is_match(variable_name) {
            errors.push(ValidationError::new(
                "content",
                format!("Invalid variable '{variable_name}': must match [a-zA-Z_][a-zA-Z0-9_]*"),
            ));
        }
    }

    for tool in &skill.suggested_tools {
        if !variable_name_regex.is_match(tool) {
            errors.push(ValidationError::new(
                "suggested_tools",
                format!("Invalid tool name '{tool}': must match [a-zA-Z_][a-zA-Z0-9_]*"),
            ));
        }
    }

    errors
}

/// Validate a skill with complete checks that require external registry data.
pub fn validate_skill_complete(
    skill: &Skill,
    tool_names: &[String],
    skill_ids: &[String],
) -> Vec<ValidationError> {
    let mut errors = validate_skill(skill);

    let known_tools: HashSet<&str> = tool_names.iter().map(String::as_str).collect();
    let known_skill_ids: HashSet<&str> = skill_ids.iter().map(String::as_str).collect();

    for tool in &skill.suggested_tools {
        if !known_tools.contains(tool.as_str()) {
            errors.push(ValidationError::new(
                "suggested_tools",
                format!("Tool '{tool}' not found in registry"),
            ));
        }
    }

    for reference in &skill.references {
        if !known_skill_ids.contains(reference.id.as_str()) {
            errors.push(ValidationError::new(
                "references",
                format!("Referenced skill '{}' not found", reference.id),
            ));
        }
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SkillReference;
    use std::sync::OnceLock;
    use tempfile::tempdir;
    use tokio::sync::Mutex;

    const MASTER_KEY_ENV: &str = "RESTFLOW_MASTER_KEY";
    const RESTFLOW_DIR_ENV: &str = "RESTFLOW_DIR";

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    async fn create_test_core() -> Arc<AppCore> {
        let _env_lock = env_lock().lock().await;
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let state_dir = temp_dir.path().join("state");
        std::fs::create_dir_all(&state_dir).unwrap();

        let previous_master_key = std::env::var_os(MASTER_KEY_ENV);
        let previous_restflow_dir = std::env::var_os(RESTFLOW_DIR_ENV);
        // SAFETY: env vars are modified under env_lock() and callers use
        // #[tokio::test(flavor = "current_thread")] so no worker threads
        // can race on reads.
        unsafe {
            std::env::set_var(RESTFLOW_DIR_ENV, &state_dir);
            std::env::remove_var(MASTER_KEY_ENV);
        }
        let core = Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap());
        unsafe {
            if let Some(value) = previous_restflow_dir {
                std::env::set_var(RESTFLOW_DIR_ENV, value);
            } else {
                std::env::remove_var(RESTFLOW_DIR_ENV);
            }
            if let Some(value) = previous_master_key {
                std::env::set_var(MASTER_KEY_ENV, value);
            } else {
                std::env::remove_var(MASTER_KEY_ENV);
            }
        }
        core
    }

    fn create_test_skill(id: &str, name: &str) -> Skill {
        Skill::new(
            id.to_string(),
            name.to_string(),
            Some(format!("Description for {}", name)),
            Some(vec!["test".to_string()]),
            format!("# {}\n\nContent here.", name),
        )
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_list_skills_empty() {
        let core = create_test_core().await;
        let skills = list_skills(&core).await.unwrap();
        // Default skills are bootstrapped; only verify no test artifacts exist
        assert!(!skills.is_empty());
        assert!(!skills.iter().any(|skill| skill.id == "test-skill"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_create_and_get_skill() {
        let core = create_test_core().await;

        let skill = create_test_skill("test-skill", "Test Skill");
        create_skill(&core, skill.clone()).await.unwrap();

        let retrieved = get_skill(&core, "test-skill").await.unwrap();
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, "test-skill");
        assert_eq!(retrieved.name, "Test Skill");
        assert_eq!(
            retrieved.description,
            Some("Description for Test Skill".to_string())
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_list_skills_multiple() {
        let core = create_test_core().await;

        let base_skills = list_skills(&core).await.unwrap();
        let base_len = base_skills.len();

        let skill1 = create_test_skill("skill-1", "Skill One");
        let skill2 = create_test_skill("skill-2", "Skill Two");

        create_skill(&core, skill1).await.unwrap();
        create_skill(&core, skill2).await.unwrap();

        let skills = list_skills(&core).await.unwrap();
        assert_eq!(skills.len(), base_len + 2);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_update_skill() {
        let core = create_test_core().await;

        let mut skill = create_test_skill("test-skill", "Original Name");
        create_skill(&core, skill.clone()).await.unwrap();

        skill.update(
            Some("Updated Name".to_string()),
            Some(Some("Updated description".to_string())),
            None,
            Some("# Updated content".to_string()),
        );

        update_skill(&core, "test-skill", &skill).await.unwrap();

        let retrieved = get_skill(&core, "test-skill").await.unwrap().unwrap();
        assert_eq!(retrieved.name, "Updated Name");
        assert_eq!(
            retrieved.description,
            Some("Updated description".to_string())
        );
        assert_eq!(retrieved.content, "# Updated content");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_delete_skill() {
        let core = create_test_core().await;

        let skill = create_test_skill("test-skill", "Test Skill");
        create_skill(&core, skill).await.unwrap();

        assert!(skill_exists(&core, "test-skill").await.unwrap());

        delete_skill(&core, "test-skill").await.unwrap();

        assert!(!skill_exists(&core, "test-skill").await.unwrap());
        assert!(get_skill(&core, "test-skill").await.unwrap().is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_skill_exists() {
        let core = create_test_core().await;

        assert!(!skill_exists(&core, "nonexistent").await.unwrap());

        let skill = create_test_skill("test-skill", "Test Skill");
        create_skill(&core, skill).await.unwrap();

        assert!(skill_exists(&core, "test-skill").await.unwrap());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_get_nonexistent_skill() {
        let core = create_test_core().await;

        let result = get_skill(&core, "nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_get_skill_reference_from_referenced_skill() {
        let core = create_test_core().await;

        let mut skill = create_test_skill("root-skill", "Root Skill");
        skill.references = vec![SkillReference {
            id: "ref-skill".to_string(),
            path: "references/ref-skill.md".to_string(),
            title: Some("Reference Skill".to_string()),
            summary: Some("Reference summary".to_string()),
        }];

        let reference_skill = Skill::new(
            "ref-skill".to_string(),
            "Reference Skill".to_string(),
            Some("Referenced skill".to_string()),
            None,
            "# Reference Skill\n\nDetailed content.".to_string(),
        );

        create_skill(&core, skill).await.unwrap();
        create_skill(&core, reference_skill.clone()).await.unwrap();

        let content = get_skill_reference(&core, "root-skill", "ref-skill")
            .await
            .unwrap();
        assert_eq!(content, Some(reference_skill.content));
    }

    #[test]
    fn test_export_skill_to_markdown() {
        let skill = create_test_skill("test-skill", "Test Skill");
        let markdown = export_skill_to_markdown(&skill);

        assert!(markdown.contains("name: Test Skill"));
        assert!(markdown.contains("description: Description for Test Skill"));
        assert!(markdown.contains("# Test Skill"));
    }

    #[test]
    fn test_import_skill_from_markdown() {
        let markdown = r#"---
name: Imported Skill
description: A skill imported from markdown
tags:
  - imported
  - test
---

# Imported Skill

This is the skill content."#;

        let skill = import_skill_from_markdown("imported-skill", markdown).unwrap();
        assert_eq!(skill.id, "imported-skill");
        assert_eq!(skill.name, "Imported Skill");
        assert_eq!(
            skill.description,
            Some("A skill imported from markdown".to_string())
        );
        assert_eq!(
            skill.tags,
            Some(vec!["imported".to_string(), "test".to_string()])
        );
        assert!(skill.content.contains("# Imported Skill"));
    }

    #[test]
    fn test_import_skill_from_markdown_invalid() {
        let markdown = "# No frontmatter";
        let result = import_skill_from_markdown("test", markdown);
        assert!(result.is_err());
    }

    #[test]
    fn test_roundtrip_markdown_export_import() {
        let original = create_test_skill("test-skill", "Test Skill");
        let markdown = export_skill_to_markdown(&original);
        let imported = import_skill_from_markdown("test-skill", &markdown).unwrap();

        assert_eq!(imported.id, original.id);
        assert_eq!(imported.name, original.name);
        assert_eq!(imported.description, original.description);
        assert_eq!(imported.tags, original.tags);
        // Note: content might have minor whitespace differences after roundtrip
    }

    #[test]
    fn test_validate_skill_empty_fields() {
        let mut skill = create_test_skill("skill-1", "Skill One");
        skill.name = "   ".to_string();
        skill.content = "\n".to_string();
        skill.tags = Some(vec!["ok".to_string(), " ".to_string()]);
        skill.triggers = vec!["".to_string()];

        let errors = validate_skill(&skill);

        assert!(errors.iter().any(|e| e.field == "name"));
        assert!(errors.iter().any(|e| e.field == "content"));
        assert!(errors.iter().any(|e| e.field == "tags[1]"));
        assert!(errors.iter().any(|e| e.field == "triggers[0]"));
    }

    #[test]
    fn test_validate_skill_invalid_tool_and_variable_name() {
        let mut skill = create_test_skill("skill-2", "Skill Two");
        skill.content = "Use {{invalid-name}} and {{valid_name}}".to_string();
        skill.suggested_tools = vec!["good_tool".to_string(), "bad-tool".to_string()];

        let errors = validate_skill(&skill);

        assert!(
            errors
                .iter()
                .any(|e| e.field == "content" && e.message.contains("invalid-name"))
        );
        assert!(
            errors
                .iter()
                .any(|e| e.field == "suggested_tools" && e.message.contains("bad-tool"))
        );
    }

    #[test]
    fn test_validate_skill_complete_unknown_tool_and_reference() {
        let mut skill = create_test_skill("skill-3", "Skill Three");
        skill.suggested_tools = vec!["bash".to_string(), "missing_tool".to_string()];
        skill.references = vec![
            crate::models::skill_folder::SkillReference {
                id: "known-skill".to_string(),
                path: "./SKILL.md".to_string(),
                title: None,
                summary: None,
            },
            crate::models::skill_folder::SkillReference {
                id: "missing-skill".to_string(),
                path: "./missing.md".to_string(),
                title: None,
                summary: None,
            },
        ];

        let tool_names = vec!["bash".to_string(), "file".to_string()];
        let skill_ids = vec!["known-skill".to_string(), "other-skill".to_string()];

        let errors = validate_skill_complete(&skill, &tool_names, &skill_ids);

        assert!(
            errors
                .iter()
                .any(|e| { e.field == "suggested_tools" && e.message.contains("missing_tool") })
        );
        assert!(
            errors
                .iter()
                .any(|e| e.field == "references" && e.message.contains("missing-skill"))
        );
    }

    #[test]
    fn test_validate_skill_complete_valid_skill() {
        let mut skill = create_test_skill("skill-4", "Skill Four");
        skill.content = "Use {{ticket_id}} with {{ticket_id}}".to_string();
        skill.suggested_tools = vec!["bash".to_string()];
        skill.references = vec![crate::models::skill_folder::SkillReference {
            id: "known-skill".to_string(),
            path: "./SKILL.md".to_string(),
            title: None,
            summary: None,
        }];

        let tool_names = vec!["bash".to_string()];
        let skill_ids = vec!["known-skill".to_string()];

        let errors = validate_skill_complete(&skill, &tool_names, &skill_ids);

        assert!(errors.is_empty());
    }
}
