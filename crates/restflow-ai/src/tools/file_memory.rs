//! Memory tools for persisting agent context
//!
//! This module provides tools that allow AI agents to save important context
//! and information, organized by agent_id and session_id.
//! The storage backend is abstracted via the `MemoryStore` trait, supporting
//! both file-based (JSON) and database-backed implementations.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;

use crate::error::Result;
use crate::tools::traits::{Tool, ToolOutput};

// ============== Data Models ==============

/// Metadata for a memory entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntryMeta {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub agent_id: String,
    pub session_id: Option<String>,
}

/// A complete memory entry with content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    #[serde(flatten)]
    pub meta: MemoryEntryMeta,
    pub content: String,
}

// ============== MemoryStore Trait ==============

/// Backend trait for memory CRUD operations.
///
/// Implementations can use file storage or database storage.
/// All methods are synchronous (matching other store traits in the codebase).
/// Methods that scope by agent accept an `agent_id` parameter.
pub trait MemoryStore: Send + Sync {
    /// Save a new memory entry. Returns JSON with `{success, id, title, message}`.
    fn save(&self, agent_id: &str, title: &str, content: &str, tags: &[String]) -> Result<Value>;

    /// Read a single memory by ID. Returns `Some(json)` with `{found, entry}` or `None`.
    fn read_by_id(&self, id: &str) -> Result<Option<Value>>;

    /// Search memories by tag and/or title keyword. Returns `{count, memories}`.
    fn search(
        &self,
        agent_id: &str,
        tag: Option<&str>,
        search: Option<&str>,
        limit: usize,
    ) -> Result<Value>;

    /// List all memory metadata. Returns `{total, count, memories}`.
    fn list(&self, agent_id: &str, tag: Option<&str>, limit: usize) -> Result<Value>;

    /// Delete a memory by ID. Returns `{deleted, id, message}`.
    fn delete(&self, id: &str) -> Result<Value>;
}

// ============== FileMemoryStore (file-based implementation) ==============

/// Configuration for file memory storage
#[derive(Debug, Clone)]
pub struct FileMemoryConfig {
    pub base_path: PathBuf,
    pub agent_id: String,
    pub session_id: Option<String>,
}

impl FileMemoryConfig {
    /// Create a new config with required fields
    pub fn new(base_path: impl Into<PathBuf>, agent_id: impl Into<String>) -> Self {
        Self {
            base_path: base_path.into(),
            agent_id: agent_id.into(),
            session_id: None,
        }
    }

    /// Set the session ID
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Get the directory path for storing memories
    fn memory_dir(&self) -> PathBuf {
        let mut path = self.base_path.clone();
        path.push("memory");
        path.push(&self.agent_id);
        if let Some(ref session) = self.session_id {
            path.push(session);
        }
        path
    }

    /// Get the file path for a specific memory entry
    fn entry_path(&self, id: &str) -> PathBuf {
        let mut path = self.memory_dir();
        path.push(format!("{}.json", id));
        path
    }

    /// Get the index file path
    fn index_path(&self) -> PathBuf {
        let mut path = self.memory_dir();
        path.push("_index.json");
        path
    }
}

/// File-based implementation of MemoryStore.
///
/// Stores memories as individual JSON files with an `_index.json` for metadata.
/// Uses synchronous std::fs operations (files are small).
pub struct FileMemoryStore {
    config: FileMemoryConfig,
}

impl FileMemoryStore {
    /// Create a new FileMemoryStore with the given config
    pub fn new(config: FileMemoryConfig) -> Self {
        Self { config }
    }

    fn generate_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    fn load_index(&self) -> Result<Vec<MemoryEntryMeta>> {
        let path = self.config.index_path();
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let index: Vec<MemoryEntryMeta> = serde_json::from_str(&content)?;
            Ok(index)
        } else {
            Ok(Vec::new())
        }
    }

    fn save_index(&self, index: &[MemoryEntryMeta]) -> Result<()> {
        let path = self.config.index_path();
        let content = serde_json::to_string_pretty(index)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    fn load_entry(&self, id: &str) -> Result<Option<MemoryEntry>> {
        let path = self.config.entry_path(id);
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let entry: MemoryEntry = serde_json::from_str(&content)?;
            Ok(Some(entry))
        } else {
            Ok(None)
        }
    }
}

impl MemoryStore for FileMemoryStore {
    fn save(&self, agent_id: &str, title: &str, content: &str, tags: &[String]) -> Result<Value> {
        let dir = self.config.memory_dir();
        std::fs::create_dir_all(&dir)?;

        let now = Utc::now();
        let id = Self::generate_id();

        let entry = MemoryEntry {
            meta: MemoryEntryMeta {
                id: id.clone(),
                title: title.to_string(),
                tags: tags.to_vec(),
                created_at: now,
                updated_at: now,
                agent_id: agent_id.to_string(),
                session_id: self.config.session_id.clone(),
            },
            content: content.to_string(),
        };

        let entry_path = self.config.entry_path(&id);
        let entry_json = serde_json::to_string_pretty(&entry)?;
        std::fs::write(&entry_path, entry_json)?;

        let mut index = self.load_index().unwrap_or_default();
        index.push(entry.meta);
        if let Err(e) = self.save_index(&index) {
            tracing::warn!("Failed to update memory index: {}", e);
        }

        Ok(json!({
            "success": true,
            "id": id,
            "title": title,
            "message": "Memory saved successfully"
        }))
    }

    fn read_by_id(&self, id: &str) -> Result<Option<Value>> {
        match self.load_entry(id)? {
            Some(entry) => Ok(Some(json!({
                "found": true,
                "entry": entry
            }))),
            None => Ok(None),
        }
    }

    fn search(
        &self,
        _agent_id: &str,
        tag: Option<&str>,
        search: Option<&str>,
        limit: usize,
    ) -> Result<Value> {
        let index = self.load_index()?;
        let mut results: Vec<&MemoryEntryMeta> = index.iter().collect();

        if let Some(tag) = tag {
            let tag_lower = tag.to_lowercase();
            results.retain(|m| m.tags.iter().any(|t| t.to_lowercase().contains(&tag_lower)));
        }

        if let Some(search) = search {
            let search_lower = search.to_lowercase();
            results.retain(|m| m.title.to_lowercase().contains(&search_lower));
        }

        results.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        results.truncate(limit);

        Ok(json!({
            "count": results.len(),
            "memories": results
        }))
    }

    fn list(&self, _agent_id: &str, tag: Option<&str>, limit: usize) -> Result<Value> {
        let index = self.load_index()?;
        let total = index.len();
        let mut results: Vec<&MemoryEntryMeta> = index.iter().collect();

        if let Some(tag) = tag {
            let tag_lower = tag.to_lowercase();
            results.retain(|m| m.tags.iter().any(|t| t.to_lowercase().contains(&tag_lower)));
        }

        results.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        results.truncate(limit);

        Ok(json!({
            "total": total,
            "count": results.len(),
            "memories": results.iter().map(|m| json!({
                "id": m.id,
                "title": m.title,
                "tags": m.tags,
                "created_at": m.created_at,
                "updated_at": m.updated_at
            })).collect::<Vec<_>>()
        }))
    }

    fn delete(&self, id: &str) -> Result<Value> {
        let entry_path = self.config.entry_path(id);

        if !entry_path.exists() {
            return Ok(json!({
                "deleted": false,
                "message": format!("No memory found with ID: {}", id)
            }));
        }

        std::fs::remove_file(&entry_path)?;

        let mut index = self.load_index().unwrap_or_default();
        index.retain(|m| m.id != id);
        if let Err(e) = self.save_index(&index) {
            tracing::warn!("Failed to update memory index after delete: {}", e);
        }

        Ok(json!({
            "deleted": true,
            "id": id,
            "message": "Memory deleted successfully"
        }))
    }
}

// ============== Tool Implementations ==============

/// Tool for saving important context to memory
pub struct SaveMemoryTool {
    store: Arc<dyn MemoryStore>,
}

impl SaveMemoryTool {
    pub fn new(store: Arc<dyn MemoryStore>) -> Self {
        Self { store }
    }
}

#[derive(Debug, Deserialize)]
struct SaveMemoryInput {
    agent_id: String,
    title: String,
    content: String,
    #[serde(default)]
    tags: Vec<String>,
}

#[async_trait]
impl Tool for SaveMemoryTool {
    fn name(&self) -> &str {
        "save_to_memory"
    }

    fn description(&self) -> &str {
        "Store a persistent memory entry with title, content, and optional tags."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "The agent ID to store this memory under"
                },
                "title": {
                    "type": "string",
                    "description": "A descriptive title for this memory entry"
                },
                "content": {
                    "type": "string",
                    "description": "The content to save - can be facts, decisions, context, etc."
                },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional tags for categorization and easier retrieval"
                }
            },
            "required": ["agent_id", "title", "content"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: SaveMemoryInput = match serde_json::from_value(input) {
            Ok(p) => p,
            Err(e) => return Ok(ToolOutput::error(format!("Invalid input: {}", e))),
        };

        match self.store.save(
            &params.agent_id,
            &params.title,
            &params.content,
            &params.tags,
        ) {
            Ok(result) => Ok(ToolOutput::success(result)),
            Err(e) => Ok(ToolOutput::error(format!("Failed to save memory: {}", e))),
        }
    }
}

/// Tool for reading saved memories
pub struct ReadMemoryTool {
    store: Arc<dyn MemoryStore>,
}

impl ReadMemoryTool {
    pub fn new(store: Arc<dyn MemoryStore>) -> Self {
        Self { store }
    }
}

#[derive(Debug, Deserialize)]
struct ReadMemoryInput {
    agent_id: String,
    id: Option<String>,
    tag: Option<String>,
    search: Option<String>,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    10
}

#[async_trait]
impl Tool for ReadMemoryTool {
    fn name(&self) -> &str {
        "read_memory"
    }

    fn description(&self) -> &str {
        "Retrieve stored memory entries by id, tag, or title keyword search."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "The agent ID to scope the memory search"
                },
                "id": {
                    "type": "string",
                    "description": "Specific memory ID to retrieve"
                },
                "tag": {
                    "type": "string",
                    "description": "Filter by tag"
                },
                "search": {
                    "type": "string",
                    "description": "Search keyword in titles"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results (default: 10)",
                    "default": 10
                }
            },
            "required": ["agent_id"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: ReadMemoryInput = match serde_json::from_value(input) {
            Ok(p) => p,
            Err(e) => return Ok(ToolOutput::error(format!("Invalid input: {}", e))),
        };

        // If specific ID requested, return that entry
        if let Some(ref id) = params.id {
            return match self.store.read_by_id(id) {
                Ok(Some(result)) => Ok(ToolOutput::success(result)),
                Ok(None) => Ok(ToolOutput::success(json!({
                    "found": false,
                    "message": format!("No memory found with ID: {}", id)
                }))),
                Err(e) => Ok(ToolOutput::error(format!("Failed to read memory: {}", e))),
            };
        }

        // Search/filter
        match self.store.search(
            &params.agent_id,
            params.tag.as_deref(),
            params.search.as_deref(),
            params.limit,
        ) {
            Ok(result) => Ok(ToolOutput::success(result)),
            Err(e) => Ok(ToolOutput::error(format!(
                "Failed to search memories: {}",
                e
            ))),
        }
    }
}

/// Tool for listing all saved memories
pub struct ListMemoryTool {
    store: Arc<dyn MemoryStore>,
}

impl ListMemoryTool {
    pub fn new(store: Arc<dyn MemoryStore>) -> Self {
        Self { store }
    }
}

#[derive(Debug, Deserialize)]
struct ListMemoryInput {
    agent_id: String,
    tag: Option<String>,
    #[serde(default = "default_list_limit")]
    limit: usize,
}

fn default_list_limit() -> usize {
    50
}

#[async_trait]
impl Tool for ListMemoryTool {
    fn name(&self) -> &str {
        "list_memories"
    }

    fn description(&self) -> &str {
        "List stored memory metadata (id, title, tags, timestamps) with optional tag filtering."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "The agent ID to list memories for"
                },
                "tag": {
                    "type": "string",
                    "description": "Optional tag to filter by"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results (default: 50)",
                    "default": 50
                }
            },
            "required": ["agent_id"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: ListMemoryInput = match serde_json::from_value(input) {
            Ok(p) => p,
            Err(e) => return Ok(ToolOutput::error(format!("Invalid input: {}", e))),
        };

        match self
            .store
            .list(&params.agent_id, params.tag.as_deref(), params.limit)
        {
            Ok(result) => Ok(ToolOutput::success(result)),
            Err(e) => Ok(ToolOutput::error(format!("Failed to list memories: {}", e))),
        }
    }
}

/// Tool for deleting a memory entry
pub struct DeleteMemoryTool {
    store: Arc<dyn MemoryStore>,
}

impl DeleteMemoryTool {
    pub fn new(store: Arc<dyn MemoryStore>) -> Self {
        Self { store }
    }
}

#[derive(Debug, Deserialize)]
struct DeleteMemoryInput {
    id: String,
}

#[async_trait]
impl Tool for DeleteMemoryTool {
    fn name(&self) -> &str {
        "delete_memory"
    }

    fn description(&self) -> &str {
        "Delete a stored memory entry by id."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "The ID of the memory to delete"
                }
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: DeleteMemoryInput = match serde_json::from_value(input) {
            Ok(p) => p,
            Err(e) => return Ok(ToolOutput::error(format!("Invalid input: {}", e))),
        };

        match self.store.delete(&params.id) {
            Ok(result) => Ok(ToolOutput::success(result)),
            Err(e) => Ok(ToolOutput::error(format!("Failed to delete memory: {}", e))),
        }
    }
}

// ============== Tests ==============

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_store(temp_dir: &TempDir) -> Arc<dyn MemoryStore> {
        let config = FileMemoryConfig::new(temp_dir.path(), "test-agent");
        Arc::new(FileMemoryStore::new(config))
    }

    #[test]
    fn test_config_memory_dir() {
        let temp_dir = TempDir::new().unwrap();
        let config = FileMemoryConfig::new(temp_dir.path(), "test-agent");

        let dir = config.memory_dir();
        assert!(dir.ends_with("memory/test-agent"));

        let config_with_session = config.with_session("session-123");
        let dir_with_session = config_with_session.memory_dir();
        assert!(dir_with_session.ends_with("memory/test-agent/session-123"));
    }

    #[test]
    fn test_config_entry_path() {
        let temp_dir = TempDir::new().unwrap();
        let config = FileMemoryConfig::new(temp_dir.path(), "test-agent");

        let path = config.entry_path("entry-id");
        assert!(path.ends_with("entry-id.json"));
    }

    #[test]
    fn test_save_tool_schema() {
        let temp_dir = TempDir::new().unwrap();
        let tool = SaveMemoryTool::new(test_store(&temp_dir));

        assert_eq!(tool.name(), "save_to_memory");
        assert!(!tool.description().is_empty());

        let schema = tool.parameters_schema();
        let props = schema.get("properties").unwrap();
        assert!(props.get("title").is_some());
        assert!(props.get("content").is_some());
        assert!(props.get("tags").is_some());
    }

    #[test]
    fn test_read_tool_schema() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ReadMemoryTool::new(test_store(&temp_dir));

        assert_eq!(tool.name(), "read_memory");
        assert!(!tool.description().is_empty());

        let schema = tool.parameters_schema();
        let props = schema.get("properties").unwrap();
        assert!(props.get("id").is_some());
        assert!(props.get("tag").is_some());
        assert!(props.get("search").is_some());
    }

    #[test]
    fn test_list_tool_schema() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ListMemoryTool::new(test_store(&temp_dir));

        assert_eq!(tool.name(), "list_memories");
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn test_delete_tool_schema() {
        let temp_dir = TempDir::new().unwrap();
        let tool = DeleteMemoryTool::new(test_store(&temp_dir));

        assert_eq!(tool.name(), "delete_memory");
        assert!(!tool.description().is_empty());
    }

    #[tokio::test]
    async fn test_save_and_read_memory() {
        let temp_dir = TempDir::new().unwrap();
        let store = test_store(&temp_dir);

        let save_tool = SaveMemoryTool::new(store.clone());
        let read_tool = ReadMemoryTool::new(store);

        // Save a memory
        let save_result = save_tool
            .execute(json!({
                "agent_id": "test-agent",
                "title": "Test Memory",
                "content": "This is a test memory content",
                "tags": ["test", "important"]
            }))
            .await
            .unwrap();

        assert!(save_result.success);
        let id = save_result.result["id"].as_str().unwrap().to_string();

        // Read it back by ID
        let read_result = read_tool
            .execute(json!({ "agent_id": "test-agent", "id": id }))
            .await
            .unwrap();

        assert!(read_result.success);
        assert!(read_result.result["found"].as_bool().unwrap());
        let entry = &read_result.result["entry"];
        assert_eq!(entry["title"], "Test Memory");
        assert_eq!(entry["content"], "This is a test memory content");
    }

    #[tokio::test]
    async fn test_save_and_list_memories() {
        let temp_dir = TempDir::new().unwrap();
        let store = test_store(&temp_dir);

        let save_tool = SaveMemoryTool::new(store.clone());
        let list_tool = ListMemoryTool::new(store);

        // Save multiple memories
        save_tool
            .execute(json!({
                "agent_id": "test-agent",
                "title": "First Memory",
                "content": "Content 1",
                "tags": ["tag-a"]
            }))
            .await
            .unwrap();

        save_tool
            .execute(json!({
                "agent_id": "test-agent",
                "title": "Second Memory",
                "content": "Content 2",
                "tags": ["tag-b"]
            }))
            .await
            .unwrap();

        // List all
        let list_result = list_tool
            .execute(json!({"agent_id": "test-agent"}))
            .await
            .unwrap();

        assert!(list_result.success);
        assert_eq!(list_result.result["count"], 2);
        assert_eq!(list_result.result["total"], 2);
    }

    #[tokio::test]
    async fn test_search_by_tag() {
        let temp_dir = TempDir::new().unwrap();
        let store = test_store(&temp_dir);

        let save_tool = SaveMemoryTool::new(store.clone());
        let read_tool = ReadMemoryTool::new(store);

        save_tool
            .execute(json!({
                "agent_id": "test-agent",
                "title": "Important Note",
                "content": "Content",
                "tags": ["important", "work"]
            }))
            .await
            .unwrap();

        save_tool
            .execute(json!({
                "agent_id": "test-agent",
                "title": "Personal Note",
                "content": "Content",
                "tags": ["personal"]
            }))
            .await
            .unwrap();

        let search_result = read_tool
            .execute(json!({ "agent_id": "test-agent", "tag": "important" }))
            .await
            .unwrap();

        assert!(search_result.success);
        assert_eq!(search_result.result["count"], 1);
    }

    #[tokio::test]
    async fn test_search_by_title() {
        let temp_dir = TempDir::new().unwrap();
        let store = test_store(&temp_dir);

        let save_tool = SaveMemoryTool::new(store.clone());
        let read_tool = ReadMemoryTool::new(store);

        save_tool
            .execute(json!({
                "agent_id": "test-agent",
                "title": "Meeting Notes for Project Alpha",
                "content": "Content"
            }))
            .await
            .unwrap();

        save_tool
            .execute(json!({
                "agent_id": "test-agent",
                "title": "Random Thought",
                "content": "Content"
            }))
            .await
            .unwrap();

        let search_result = read_tool
            .execute(json!({ "agent_id": "test-agent", "search": "project" }))
            .await
            .unwrap();

        assert!(search_result.success);
        assert_eq!(search_result.result["count"], 1);
    }

    #[tokio::test]
    async fn test_delete_memory() {
        let temp_dir = TempDir::new().unwrap();
        let store = test_store(&temp_dir);

        let save_tool = SaveMemoryTool::new(store.clone());
        let delete_tool = DeleteMemoryTool::new(store.clone());
        let list_tool = ListMemoryTool::new(store);

        let save_result = save_tool
            .execute(json!({
                "agent_id": "test-agent",
                "title": "To Be Deleted",
                "content": "Content"
            }))
            .await
            .unwrap();

        let id = save_result.result["id"].as_str().unwrap().to_string();

        let list_before = list_tool
            .execute(json!({"agent_id": "test-agent"}))
            .await
            .unwrap();
        assert_eq!(list_before.result["count"], 1);

        let delete_result = delete_tool.execute(json!({ "id": id })).await.unwrap();

        assert!(delete_result.success);
        assert!(delete_result.result["deleted"].as_bool().unwrap());

        let list_after = list_tool
            .execute(json!({"agent_id": "test-agent"}))
            .await
            .unwrap();
        assert_eq!(list_after.result["count"], 0);
    }

    #[tokio::test]
    async fn test_read_nonexistent_memory() {
        let temp_dir = TempDir::new().unwrap();
        let store = test_store(&temp_dir);
        let read_tool = ReadMemoryTool::new(store);

        let result = read_tool
            .execute(json!({ "agent_id": "test-agent", "id": "nonexistent-id" }))
            .await
            .unwrap();

        assert!(result.success);
        assert!(!result.result["found"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_memory() {
        let temp_dir = TempDir::new().unwrap();
        let store = test_store(&temp_dir);
        let delete_tool = DeleteMemoryTool::new(store);

        let result = delete_tool
            .execute(json!({ "id": "nonexistent-id" }))
            .await
            .unwrap();

        assert!(result.success);
        assert!(!result.result["deleted"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn test_session_isolation() {
        let temp_dir = TempDir::new().unwrap();

        let config1 = FileMemoryConfig::new(temp_dir.path(), "agent").with_session("session-1");
        let config2 = FileMemoryConfig::new(temp_dir.path(), "agent").with_session("session-2");

        let store1: Arc<dyn MemoryStore> = Arc::new(FileMemoryStore::new(config1));
        let store2: Arc<dyn MemoryStore> = Arc::new(FileMemoryStore::new(config2));

        let save1 = SaveMemoryTool::new(store1.clone());
        let list1 = ListMemoryTool::new(store1);

        let save2 = SaveMemoryTool::new(store2.clone());
        let list2 = ListMemoryTool::new(store2);

        save1
            .execute(json!({
                "agent_id": "agent",
                "title": "Session 1 Memory",
                "content": "Content"
            }))
            .await
            .unwrap();

        save2
            .execute(json!({
                "agent_id": "agent",
                "title": "Session 2 Memory",
                "content": "Content"
            }))
            .await
            .unwrap();

        let list1_result = list1.execute(json!({"agent_id": "agent"})).await.unwrap();
        let list2_result = list2.execute(json!({"agent_id": "agent"})).await.unwrap();

        assert_eq!(list1_result.result["count"], 1);
        assert_eq!(list2_result.result["count"], 1);
    }
}
