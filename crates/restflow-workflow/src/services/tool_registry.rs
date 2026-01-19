//! Tool registry service for creating tool registries with storage access.

use crate::storage::skill::SkillStorage;
use restflow_ai::{SkillContent, SkillInfo, SkillProvider, SkillTool, ToolRegistry};
use std::sync::Arc;

/// SkillProvider implementation that reads from SkillStorage
pub struct SkillStorageProvider {
    storage: SkillStorage,
}

impl SkillStorageProvider {
    /// Create a new SkillStorageProvider
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
}

/// Create a tool registry with all available tools including storage-backed tools.
///
/// This function creates a registry with:
/// - Default tools from restflow-ai (http_request, run_python, send_email)
/// - SkillTool that can access skills from storage
pub fn create_tool_registry(skill_storage: SkillStorage) -> ToolRegistry {
    let mut registry = restflow_ai::tools::default_registry();

    // Add SkillTool with storage access
    let skill_provider = Arc::new(SkillStorageProvider::new(skill_storage));
    registry.register(SkillTool::new(skill_provider));

    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::Database;
    use tempfile::tempdir;

    fn setup_storage() -> (SkillStorage, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = SkillStorage::new(db).unwrap();
        (storage, temp_dir)
    }

    #[test]
    fn test_create_tool_registry() {
        let (storage, _temp_dir) = setup_storage();
        let registry = create_tool_registry(storage);

        // Should have default tools + skill tool
        assert!(registry.has("http_request"));
        assert!(registry.has("run_python"));
        assert!(registry.has("send_email"));
        assert!(registry.has("skill"));
    }

    #[test]
    fn test_skill_provider_list_empty() {
        let (storage, _temp_dir) = setup_storage();
        let provider = SkillStorageProvider::new(storage);

        let skills = provider.list_skills();
        assert!(skills.is_empty());
    }

    #[test]
    fn test_skill_provider_with_data() {
        let (storage, _temp_dir) = setup_storage();

        // Add a skill
        let skill = crate::models::Skill::new(
            "test-skill".to_string(),
            "Test Skill".to_string(),
            Some("A test".to_string()),
            Some(vec!["http_request".to_string()]),
            "# Test Content".to_string(),
        );
        storage.create(&skill).unwrap();

        let provider = SkillStorageProvider::new(storage);

        // Test list
        let skills = provider.list_skills();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "test-skill");

        // Test get
        let content = provider.get_skill("test-skill").unwrap();
        assert_eq!(content.id, "test-skill");
        assert!(content.content.contains("Test Content"));

        // Test get nonexistent
        assert!(provider.get_skill("nonexistent").is_none());
    }
}
