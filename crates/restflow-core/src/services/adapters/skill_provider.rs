//! SkillProvider implementation backed by SkillStorage.

use crate::storage::skill::SkillStorage;
use restflow_traits::skill::{SkillContent, SkillInfo, SkillProvider, SkillRecord, SkillUpdate};

/// SkillProvider implementation that reads from SkillStorage.
pub struct SkillStorageProvider {
    storage: SkillStorage,
}

impl SkillStorageProvider {
    /// Create a new SkillStorageProvider.
    pub fn new(storage: SkillStorage) -> Self {
        Self { storage }
    }
}

impl SkillProvider for SkillStorageProvider {
    fn list_skills(&self) -> Vec<SkillInfo> {
        match self.storage.list() {
            Ok(skills) => skills
                .into_iter()
                .map(|s| SkillInfo {
                    id: s.id,
                    name: s.name,
                    description: s.description,
                    tags: s.tags,
                })
                .collect(),
            Err(e) => {
                tracing::error!(error = %e, "Failed to list skills");
                Vec::new()
            }
        }
    }

    fn get_skill(&self, id: &str) -> Option<SkillContent> {
        match self.storage.get(id) {
            Ok(Some(skill)) => Some(SkillContent {
                id: skill.id,
                name: skill.name,
                content: skill.content,
            }),
            Ok(None) => None,
            Err(e) => {
                tracing::error!(error = %e, skill_id = %id, "Failed to get skill");
                None
            }
        }
    }

    fn create_skill(&self, skill: SkillRecord) -> Result<SkillRecord, String> {
        let model = crate::models::Skill::new(
            skill.id.clone(),
            skill.name.clone(),
            skill.description.clone(),
            skill.tags.clone(),
            skill.content.clone(),
        );
        self.storage.create(&model).map_err(|e| e.to_string())?;
        Ok(skill)
    }

    fn update_skill(&self, id: &str, update: SkillUpdate) -> Result<SkillRecord, String> {
        let mut skill = self
            .storage
            .get(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Skill {} not found", id))?;

        skill.update(update.name, update.description, update.tags, update.content);
        self.storage.update(id, &skill).map_err(|e| e.to_string())?;

        Ok(SkillRecord {
            id: skill.id,
            name: skill.name,
            description: skill.description,
            tags: skill.tags,
            content: skill.content,
        })
    }

    fn delete_skill(&self, id: &str) -> Result<bool, String> {
        if !self.storage.exists(id).map_err(|e| e.to_string())? {
            return Ok(false);
        }
        self.storage.delete(id).map_err(|e| e.to_string())?;
        Ok(true)
    }

    fn export_skill(&self, id: &str) -> Result<String, String> {
        let skill = self
            .storage
            .get(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Skill {} not found", id))?;
        Ok(skill.to_markdown())
    }

    fn import_skill(
        &self,
        id: &str,
        markdown: &str,
        overwrite: bool,
    ) -> Result<SkillRecord, String> {
        let exists = self.storage.exists(id).map_err(|e| e.to_string())?;
        if exists && !overwrite {
            return Err(format!("Skill {} already exists", id));
        }

        let skill = crate::models::Skill::from_markdown(id, markdown).map_err(|e| e.to_string())?;

        if exists {
            self.storage.update(id, &skill).map_err(|e| e.to_string())?;
        } else {
            self.storage.create(&skill).map_err(|e| e.to_string())?;
        }

        Ok(SkillRecord {
            id: skill.id,
            name: skill.name,
            description: skill.description,
            tags: skill.tags,
            content: skill.content,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn setup() -> (SkillStorageProvider, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(redb::Database::create(db_path).unwrap());
        let storage = SkillStorage::new(db).unwrap();
        (SkillStorageProvider::new(storage), temp_dir)
    }

    fn sample_record(id: &str) -> SkillRecord {
        SkillRecord {
            id: id.to_string(),
            name: format!("Skill {}", id),
            description: Some("A test skill".to_string()),
            tags: Some(vec!["test".to_string()]),
            content: "# Test\nDo something.".to_string(),
        }
    }

    #[test]
    fn test_create_and_get_skill() {
        let (provider, _dir) = setup();
        let record = sample_record("skill-1");
        provider.create_skill(record.clone()).unwrap();

        let content = provider.get_skill("skill-1").unwrap();
        assert_eq!(content.name, "Skill skill-1");
        assert_eq!(content.id, "skill-1");
    }

    #[test]
    fn test_list_skills() {
        let (provider, _dir) = setup();
        provider.create_skill(sample_record("a")).unwrap();
        provider.create_skill(sample_record("b")).unwrap();

        let skills = provider.list_skills();
        assert_eq!(skills.len(), 2);
    }

    #[test]
    fn test_update_skill() {
        let (provider, _dir) = setup();
        provider.create_skill(sample_record("upd")).unwrap();

        let update = SkillUpdate {
            name: Some("Updated Name".to_string()),
            description: None,
            tags: None,
            content: None,
        };
        let updated = provider.update_skill("upd", update).unwrap();
        assert_eq!(updated.name, "Updated Name");
    }

    #[test]
    fn test_delete_skill() {
        let (provider, _dir) = setup();
        provider.create_skill(sample_record("del")).unwrap();
        assert!(provider.delete_skill("del").unwrap());
        assert!(!provider.delete_skill("del").unwrap());
    }

    #[test]
    fn test_get_nonexistent_skill() {
        let (provider, _dir) = setup();
        assert!(provider.get_skill("nonexistent").is_none());
    }

    #[test]
    fn test_export_skill() {
        let (provider, _dir) = setup();
        provider.create_skill(sample_record("exp")).unwrap();
        let markdown = provider.export_skill("exp").unwrap();
        assert!(!markdown.is_empty());
    }

    #[test]
    fn test_import_skill_no_overwrite() {
        let (provider, _dir) = setup();
        let markdown = "---\nname: Imported\ndescription: A skill\n---\n# Content";
        provider.import_skill("imp", markdown, false).unwrap();

        let result = provider.import_skill("imp", markdown, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_import_skill_with_overwrite() {
        let (provider, _dir) = setup();
        let markdown = "---\nname: Imported\ndescription: A skill\n---\n# Content";
        provider.import_skill("imp2", markdown, false).unwrap();
        let result = provider.import_skill("imp2", markdown, true);
        assert!(result.is_ok());
    }
}
