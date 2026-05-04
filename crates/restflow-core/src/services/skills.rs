//! Skills service layer for the skrun-managed catalog.

use crate::{
    AppCore,
    models::{Skill, ValidationError},
    services::adapters::SkrunSkillProvider,
};
use anyhow::{Context, Result, anyhow};
use regex::Regex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

/// List all skills visible to RestFlow.
pub async fn list_skills(_core: &Arc<AppCore>) -> Result<Vec<Skill>> {
    list_available_skills()
}

/// List the skrun-managed skill catalog visible to runtime validation and preflight.
pub fn list_available_skills() -> Result<Vec<Skill>> {
    SkrunSkillProvider::default()
        .try_list_skill_models()
        .map_err(|error| anyhow!("skrun skill catalog unavailable: {error}"))
}

/// Check whether a skill exists in the skrun-managed catalog.
pub fn skill_exists_in_catalog(id: &str) -> Result<bool> {
    SkrunSkillProvider::default()
        .try_get_skill_model(id)
        .map(|skill| skill.is_some())
        .map_err(|error| anyhow!("skrun skill catalog unavailable: {error}"))
}

/// Get a skill by ID.
pub async fn get_skill(_core: &Arc<AppCore>, id: &str) -> Result<Option<Skill>> {
    SkrunSkillProvider::default()
        .try_get_skill_model(id)
        .map_err(|error| anyhow!("skrun skill catalog unavailable: {error}"))
}

/// RestFlow no longer persists skills. Use skrun for skill creation.
pub async fn create_skill(_core: &Arc<AppCore>, skill: Skill) -> Result<()> {
    anyhow::bail!(
        "RestFlow no longer persists skills in storage; create skill '{}' through skrun",
        skill.id
    )
}

/// RestFlow no longer persists skills. Use skrun for skill updates.
pub async fn update_skill(_core: &Arc<AppCore>, id: &str, _skill: &Skill) -> Result<()> {
    anyhow::bail!(
        "RestFlow no longer persists skills in storage; update skill '{}' through skrun",
        id
    )
}

/// RestFlow no longer persists skills. Use skrun for skill removal.
pub async fn delete_skill(_core: &Arc<AppCore>, id: &str) -> Result<()> {
    anyhow::bail!(
        "RestFlow no longer persists skills in storage; remove skill '{}' through skrun",
        id
    )
}

/// Check if a skill exists.
pub async fn skill_exists(_core: &Arc<AppCore>, id: &str) -> Result<bool> {
    skill_exists_in_catalog(id)
}

/// Get full content for a skill reference by skill_id and ref_id.
pub async fn get_skill_reference(
    core: &Arc<AppCore>,
    skill_id: &str,
    ref_id: &str,
) -> Result<Option<String>> {
    let skill = get_skill(core, skill_id)
        .await?
        .ok_or_else(|| anyhow!("Skill not found: {}", skill_id))?;

    let reference = skill
        .references
        .iter()
        .find(|reference| reference.id == ref_id)
        .ok_or_else(|| anyhow!("Reference '{}' not found in skill '{}'", ref_id, skill_id))?;

    if let Some(reference_skill) = get_skill(core, &reference.id).await? {
        return Ok(Some(reference_skill.content));
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

/// Export a skill to markdown format.
pub fn export_skill_to_markdown(skill: &Skill) -> String {
    skill.to_markdown()
}

/// Import a skill from markdown format.
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
    use std::ffi::OsString;
    use std::sync::MutexGuard;
    use tempfile::{TempDir, tempdir};

    const MASTER_KEY_ENV: &str = "RESTFLOW_MASTER_KEY";
    const RESTFLOW_DIR_ENV: &str = "RESTFLOW_DIR";
    const SKRUN_SKILLS_DIR_ENV: &str = "SKRUN_SKILLS_DIR";

    struct SkillsTestEnv {
        _lock: MutexGuard<'static, ()>,
        temp_dir: TempDir,
        previous_master_key: Option<OsString>,
        previous_restflow_dir: Option<OsString>,
        previous_skrun_skills_dir: Option<OsString>,
    }

    impl Drop for SkillsTestEnv {
        fn drop(&mut self) {
            unsafe {
                if let Some(value) = self.previous_restflow_dir.as_ref() {
                    std::env::set_var(RESTFLOW_DIR_ENV, value);
                } else {
                    std::env::remove_var(RESTFLOW_DIR_ENV);
                }
                if let Some(value) = self.previous_master_key.as_ref() {
                    std::env::set_var(MASTER_KEY_ENV, value);
                } else {
                    std::env::remove_var(MASTER_KEY_ENV);
                }
                if let Some(value) = self.previous_skrun_skills_dir.as_ref() {
                    std::env::set_var(SKRUN_SKILLS_DIR_ENV, value);
                } else {
                    std::env::remove_var(SKRUN_SKILLS_DIR_ENV);
                }
            }
        }
    }

    impl SkillsTestEnv {
        fn install_markdown_skill(&self, mut artifact: skrun::SkillArtifact) {
            let root = self.temp_dir.path().join("skrun-skills");
            artifact.executable = false;
            skrun::save_artifact(root.join(&artifact.id), &artifact).unwrap();
            unsafe {
                std::env::set_var(SKRUN_SKILLS_DIR_ENV, root);
            }
        }
    }

    #[allow(clippy::await_holding_lock)]
    async fn create_test_core() -> (Arc<AppCore>, SkillsTestEnv) {
        let env_lock = crate::paths::restflow_dir_env_lock();
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let state_dir = temp_dir.path().join("state");
        std::fs::create_dir_all(&state_dir).unwrap();

        let previous_master_key = std::env::var_os(MASTER_KEY_ENV);
        let previous_restflow_dir = std::env::var_os(RESTFLOW_DIR_ENV);
        let previous_skrun_skills_dir = std::env::var_os(SKRUN_SKILLS_DIR_ENV);
        unsafe {
            std::env::set_var(RESTFLOW_DIR_ENV, &state_dir);
            std::env::set_var(MASTER_KEY_ENV, "11".repeat(32));
            std::env::set_var(
                SKRUN_SKILLS_DIR_ENV,
                temp_dir.path().join("empty-skrun-skills"),
            );
        }
        let core = Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap());
        (
            core,
            SkillsTestEnv {
                _lock: env_lock,
                temp_dir,
                previous_master_key,
                previous_restflow_dir,
                previous_skrun_skills_dir,
            },
        )
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
    async fn test_list_skills_empty_without_skrun() {
        let (core, _env) = create_test_core().await;
        let skills = list_skills(&core).await.unwrap();
        assert!(skills.is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_list_and_get_team_skrun_skill() {
        let (core, env) = create_test_core().await;
        let mut artifact = skrun::SkillArtifact::markdown(
            "team",
            "Team",
            "0.1.0",
            "# Team\n\nUse spawn_subagent_batch.",
        );
        artifact.suggested_tools = vec!["spawn_subagent_batch".to_string()];
        env.install_markdown_skill(artifact);

        let skills = list_skills(&core).await.unwrap();
        let team = skills
            .iter()
            .find(|skill| skill.id == "team")
            .expect("team skrun skill should be listed");
        assert_eq!(team.source, crate::models::SkillSource::External);
        assert!(team.read_only);

        let team = get_skill(&core, "team")
            .await
            .unwrap()
            .expect("team skrun skill should be readable");
        assert_eq!(team.name, "Team");
        assert!(team.content.contains("spawn_subagent_batch"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_create_update_delete_skill_reject_storage_writes() {
        let (core, _env) = create_test_core().await;
        let skill = create_test_skill("test-skill", "Test Skill");

        assert!(create_skill(&core, skill.clone()).await.is_err());
        assert!(update_skill(&core, "test-skill", &skill).await.is_err());
        assert!(delete_skill(&core, "test-skill").await.is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_get_nonexistent_skill() {
        let (core, _env) = create_test_core().await;
        let result = get_skill(&core, "nonexistent").await.unwrap();
        assert!(result.is_none());
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
