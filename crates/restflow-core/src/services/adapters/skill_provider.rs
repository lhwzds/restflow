//! SkillProvider implementation backed by SkillStorage.

use crate::storage::skill::SkillStorage;
use restflow_ai::{SkillContent, SkillInfo, SkillProvider, SkillRecord, SkillUpdate};

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
