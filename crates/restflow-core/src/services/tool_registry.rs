//! Tool registry service for creating tool registries with storage access.

use crate::memory::UnifiedSearchEngine;
use crate::models::{MemorySearchQuery, SearchMode, UnifiedSearchQuery};
use crate::storage::{ChatSessionStorage, MemoryStorage};
use crate::storage::skill::SkillStorage;
use restflow_ai::{SkillContent, SkillInfo, SkillProvider, SkillTool, Tool, ToolOutput, ToolRegistry};
use serde_json::json;
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
/// - Memory search tool for unified memory and session search
pub fn create_tool_registry(
    skill_storage: SkillStorage,
    memory_storage: MemoryStorage,
    chat_storage: ChatSessionStorage,
) -> ToolRegistry {
    let mut registry = restflow_ai::tools::default_registry();

    // Add SkillTool with storage access
    let skill_provider = Arc::new(SkillStorageProvider::new(skill_storage));
    registry.register(SkillTool::new(skill_provider));

    // Add unified memory search tool
    let search_engine = UnifiedSearchEngine::new(memory_storage, chat_storage);
    registry.register(MemorySearchTool::new(search_engine));

    registry
}

#[derive(Clone)]
struct MemorySearchTool {
    engine: UnifiedSearchEngine,
}

impl MemorySearchTool {
    fn new(engine: UnifiedSearchEngine) -> Self {
        Self { engine }
    }
}

#[async_trait::async_trait]
impl Tool for MemorySearchTool {
    fn name(&self) -> &str {
        "memory_search"
    }

    fn description(&self) -> &str {
        "Search through long-term memory and chat session history"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Keywords or phrase to search for"
                },
                "agent_id": {
                    "type": "string",
                    "description": "Agent ID to search within"
                },
                "include_sessions": {
                    "type": "boolean",
                    "description": "Whether to search chat sessions",
                    "default": true
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results to return",
                    "default": 5,
                    "minimum": 1
                },
                "offset": {
                    "type": "integer",
                    "description": "Offset for pagination",
                    "default": 0,
                    "minimum": 0
                }
            },
            "required": ["query", "agent_id"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> restflow_ai::error::Result<ToolOutput> {
        let query = input
            .get("query")
            .and_then(|value| value.as_str())
            .ok_or_else(|| restflow_ai::error::Error::Tool("Missing query parameter".to_string()))?;
        let agent_id = input
            .get("agent_id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                restflow_ai::error::Error::Tool("Missing agent_id parameter".to_string())
            })?;
        let include_sessions = input
            .get("include_sessions")
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        let limit = input
            .get("limit")
            .and_then(|value| value.as_u64())
            .unwrap_or(5)
            .min(u32::MAX as u64) as u32;
        let offset = input
            .get("offset")
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
            .min(u32::MAX as u64) as u32;

        let base = MemorySearchQuery::new(agent_id.to_string())
            .with_query(query.to_string())
            .with_mode(SearchMode::Keyword)
            .paginate(limit, offset);
        let unified_query = UnifiedSearchQuery::new(base).with_sessions(include_sessions);

        let results = self
            .engine
            .search(&unified_query)
            .map_err(|e| restflow_ai::error::Error::Tool(e.to_string()))?;

        Ok(ToolOutput::success(serde_json::to_value(results)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::Database;
    use tempfile::tempdir;

    fn setup_storage() -> (SkillStorage, MemoryStorage, ChatSessionStorage, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let skill_storage = SkillStorage::new(db.clone()).unwrap();
        let memory_storage = MemoryStorage::new(db.clone()).unwrap();
        let chat_storage = ChatSessionStorage::new(db).unwrap();
        (skill_storage, memory_storage, chat_storage, temp_dir)
    }

    #[test]
    fn test_create_tool_registry() {
        let (skill_storage, memory_storage, chat_storage, _temp_dir) = setup_storage();
        let registry = create_tool_registry(skill_storage, memory_storage, chat_storage);

        // Should have default tools + skill tool
        assert!(registry.has("http_request"));
        assert!(registry.has("run_python"));
        assert!(registry.has("send_email"));
        assert!(registry.has("skill"));
        assert!(registry.has("memory_search"));
    }

    #[test]
    fn test_skill_provider_list_empty() {
        let (storage, _memory_storage, _chat_storage, _temp_dir) = setup_storage();
        let provider = SkillStorageProvider::new(storage);

        let skills = provider.list_skills();
        assert!(skills.is_empty());
    }

    #[test]
    fn test_skill_provider_with_data() {
        let (storage, _memory_storage, _chat_storage, _temp_dir) = setup_storage();

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
