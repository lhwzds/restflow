//! SkillProvider implementations for user, external, and systemskill catalogs.

use crate::models::Skill;
use crate::skill_files;
use crate::storage::skill::SkillStorage;
use restflow_traits::skill::{
    SkillContent, SkillInfo, SkillProvider, SkillRecord, SkillSource, SkillUpdate,
};
use std::collections::HashSet;
use std::sync::Arc;

fn skill_info(skill: Skill) -> SkillInfo {
    SkillInfo {
        id: skill.id,
        name: skill.name,
        description: skill.description,
        tags: skill.tags,
        source: skill.source,
        read_only: skill.read_only,
        source_ref: skill.source_ref,
    }
}

fn skill_content(skill: Skill) -> SkillContent {
    SkillContent {
        id: skill.id,
        name: skill.name,
        content: skill.content,
        source: skill.source,
        read_only: skill.read_only,
        source_ref: skill.source_ref,
    }
}

fn skill_record(skill: Skill) -> SkillRecord {
    SkillRecord {
        id: skill.id,
        name: skill.name,
        description: skill.description,
        tags: skill.tags,
        content: skill.content,
        source: skill.source,
        read_only: skill.read_only,
        source_ref: skill.source_ref,
    }
}

fn writable_model_from_record(record: &SkillRecord) -> Result<Skill, String> {
    if record.read_only || record.source == SkillSource::System {
        return Err("systemskill entries are read-only and cannot be stored".to_string());
    }
    let mut model = Skill::new(
        record.id.clone(),
        record.name.clone(),
        record.description.clone(),
        record.tags.clone(),
        record.content.clone(),
    );
    model.source = SkillSource::User;
    model.read_only = false;
    model.source_ref = None;
    Ok(model)
}

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
            Ok(skills) => skills.into_iter().map(skill_info).collect(),
            Err(e) => {
                tracing::error!(error = %e, "Failed to list skills");
                Vec::new()
            }
        }
    }

    fn get_skill(&self, id: &str) -> Option<SkillContent> {
        match self.storage.get(id) {
            Ok(Some(skill)) => Some(skill_content(skill)),
            Ok(None) => None,
            Err(e) => {
                tracing::error!(error = %e, skill_id = %id, "Failed to get skill");
                None
            }
        }
    }

    fn create_skill(&self, skill: SkillRecord) -> Result<SkillRecord, String> {
        let model = writable_model_from_record(&skill)?;
        self.storage.create(&model).map_err(|e| e.to_string())?;
        Ok(skill_record(model))
    }

    fn update_skill(&self, id: &str, update: SkillUpdate) -> Result<SkillRecord, String> {
        let mut skill = self
            .storage
            .get(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Skill {} not found", id))?;
        if skill.read_only || skill.source == SkillSource::System {
            return Err(format!("Skill {} is read-only", id));
        }

        skill.update(update.name, update.description, update.tags, update.content);
        self.storage.update(id, &skill).map_err(|e| e.to_string())?;

        Ok(skill_record(skill))
    }

    fn delete_skill(&self, id: &str) -> Result<bool, String> {
        let Some(skill) = self.storage.get(id).map_err(|e| e.to_string())? else {
            return Ok(false);
        };
        if skill.read_only || skill.source == SkillSource::System {
            return Err(format!("Skill {} is read-only", id));
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
        if skill.read_only || skill.source == SkillSource::System {
            return Err("systemskill entries are read-only and cannot be imported".to_string());
        }

        if exists {
            let existing = self
                .storage
                .get(id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Skill {} not found", id))?;
            if existing.read_only || existing.source == SkillSource::System {
                return Err(format!("Skill {} is read-only", id));
            }
            self.storage.update(id, &skill).map_err(|e| e.to_string())?;
        } else {
            self.storage.create(&skill).map_err(|e| e.to_string())?;
        }

        Ok(skill_record(skill))
    }
}

/// Read-only provider for RestFlow-shipped systemskills.
pub struct SystemSkillProvider;

impl SystemSkillProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SystemSkillProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillProvider for SystemSkillProvider {
    fn list_skills(&self) -> Vec<SkillInfo> {
        match skill_files::list_systemskills() {
            Ok(skills) => skills.into_iter().map(skill_info).collect(),
            Err(e) => {
                tracing::error!(error = %e, "Failed to list systemskills");
                Vec::new()
            }
        }
    }

    fn get_skill(&self, id: &str) -> Option<SkillContent> {
        match skill_files::get_systemskill(id) {
            Ok(Some(skill)) => Some(skill_content(skill)),
            Ok(None) => None,
            Err(e) => {
                tracing::error!(error = %e, skill_id = %id, "Failed to get systemskill");
                None
            }
        }
    }

    fn create_skill(&self, _: SkillRecord) -> Result<SkillRecord, String> {
        Err("systemskill catalog is read-only".to_string())
    }

    fn update_skill(&self, id: &str, _: SkillUpdate) -> Result<SkillRecord, String> {
        Err(format!("systemskill '{}' is read-only", id))
    }

    fn delete_skill(&self, id: &str) -> Result<bool, String> {
        Err(format!("systemskill '{}' is read-only", id))
    }

    fn export_skill(&self, id: &str) -> Result<String, String> {
        let skill = skill_files::get_systemskill(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Skill {} not found", id))?;
        Ok(skill.to_markdown())
    }

    fn import_skill(&self, id: &str, _: &str, _: bool) -> Result<SkillRecord, String> {
        Err(format!("systemskill '{}' is read-only", id))
    }
}

/// Composite provider that exposes systemskills together with user/external storage skills.
pub struct CompositeSkillProvider {
    system: Arc<dyn SkillProvider>,
    storage: Arc<dyn SkillProvider>,
    reserved_ids: HashSet<String>,
}

impl CompositeSkillProvider {
    pub fn new(system: Arc<dyn SkillProvider>, storage: Arc<dyn SkillProvider>) -> Self {
        let reserved_ids = skill_files::systemskill_ids().map(str::to_string).collect();
        Self {
            system,
            storage,
            reserved_ids,
        }
    }

    pub fn with_storage(skill_storage: SkillStorage) -> Self {
        Self::new(
            Arc::new(SystemSkillProvider::new()),
            Arc::new(SkillStorageProvider::new(skill_storage)),
        )
    }

    fn is_reserved(&self, id: &str) -> bool {
        self.reserved_ids.contains(id)
    }
}

impl SkillProvider for CompositeSkillProvider {
    fn list_skills(&self) -> Vec<SkillInfo> {
        let mut skills = self.system.list_skills();
        skills.extend(
            self.storage
                .list_skills()
                .into_iter()
                .filter(|skill| !self.is_reserved(&skill.id)),
        );
        skills
    }

    fn get_skill(&self, id: &str) -> Option<SkillContent> {
        self.system
            .get_skill(id)
            .or_else(|| self.storage.get_skill(id))
    }

    fn create_skill(&self, skill: SkillRecord) -> Result<SkillRecord, String> {
        if self.is_reserved(&skill.id) || skill.source == SkillSource::System {
            return Err(format!(
                "Skill '{}' is reserved by a systemskill and cannot be overwritten",
                skill.id
            ));
        }
        self.storage.create_skill(skill)
    }

    fn update_skill(&self, id: &str, update: SkillUpdate) -> Result<SkillRecord, String> {
        if self.is_reserved(id) {
            return Err(format!("systemskill '{}' is read-only", id));
        }
        self.storage.update_skill(id, update)
    }

    fn delete_skill(&self, id: &str) -> Result<bool, String> {
        if self.is_reserved(id) {
            return Err(format!("systemskill '{}' is read-only", id));
        }
        self.storage.delete_skill(id)
    }

    fn export_skill(&self, id: &str) -> Result<String, String> {
        if self.is_reserved(id) {
            return self.system.export_skill(id);
        }
        self.storage.export_skill(id)
    }

    fn import_skill(
        &self,
        id: &str,
        markdown: &str,
        overwrite: bool,
    ) -> Result<SkillRecord, String> {
        if self.is_reserved(id) {
            return Err(format!("systemskill '{}' is read-only", id));
        }
        self.storage.import_skill(id, markdown, overwrite)
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
            source: SkillSource::User,
            read_only: false,
            source_ref: None,
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

    #[test]
    fn test_systemskill_provider_is_read_only() {
        let provider = SystemSkillProvider::new();
        let team = provider.get_skill("team").expect("team systemskill");
        assert_eq!(team.source, SkillSource::System);
        assert!(team.read_only);
        assert!(provider.delete_skill("team").is_err());
    }

    #[test]
    fn test_composite_provider_hides_storage_shadow_for_systemskill() {
        let (storage_provider, _dir) = setup();
        storage_provider
            .create_skill(SkillRecord {
                id: "team".to_string(),
                name: "Shadow".to_string(),
                description: None,
                tags: None,
                content: "# Shadow".to_string(),
                source: SkillSource::User,
                read_only: false,
                source_ref: None,
            })
            .unwrap();
        let provider = CompositeSkillProvider::new(
            Arc::new(SystemSkillProvider::new()),
            Arc::new(storage_provider),
        );

        let skills = provider.list_skills();
        assert_eq!(skills.iter().filter(|skill| skill.id == "team").count(), 1);
        assert_eq!(
            provider.get_skill("team").unwrap().source,
            SkillSource::System
        );
        assert!(provider.delete_skill("team").is_err());
    }
}
