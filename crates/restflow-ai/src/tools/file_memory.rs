//! File-based memory tools for persisting agent context
//!
//! This module provides tools that allow AI agents to save important context
//! and information to files, organized by agent_id and session_id.
//! This enables agents to externalize knowledge that should persist beyond
//! the working memory window.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
use tokio::fs;

use crate::error::Result;
use crate::tools::traits::{Tool, ToolOutput};

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

/// Configuration for file memory tools
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

/// Tool for saving important context to file-based memory
///
/// Allows agents to persist knowledge that should survive beyond the
/// working memory window. Memories are organized by agent_id and
/// optionally by session_id.
pub struct SaveMemoryTool {
    config: FileMemoryConfig,
}

impl SaveMemoryTool {
    /// Create a new SaveMemoryTool with the given config
    pub fn new(config: FileMemoryConfig) -> Self {
        Self { config }
    }

    /// Generate a unique ID for a memory entry
    fn generate_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    /// Load the index file
    async fn load_index(&self) -> Result<Vec<MemoryEntryMeta>> {
        let path = self.config.index_path();
        if path.exists() {
            let content = fs::read_to_string(&path).await?;
            let index: Vec<MemoryEntryMeta> = serde_json::from_str(&content)?;
            Ok(index)
        } else {
            Ok(Vec::new())
        }
    }

    /// Save the index file
    async fn save_index(&self, index: &[MemoryEntryMeta]) -> Result<()> {
        let path = self.config.index_path();
        let content = serde_json::to_string_pretty(index)?;
        fs::write(&path, content).await?;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct SaveMemoryInput {
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
            "required": ["title", "content"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: SaveMemoryInput = match serde_json::from_value(input) {
            Ok(p) => p,
            Err(e) => return Ok(ToolOutput::error(format!("Invalid input: {}", e))),
        };

        // Ensure directory exists
        let dir = self.config.memory_dir();
        if let Err(e) = fs::create_dir_all(&dir).await {
            return Ok(ToolOutput::error(format!(
                "Failed to create memory directory: {}",
                e
            )));
        }

        let now = Utc::now();
        let id = Self::generate_id();

        let entry = MemoryEntry {
            meta: MemoryEntryMeta {
                id: id.clone(),
                title: params.title.clone(),
                tags: params.tags.clone(),
                created_at: now,
                updated_at: now,
                agent_id: self.config.agent_id.clone(),
                session_id: self.config.session_id.clone(),
            },
            content: params.content,
        };

        // Save the entry
        let entry_path = self.config.entry_path(&id);
        let entry_json = match serde_json::to_string_pretty(&entry) {
            Ok(j) => j,
            Err(e) => {
                return Ok(ToolOutput::error(format!(
                    "Failed to serialize entry: {}",
                    e
                )));
            }
        };

        if let Err(e) = fs::write(&entry_path, entry_json).await {
            return Ok(ToolOutput::error(format!("Failed to write entry: {}", e)));
        }

        // Update index
        let mut index = self.load_index().await.unwrap_or_default();
        index.push(entry.meta.clone());
        if let Err(e) = self.save_index(&index).await {
            // Non-fatal: entry is saved, just index failed
            tracing::warn!("Failed to update memory index: {}", e);
        }

        Ok(ToolOutput::success(json!({
            "success": true,
            "id": id,
            "title": params.title,
            "message": "Memory saved successfully"
        })))
    }
}

/// Tool for reading saved memories
///
/// Allows agents to retrieve previously saved context and information.
pub struct ReadMemoryTool {
    config: FileMemoryConfig,
}

impl ReadMemoryTool {
    /// Create a new ReadMemoryTool with the given config
    pub fn new(config: FileMemoryConfig) -> Self {
        Self { config }
    }

    /// Load the index file
    async fn load_index(&self) -> Result<Vec<MemoryEntryMeta>> {
        let path = self.config.index_path();
        if path.exists() {
            let content = fs::read_to_string(&path).await?;
            let index: Vec<MemoryEntryMeta> = serde_json::from_str(&content)?;
            Ok(index)
        } else {
            Ok(Vec::new())
        }
    }

    /// Load a specific entry by ID
    async fn load_entry(&self, id: &str) -> Result<Option<MemoryEntry>> {
        let path = self.config.entry_path(id);
        if path.exists() {
            let content = fs::read_to_string(&path).await?;
            let entry: MemoryEntry = serde_json::from_str(&content)?;
            Ok(Some(entry))
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug, Deserialize)]
struct ReadMemoryInput {
    /// Specific ID to read (optional)
    id: Option<String>,
    /// Search by tag (optional)
    tag: Option<String>,
    /// Search in title (optional)
    search: Option<String>,
    /// Maximum number of results
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
            }
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: ReadMemoryInput = match serde_json::from_value(input) {
            Ok(p) => p,
            Err(e) => return Ok(ToolOutput::error(format!("Invalid input: {}", e))),
        };

        // If specific ID requested, return that entry
        if let Some(ref id) = params.id {
            return match self.load_entry(id).await {
                Ok(Some(entry)) => Ok(ToolOutput::success(json!({
                    "found": true,
                    "entry": entry
                }))),
                Ok(None) => Ok(ToolOutput::success(json!({
                    "found": false,
                    "message": format!("No memory found with ID: {}", id)
                }))),
                Err(e) => Ok(ToolOutput::error(format!("Failed to read memory: {}", e))),
            };
        }

        // Load index and filter
        let index = match self.load_index().await {
            Ok(i) => i,
            Err(e) => return Ok(ToolOutput::error(format!("Failed to load index: {}", e))),
        };

        let mut results: Vec<&MemoryEntryMeta> = index.iter().collect();

        // Filter by tag
        if let Some(ref tag) = params.tag {
            let tag_lower = tag.to_lowercase();
            results.retain(|m| m.tags.iter().any(|t| t.to_lowercase().contains(&tag_lower)));
        }

        // Filter by search term in title
        if let Some(ref search) = params.search {
            let search_lower = search.to_lowercase();
            results.retain(|m| m.title.to_lowercase().contains(&search_lower));
        }

        // Sort by updated_at descending (most recent first)
        results.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        // Apply limit
        results.truncate(params.limit);

        Ok(ToolOutput::success(json!({
            "count": results.len(),
            "memories": results
        })))
    }
}

/// Tool for listing all saved memories
pub struct ListMemoryTool {
    config: FileMemoryConfig,
}

impl ListMemoryTool {
    /// Create a new ListMemoryTool with the given config
    pub fn new(config: FileMemoryConfig) -> Self {
        Self { config }
    }

    /// Load the index file
    async fn load_index(&self) -> Result<Vec<MemoryEntryMeta>> {
        let path = self.config.index_path();
        if path.exists() {
            let content = fs::read_to_string(&path).await?;
            let index: Vec<MemoryEntryMeta> = serde_json::from_str(&content)?;
            Ok(index)
        } else {
            Ok(Vec::new())
        }
    }
}

#[derive(Debug, Deserialize)]
struct ListMemoryInput {
    /// Filter by tag (optional)
    tag: Option<String>,
    /// Maximum number of results
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
                "tag": {
                    "type": "string",
                    "description": "Optional tag to filter by"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results (default: 50)",
                    "default": 50
                }
            }
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: ListMemoryInput = match serde_json::from_value(input) {
            Ok(p) => p,
            Err(e) => return Ok(ToolOutput::error(format!("Invalid input: {}", e))),
        };

        let index = match self.load_index().await {
            Ok(i) => i,
            Err(e) => return Ok(ToolOutput::error(format!("Failed to load index: {}", e))),
        };

        let mut results: Vec<&MemoryEntryMeta> = index.iter().collect();

        // Filter by tag
        if let Some(ref tag) = params.tag {
            let tag_lower = tag.to_lowercase();
            results.retain(|m| m.tags.iter().any(|t| t.to_lowercase().contains(&tag_lower)));
        }

        // Sort by updated_at descending
        results.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        // Apply limit
        results.truncate(params.limit);

        Ok(ToolOutput::success(json!({
            "total": index.len(),
            "count": results.len(),
            "memories": results.iter().map(|m| json!({
                "id": m.id,
                "title": m.title,
                "tags": m.tags,
                "created_at": m.created_at,
                "updated_at": m.updated_at
            })).collect::<Vec<_>>()
        })))
    }
}

/// Tool for deleting a memory entry
pub struct DeleteMemoryTool {
    config: FileMemoryConfig,
}

impl DeleteMemoryTool {
    /// Create a new DeleteMemoryTool with the given config
    pub fn new(config: FileMemoryConfig) -> Self {
        Self { config }
    }

    /// Load the index file
    async fn load_index(&self) -> Result<Vec<MemoryEntryMeta>> {
        let path = self.config.index_path();
        if path.exists() {
            let content = fs::read_to_string(&path).await?;
            let index: Vec<MemoryEntryMeta> = serde_json::from_str(&content)?;
            Ok(index)
        } else {
            Ok(Vec::new())
        }
    }

    /// Save the index file
    async fn save_index(&self, index: &[MemoryEntryMeta]) -> Result<()> {
        let path = self.config.index_path();
        let content = serde_json::to_string_pretty(index)?;
        fs::write(&path, content).await?;
        Ok(())
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

        let entry_path = self.config.entry_path(&params.id);

        // Check if entry exists
        if !entry_path.exists() {
            return Ok(ToolOutput::success(json!({
                "deleted": false,
                "message": format!("No memory found with ID: {}", params.id)
            })));
        }

        // Delete the file
        if let Err(e) = fs::remove_file(&entry_path).await {
            return Ok(ToolOutput::error(format!("Failed to delete memory: {}", e)));
        }

        // Update index
        let mut index = self.load_index().await.unwrap_or_default();
        index.retain(|m| m.id != params.id);
        if let Err(e) = self.save_index(&index).await {
            tracing::warn!("Failed to update memory index after delete: {}", e);
        }

        Ok(ToolOutput::success(json!({
            "deleted": true,
            "id": params.id,
            "message": "Memory deleted successfully"
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_config(temp_dir: &TempDir) -> FileMemoryConfig {
        FileMemoryConfig::new(temp_dir.path(), "test-agent")
    }

    #[test]
    fn test_config_memory_dir() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);

        let dir = config.memory_dir();
        assert!(dir.ends_with("memory/test-agent"));

        let config_with_session = config.with_session("session-123");
        let dir_with_session = config_with_session.memory_dir();
        assert!(dir_with_session.ends_with("memory/test-agent/session-123"));
    }

    #[test]
    fn test_config_entry_path() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);

        let path = config.entry_path("entry-id");
        assert!(path.ends_with("entry-id.json"));
    }

    #[test]
    fn test_save_tool_schema() {
        let temp_dir = TempDir::new().unwrap();
        let tool = SaveMemoryTool::new(test_config(&temp_dir));

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
        let tool = ReadMemoryTool::new(test_config(&temp_dir));

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
        let tool = ListMemoryTool::new(test_config(&temp_dir));

        assert_eq!(tool.name(), "list_memories");
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn test_delete_tool_schema() {
        let temp_dir = TempDir::new().unwrap();
        let tool = DeleteMemoryTool::new(test_config(&temp_dir));

        assert_eq!(tool.name(), "delete_memory");
        assert!(!tool.description().is_empty());
    }

    #[tokio::test]
    async fn test_save_and_read_memory() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);

        let save_tool = SaveMemoryTool::new(config.clone());
        let read_tool = ReadMemoryTool::new(config.clone());

        // Save a memory
        let save_result = save_tool
            .execute(json!({
                "title": "Test Memory",
                "content": "This is a test memory content",
                "tags": ["test", "important"]
            }))
            .await
            .unwrap();

        assert!(save_result.success);
        let id = save_result.result["id"].as_str().unwrap().to_string();

        // Read it back by ID
        let read_result = read_tool.execute(json!({ "id": id })).await.unwrap();

        assert!(read_result.success);
        assert!(read_result.result["found"].as_bool().unwrap());
        let entry = &read_result.result["entry"];
        assert_eq!(entry["title"], "Test Memory");
        assert_eq!(entry["content"], "This is a test memory content");
    }

    #[tokio::test]
    async fn test_save_and_list_memories() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);

        let save_tool = SaveMemoryTool::new(config.clone());
        let list_tool = ListMemoryTool::new(config.clone());

        // Save multiple memories
        save_tool
            .execute(json!({
                "title": "First Memory",
                "content": "Content 1",
                "tags": ["tag-a"]
            }))
            .await
            .unwrap();

        save_tool
            .execute(json!({
                "title": "Second Memory",
                "content": "Content 2",
                "tags": ["tag-b"]
            }))
            .await
            .unwrap();

        // List all
        let list_result = list_tool.execute(json!({})).await.unwrap();

        assert!(list_result.success);
        assert_eq!(list_result.result["count"], 2);
        assert_eq!(list_result.result["total"], 2);
    }

    #[tokio::test]
    async fn test_search_by_tag() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);

        let save_tool = SaveMemoryTool::new(config.clone());
        let read_tool = ReadMemoryTool::new(config.clone());

        // Save memories with different tags
        save_tool
            .execute(json!({
                "title": "Important Note",
                "content": "Content",
                "tags": ["important", "work"]
            }))
            .await
            .unwrap();

        save_tool
            .execute(json!({
                "title": "Personal Note",
                "content": "Content",
                "tags": ["personal"]
            }))
            .await
            .unwrap();

        // Search by tag
        let search_result = read_tool
            .execute(json!({ "tag": "important" }))
            .await
            .unwrap();

        assert!(search_result.success);
        assert_eq!(search_result.result["count"], 1);
    }

    #[tokio::test]
    async fn test_search_by_title() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);

        let save_tool = SaveMemoryTool::new(config.clone());
        let read_tool = ReadMemoryTool::new(config.clone());

        // Save memories
        save_tool
            .execute(json!({
                "title": "Meeting Notes for Project Alpha",
                "content": "Content"
            }))
            .await
            .unwrap();

        save_tool
            .execute(json!({
                "title": "Random Thought",
                "content": "Content"
            }))
            .await
            .unwrap();

        // Search in title
        let search_result = read_tool
            .execute(json!({ "search": "project" }))
            .await
            .unwrap();

        assert!(search_result.success);
        assert_eq!(search_result.result["count"], 1);
    }

    #[tokio::test]
    async fn test_delete_memory() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);

        let save_tool = SaveMemoryTool::new(config.clone());
        let delete_tool = DeleteMemoryTool::new(config.clone());
        let list_tool = ListMemoryTool::new(config.clone());

        // Save a memory
        let save_result = save_tool
            .execute(json!({
                "title": "To Be Deleted",
                "content": "Content"
            }))
            .await
            .unwrap();

        let id = save_result.result["id"].as_str().unwrap().to_string();

        // Verify it exists
        let list_before = list_tool.execute(json!({})).await.unwrap();
        assert_eq!(list_before.result["count"], 1);

        // Delete it
        let delete_result = delete_tool.execute(json!({ "id": id })).await.unwrap();

        assert!(delete_result.success);
        assert!(delete_result.result["deleted"].as_bool().unwrap());

        // Verify it's gone
        let list_after = list_tool.execute(json!({})).await.unwrap();
        assert_eq!(list_after.result["count"], 0);
    }

    #[tokio::test]
    async fn test_read_nonexistent_memory() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);

        let read_tool = ReadMemoryTool::new(config);

        let result = read_tool
            .execute(json!({ "id": "nonexistent-id" }))
            .await
            .unwrap();

        assert!(result.success);
        assert!(!result.result["found"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_memory() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);

        let delete_tool = DeleteMemoryTool::new(config);

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

        let save1 = SaveMemoryTool::new(config1.clone());
        let list1 = ListMemoryTool::new(config1);

        let save2 = SaveMemoryTool::new(config2.clone());
        let list2 = ListMemoryTool::new(config2);

        // Save to session 1
        save1
            .execute(json!({
                "title": "Session 1 Memory",
                "content": "Content"
            }))
            .await
            .unwrap();

        // Save to session 2
        save2
            .execute(json!({
                "title": "Session 2 Memory",
                "content": "Content"
            }))
            .await
            .unwrap();

        // Each session should only see its own memories
        let list1_result = list1.execute(json!({})).await.unwrap();
        let list2_result = list2.execute(json!({})).await.unwrap();

        assert_eq!(list1_result.result["count"], 1);
        assert_eq!(list2_result.result["count"], 1);
    }
}
