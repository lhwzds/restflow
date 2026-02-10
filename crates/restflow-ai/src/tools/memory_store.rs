//! Memory tools for persisting agent context
//!
//! This module provides tools that allow AI agents to save important context
//! and information, organized by agent_id and session_id.
//! The storage backend is abstracted via the `MemoryStore` trait;
//! the production implementation is `DbMemoryStoreAdapter` in restflow-core.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::error::Result;
use crate::tools::traits::{Tool, ToolOutput};

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
