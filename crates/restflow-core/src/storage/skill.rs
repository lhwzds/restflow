//! Typed skill storage wrapper.

use crate::models::Skill;
use anyhow::Result;
use redb::Database;
use restflow_storage::SimpleStorage;
use restflow_storage::time_utils;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Typed skill storage wrapper around restflow-storage::SkillStorage.
#[derive(Debug, Clone)]
pub struct SkillStorage {
    inner: restflow_storage::SkillStorage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillUpsertOutcome {
    Created,
    Updated,
}

impl SkillStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            inner: restflow_storage::SkillStorage::new(db)?,
        })
    }

    /// Create a new skill (fails if already exists)
    pub fn create(&self, skill: &Skill) -> Result<()> {
        if self.inner.exists(&skill.id)? {
            return Err(anyhow::anyhow!("Skill {} already exists", skill.id));
        }
        let json = serde_json::to_string(skill)?;
        self.inner.put_raw(&skill.id, json.as_bytes())
    }

    /// Get a skill by ID
    pub fn get(&self, id: &str) -> Result<Option<Skill>> {
        if let Some(bytes) = self.inner.get_raw(id)? {
            let json = std::str::from_utf8(&bytes)?;
            Ok(Some(serde_json::from_str(json)?))
        } else {
            Ok(None)
        }
    }

    /// List all skills
    pub fn list(&self) -> Result<Vec<Skill>> {
        let raw_skills = self.inner.list_raw()?;
        let mut skills = Vec::new();
        for (_, bytes) in raw_skills {
            let json = std::str::from_utf8(&bytes)?;
            let skill: Skill = serde_json::from_str(json)?;
            skills.push(skill);
        }

        // Sort by updated_at descending (most recent first)
        skills.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        Ok(skills)
    }

    /// Update an existing skill
    pub fn update(&self, id: &str, skill: &Skill) -> Result<()> {
        if !self.inner.exists(id)? {
            return Err(anyhow::anyhow!("Skill {} not found", id));
        }
        let json = serde_json::to_string(skill)?;
        self.inner.put_raw(id, json.as_bytes())
    }

    /// Delete a skill
    pub fn delete(&self, id: &str) -> Result<()> {
        self.inner.delete(id)?;
        Ok(())
    }

    /// Create or update a skill
    pub fn upsert(&self, skill: &Skill) -> Result<SkillUpsertOutcome> {
        if self.inner.exists(&skill.id)? {
            self.update(&skill.id, skill)?;
            Ok(SkillUpsertOutcome::Updated)
        } else {
            self.create(skill)?;
            Ok(SkillUpsertOutcome::Created)
        }
    }

    /// Check if a skill exists
    pub fn exists(&self, id: &str) -> Result<bool> {
        self.inner.exists(id)
    }

    /// Rewrite skill folder paths from legacy workspace directories into
    /// the user-global skills directory.
    pub fn migrate_folder_paths_to_user(
        &self,
        user_skills_dir: &Path,
        legacy_skill_dirs: &[PathBuf],
    ) -> Result<usize> {
        if legacy_skill_dirs.is_empty() {
            return Ok(0);
        }

        let mut migrated = 0usize;
        let skills = self.list()?;
        for mut skill in skills {
            let Some(folder_path) = skill.folder_path.clone() else {
                continue;
            };
            let folder_path_buf = PathBuf::from(folder_path);

            let mut rewritten_path = None;
            for legacy_dir in legacy_skill_dirs {
                if !folder_path_buf.starts_with(legacy_dir) {
                    continue;
                }
                let Ok(relative_path) = folder_path_buf.strip_prefix(legacy_dir) else {
                    continue;
                };
                let candidate = user_skills_dir.join(relative_path);
                if candidate.exists() {
                    rewritten_path = Some(candidate);
                    break;
                }
            }

            let Some(new_path) = rewritten_path else {
                continue;
            };

            let normalized = new_path.to_string_lossy().to_string();
            if skill.folder_path.as_deref() == Some(normalized.as_str()) {
                continue;
            }

            skill.folder_path = Some(normalized);
            skill.updated_at = time_utils::now_ms();
            self.update(&skill.id, &skill)?;
            migrated += 1;
        }

        Ok(migrated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::StorageMode;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn setup() -> (SkillStorage, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = SkillStorage::new(db).unwrap();
        (storage, temp_dir)
    }

    #[test]
    fn test_create_and_get() {
        let (storage, _temp_dir) = setup();

        let skill = Skill::new(
            "test-skill".to_string(),
            "Test Skill".to_string(),
            Some("A test skill".to_string()),
            Some(vec!["test".to_string()]),
            "# Test Content".to_string(),
        );

        storage.create(&skill).unwrap();

        let retrieved = storage.get("test-skill").unwrap().unwrap();
        assert_eq!(retrieved.id, "test-skill");
        assert_eq!(retrieved.name, "Test Skill");
        assert_eq!(retrieved.description, Some("A test skill".to_string()));
    }

    #[test]
    fn test_create_duplicate_fails() {
        let (storage, _temp_dir) = setup();

        let skill = Skill::new(
            "test-skill".to_string(),
            "Test Skill".to_string(),
            None,
            None,
            "# Test".to_string(),
        );

        storage.create(&skill).unwrap();
        let result = storage.create(&skill);
        assert!(result.is_err());
    }

    #[test]
    fn test_list() {
        let (storage, _temp_dir) = setup();

        let skill1 = Skill::new(
            "skill-1".to_string(),
            "Skill 1".to_string(),
            None,
            None,
            "# Skill 1".to_string(),
        );
        let skill2 = Skill::new(
            "skill-2".to_string(),
            "Skill 2".to_string(),
            None,
            None,
            "# Skill 2".to_string(),
        );

        storage.create(&skill1).unwrap();
        storage.create(&skill2).unwrap();

        let skills = storage.list().unwrap();
        assert_eq!(skills.len(), 2);
    }

    #[test]
    fn test_update() {
        let (storage, _temp_dir) = setup();

        let mut skill = Skill::new(
            "test-skill".to_string(),
            "Test Skill".to_string(),
            None,
            None,
            "# Test".to_string(),
        );

        storage.create(&skill).unwrap();

        skill.update(
            Some("Updated Name".to_string()),
            Some(Some("New description".to_string())),
            None,
            None,
        );

        storage.update("test-skill", &skill).unwrap();

        let retrieved = storage.get("test-skill").unwrap().unwrap();
        assert_eq!(retrieved.name, "Updated Name");
        assert_eq!(retrieved.description, Some("New description".to_string()));
    }

    #[test]
    fn test_delete() {
        let (storage, _temp_dir) = setup();

        let skill = Skill::new(
            "test-skill".to_string(),
            "Test Skill".to_string(),
            None,
            None,
            "# Test".to_string(),
        );

        storage.create(&skill).unwrap();
        assert!(storage.exists("test-skill").unwrap());

        storage.delete("test-skill").unwrap();
        assert!(!storage.exists("test-skill").unwrap());
    }

    #[test]
    fn test_update_nonexistent_fails() {
        let (storage, _temp_dir) = setup();

        let skill = Skill::new(
            "nonexistent".to_string(),
            "Test".to_string(),
            None,
            None,
            "# Test".to_string(),
        );

        let result = storage.update("nonexistent", &skill);
        assert!(result.is_err());
    }

    #[test]
    fn test_migrate_folder_paths_to_user_updates_legacy_workspace_paths() {
        let (storage, temp_dir) = setup();

        let legacy_base = temp_dir
            .path()
            .join("workspace")
            .join(".restflow")
            .join("skills");
        let user_base = temp_dir.path().join("user").join("skills");
        let legacy_folder = legacy_base.join("legacy-skill");
        let user_folder = user_base.join("legacy-skill");
        std::fs::create_dir_all(&legacy_folder).unwrap();
        std::fs::create_dir_all(&user_folder).unwrap();

        let mut skill = Skill::new(
            "legacy-skill".to_string(),
            "Legacy Skill".to_string(),
            None,
            None,
            "# Legacy".to_string(),
        );
        skill.storage_mode = StorageMode::FileSystemOnly;
        skill.folder_path = Some(legacy_folder.to_string_lossy().to_string());
        storage.create(&skill).unwrap();

        let migrated = storage
            .migrate_folder_paths_to_user(&user_base, &[PathBuf::from(&legacy_base)])
            .unwrap();
        assert_eq!(migrated, 1);

        let updated = storage.get("legacy-skill").unwrap().unwrap();
        assert_eq!(
            updated.folder_path.as_deref(),
            Some(user_folder.to_string_lossy().as_ref())
        );
    }
}
