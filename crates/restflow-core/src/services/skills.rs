//! Skills service layer for business logic.

use crate::{AppCore, models::Skill};
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

#[cfg(test)]
mod tests {
    use super::*;
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
}
