//! Tool registry service for creating tool registries with storage access.

use crate::lsp::LspManager;
use crate::memory::UnifiedSearchEngine;
use crate::models::{AgentTaskStatus, MemorySearchQuery, SearchMode, SharedEntry, TaskSchedule, UnifiedSearchQuery, Visibility};
use crate::storage::skill::SkillStorage;
use crate::storage::{AgentTaskStorage, ChatSessionStorage, ConfigStorage, MemoryStorage, SecretStorage, SharedSpaceStorage};
use chrono::Utc;
use restflow_ai::error::AiError;
use restflow_ai::tools::{ConfigTool, SecretsTool, TaskCreateRequest, TaskStore, TaskTool};
use restflow_ai::{
    SkillContent, SkillInfo, SkillProvider, SkillTool, Tool, ToolOutput, ToolRegistry,
};
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

#[derive(Clone)]
struct TaskStoreAdapter {
    storage: AgentTaskStorage,
}

impl TaskStoreAdapter {
    fn new(storage: AgentTaskStorage) -> Self {
        Self { storage }
    }

    fn parse_status(status: &str) -> Result<AgentTaskStatus, AiError> {
        match status.trim().to_lowercase().as_str() {
            "active" => Ok(AgentTaskStatus::Active),
            "paused" => Ok(AgentTaskStatus::Paused),
            "running" => Ok(AgentTaskStatus::Running),
            "completed" => Ok(AgentTaskStatus::Completed),
            "failed" => Ok(AgentTaskStatus::Failed),
            _ => Err(AiError::Tool(format!("Unknown status: {}", status))),
        }
    }
}

impl TaskStore for TaskStoreAdapter {
    fn create_task(&self, request: TaskCreateRequest) -> restflow_ai::error::Result<serde_json::Value> {
        let schedule = match request.schedule {
            Some(value) => serde_json::from_value::<TaskSchedule>(value)
                .map_err(|e| AiError::Tool(e.to_string()))?,
            None => TaskSchedule::default(),
        };

        let mut task = self
            .storage
            .create_task(request.name, request.agent_id, schedule)
            .map_err(|e| AiError::Tool(e.to_string()))?;

        if let Some(input) = request.input {
            task.input = Some(input);
            self.storage
                .update_task(&task)
                .map_err(|e| AiError::Tool(e.to_string()))?;
        }

        serde_json::to_value(task).map_err(AiError::from)
    }

    fn list_tasks(&self, status: Option<String>) -> restflow_ai::error::Result<serde_json::Value> {
        let tasks = if let Some(status) = status {
            let status = Self::parse_status(&status)?;
            self.storage
                .list_tasks_by_status(status)
                .map_err(|e| AiError::Tool(e.to_string()))?
        } else {
            self.storage
                .list_tasks()
                .map_err(|e| AiError::Tool(e.to_string()))?
        };

        serde_json::to_value(tasks).map_err(AiError::from)
    }

    fn pause_task(&self, id: &str) -> restflow_ai::error::Result<serde_json::Value> {
        let task = self
            .storage
            .pause_task(id)
            .map_err(|e| AiError::Tool(e.to_string()))?;
        serde_json::to_value(task).map_err(AiError::from)
    }

    fn resume_task(&self, id: &str) -> restflow_ai::error::Result<serde_json::Value> {
        let task = self
            .storage
            .resume_task(id)
            .map_err(|e| AiError::Tool(e.to_string()))?;
        serde_json::to_value(task).map_err(AiError::from)
    }

    fn cancel_task(&self, id: &str) -> restflow_ai::error::Result<serde_json::Value> {
        let deleted = self
            .storage
            .delete_task(id)
            .map_err(|e| AiError::Tool(e.to_string()))?;
        Ok(json!({ "id": id, "deleted": deleted }))
    }

    fn run_task(&self, id: &str) -> restflow_ai::error::Result<serde_json::Value> {
        let mut task = self
            .storage
            .get_task(id)
            .map_err(|e| AiError::Tool(e.to_string()))?
            .ok_or_else(|| AiError::Tool(format!("Task {} not found", id)))?;

        let now = chrono::Utc::now().timestamp_millis();
        task.status = AgentTaskStatus::Active;
        task.next_run_at = Some(now);
        task.updated_at = now;

        self.storage
            .update_task(&task)
            .map_err(|e| AiError::Tool(e.to_string()))?;

        serde_json::to_value(task).map_err(AiError::from)
    }
}

/// Create a tool registry with all available tools including storage-backed tools.
///
/// This function creates a registry with:
/// - Default tools from restflow-ai (http_request, run_python, send_email)
/// - SkillTool that can access skills from storage
/// - Memory search tool for unified memory and session search
#[allow(clippy::too_many_arguments)]
pub fn create_tool_registry(
    skill_storage: SkillStorage,
    memory_storage: MemoryStorage,
    chat_storage: ChatSessionStorage,
    shared_space_storage: SharedSpaceStorage,
    secret_storage: SecretStorage,
    config_storage: ConfigStorage,
    agent_task_storage: AgentTaskStorage,
    accessor_id: Option<String>,
) -> ToolRegistry {
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let lsp_manager = Arc::new(LspManager::new(root));
    let mut registry = restflow_ai::tools::default_registry_with_diagnostics(lsp_manager);

    // Add SkillTool with storage access
    let skill_provider = Arc::new(SkillStorageProvider::new(skill_storage));
    registry.register(SkillTool::new(skill_provider));

    // Add unified memory search tool
    let search_engine = UnifiedSearchEngine::new(memory_storage, chat_storage);
    registry.register(MemorySearchTool::new(search_engine));

    // Add shared space tool
    registry.register(SharedSpaceTool::new(shared_space_storage, accessor_id));

    // Add system management tools (read-only by default)
    registry.register(SecretsTool::new(Arc::new(secret_storage)));
    registry.register(ConfigTool::new(Arc::new(config_storage)));
    let task_store = Arc::new(TaskStoreAdapter::new(agent_task_storage));
    registry.register(TaskTool::new(task_store));

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
            .ok_or_else(|| AiError::Tool("Missing query parameter".to_string()))?;
        let agent_id = input
            .get("agent_id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AiError::Tool("Missing agent_id parameter".to_string()))?;
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
            .map_err(|e| AiError::Tool(e.to_string()))?;

        Ok(ToolOutput::success(serde_json::to_value(results)?))
    }
}

#[derive(Clone)]
struct SharedSpaceTool {
    storage: SharedSpaceStorage,
    accessor_id: Option<String>,
}

impl SharedSpaceTool {
    fn new(storage: SharedSpaceStorage, accessor_id: Option<String>) -> Self {
        Self {
            storage,
            accessor_id,
        }
    }

    fn parse_visibility(value: &str) -> Visibility {
        match value {
            "private" => Visibility::Private,
            "shared" => Visibility::Shared,
            _ => Visibility::Public,
        }
    }
}

#[async_trait::async_trait]
impl Tool for SharedSpaceTool {
    fn name(&self) -> &str {
        "shared_space"
    }

    fn description(&self) -> &str {
        "Read and write entries in the shared space storage. Use namespace:name keys."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["get", "set", "delete", "list"],
                    "description": "The action to perform"
                },
                "key": {
                    "type": "string",
                    "description": "The key (namespace:name). Required for get/set/delete."
                },
                "value": {
                    "type": "string",
                    "description": "The value to store. Required for set."
                },
                "visibility": {
                    "type": "string",
                    "enum": ["public", "shared", "private"],
                    "description": "Access level for the entry"
                },
                "content_type": {
                    "type": "string",
                    "description": "Optional content type hint"
                },
                "type_hint": {
                    "type": "string",
                    "description": "Optional type hint for categorization"
                },
                "tags": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    },
                    "description": "Optional tags for filtering"
                },
                "namespace": {
                    "type": "string",
                    "description": "For list: filter by namespace prefix"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> restflow_ai::error::Result<ToolOutput> {
        let action = input
            .get("action")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AiError::Tool("Missing action parameter".to_string()))?;
        let accessor_id = self.accessor_id.as_deref();

        match action {
            "get" => {
                let key = input
                    .get("key")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| AiError::Tool("Missing key parameter".to_string()))?;
                let entry = self
                    .storage
                    .get(key, accessor_id)
                    .map_err(|e| AiError::Tool(e.to_string()))?;
                let payload = match entry {
                    Some(entry) => json!({
                        "found": true,
                        "key": entry.key,
                        "value": entry.value,
                        "content_type": entry.content_type,
                        "visibility": entry.visibility,
                        "tags": entry.tags,
                        "updated_at": entry.updated_at
                    }),
                    None => json!({
                        "found": false,
                        "key": key
                    }),
                };
                Ok(ToolOutput::success(payload))
            }
            "set" => {
                let key = input
                    .get("key")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| AiError::Tool("Missing key parameter".to_string()))?;
                let value = input
                    .get("value")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| AiError::Tool("Missing value parameter".to_string()))?;

                let existing = self
                    .storage
                    .get_unchecked(key)
                    .map_err(|e| AiError::Tool(e.to_string()))?;

                if let Some(ref entry) = existing
                    && !entry.can_write(accessor_id)
                {
                    return Ok(ToolOutput::error(
                        "Access denied: cannot write to this entry",
                    ));
                }

                let visibility = input
                    .get("visibility")
                    .and_then(|value| value.as_str())
                    .map(Self::parse_visibility)
                    .or(existing.as_ref().map(|e| e.visibility))
                    .unwrap_or_default();

                let entry = SharedEntry {
                    key: key.to_string(),
                    value: value.to_string(),
                    visibility,
                    owner: existing
                        .as_ref()
                        .and_then(|e| e.owner.clone())
                        .or_else(|| self.accessor_id.clone()),
                    content_type: input
                        .get("content_type")
                        .and_then(|value| value.as_str())
                        .map(|value| value.to_string())
                        .or_else(|| existing.as_ref().and_then(|e| e.content_type.clone())),
                    type_hint: input
                        .get("type_hint")
                        .and_then(|value| value.as_str())
                        .map(|value| value.to_string())
                        .or_else(|| existing.as_ref().and_then(|e| e.type_hint.clone())),
                    tags: input
                        .get("tags")
                        .and_then(|value| value.as_array())
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(|item| item.as_str().map(|s| s.to_string()))
                                .collect::<Vec<String>>()
                        })
                        .or_else(|| existing.as_ref().map(|e| e.tags.clone()))
                        .unwrap_or_default(),
                    created_at: existing
                        .as_ref()
                        .map(|e| e.created_at)
                        .unwrap_or_else(|| Utc::now().timestamp_millis()),
                    updated_at: Utc::now().timestamp_millis(),
                    last_modified_by: self.accessor_id.clone(),
                };

                self.storage
                    .set(&entry)
                    .map_err(|e| AiError::Tool(e.to_string()))?;

                Ok(ToolOutput::success(json!({
                    "success": true,
                    "key": key,
                    "created": existing.is_none()
                })))
            }
            "delete" => {
                let key = input
                    .get("key")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| AiError::Tool("Missing key parameter".to_string()))?;
                let deleted = self
                    .storage
                    .delete(key, accessor_id)
                    .map_err(|e| AiError::Tool(e.to_string()))?;
                Ok(ToolOutput::success(json!({
                    "deleted": deleted,
                    "key": key
                })))
            }
            "list" => {
                let namespace = input.get("namespace").and_then(|value| value.as_str());
                let prefix = namespace.map(|ns| format!("{}:", ns));
                let entries = self
                    .storage
                    .list(prefix.as_deref(), accessor_id)
                    .map_err(|e| AiError::Tool(e.to_string()))?;
                let items: Vec<_> = entries
                    .iter()
                    .map(|entry| {
                        let preview = if entry.value.len() > 100 {
                            format!("{}...", &entry.value[..100])
                        } else {
                            entry.value.clone()
                        };
                        json!({
                            "key": entry.key,
                            "content_type": entry.content_type,
                            "visibility": entry.visibility,
                            "tags": entry.tags,
                            "updated_at": entry.updated_at,
                            "preview": preview
                        })
                    })
                    .collect();
                Ok(ToolOutput::success(json!({
                    "count": items.len(),
                    "entries": items
                })))
            }
            _ => Ok(ToolOutput::error(format!("Unknown action: {}", action))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::Database;
    use tempfile::tempdir;

    fn setup_storage() -> (
        SkillStorage,
        MemoryStorage,
        ChatSessionStorage,
        SharedSpaceStorage,
        SecretStorage,
        ConfigStorage,
        AgentTaskStorage,
        tempfile::TempDir,
    ) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());

        let state_dir = temp_dir.path().join("state");
        std::fs::create_dir_all(&state_dir).unwrap();
        unsafe {
            std::env::set_var("RESTFLOW_DIR", &state_dir);
        }

        let skill_storage = SkillStorage::new(db.clone()).unwrap();
        let memory_storage = MemoryStorage::new(db.clone()).unwrap();
        let chat_storage = ChatSessionStorage::new(db.clone()).unwrap();
        let shared_space_storage =
            SharedSpaceStorage::new(restflow_storage::SharedSpaceStorage::new(db.clone()).unwrap());
        let secret_storage = SecretStorage::with_config(
            db.clone(),
            restflow_storage::SecretStorageConfig {
                allow_insecure_file_permissions: true,
            },
        )
        .unwrap();
        let config_storage = ConfigStorage::new(db.clone()).unwrap();
        let agent_task_storage = AgentTaskStorage::new(db).unwrap();

        unsafe {
            std::env::remove_var("RESTFLOW_DIR");
        }
        (
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            secret_storage,
            config_storage,
            agent_task_storage,
            temp_dir,
        )
    }

    #[test]
    fn test_create_tool_registry() {
        let (
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            secret_storage,
            config_storage,
            agent_task_storage,
            _temp_dir,
        ) = setup_storage();
        let registry = create_tool_registry(
            skill_storage,
            memory_storage,
            chat_storage,
            shared_space_storage,
            secret_storage,
            config_storage,
            agent_task_storage,
            None,
        );

        // Should have default tools + skill tool
        assert!(registry.has("http_request"));
        assert!(registry.has("run_python"));
        assert!(registry.has("send_email"));
        assert!(registry.has("skill"));
        assert!(registry.has("memory_search"));
        assert!(registry.has("shared_space"));
        // New system management tools
        assert!(registry.has("manage_secrets"));
        assert!(registry.has("manage_config"));
        assert!(registry.has("manage_tasks"));
    }

    #[test]
    fn test_skill_provider_list_empty() {
        let (
            storage,
            _memory_storage,
            _chat_storage,
            _shared_space_storage,
            _secret_storage,
            _config_storage,
            _agent_task_storage,
            _temp_dir,
        ) = setup_storage();
        let provider = SkillStorageProvider::new(storage);

        let skills = provider.list_skills();
        assert!(skills.is_empty());
    }

    #[test]
    fn test_skill_provider_with_data() {
        let (
            storage,
            _memory_storage,
            _chat_storage,
            _shared_space_storage,
            _secret_storage,
            _config_storage,
            _agent_task_storage,
            _temp_dir,
        ) = setup_storage();

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
