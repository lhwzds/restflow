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

use crate::Result;
use crate::{Tool, ToolOutput};
use restflow_ai::tools::store_traits::MemoryStore;

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
        "Store a structured persistent memory entry with title, content, and optional tags. Use this for facts, decisions, and long-term knowledge."
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
            Err(e) => Ok(ToolOutput::error(format!(
                "Failed to save memory: {e}. The storage may be full or temporarily unavailable. Retry the operation."
            ))),
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
        "Retrieve stored memory entries by id, tag, or title keyword. Returns structured entries with metadata. Use this for specific lookups and tag-based retrieval."
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
                Err(e) => Ok(ToolOutput::error(format!(
                    "Memory not found or read error: {e}. Use list_memories to check available entries."
                ))),
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
                "Memory search failed: {e}. Try with different search terms or use list_memories instead."
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
            Err(e) => Ok(ToolOutput::error(format!(
                "Failed to list memories: {e}. Storage may be temporarily unavailable."
            ))),
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
            Err(e) => Ok(ToolOutput::error(format!(
                "Failed to delete memory: {e}. Verify the memory key exists using read_memory first."
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_ai::error::AiError;

    struct FailingMemoryStore;

    impl MemoryStore for FailingMemoryStore {
        fn save(
            &self,
            _agent_id: &str,
            _title: &str,
            _content: &str,
            _tags: &[String],
        ) -> Result<Value> {
            Err(crate::ToolError::Tool("db down".to_string()))
        }

        fn read_by_id(&self, _id: &str) -> Result<Option<Value>> {
            Err(crate::ToolError::Tool("db down".to_string()))
        }

        fn search(
            &self,
            _agent_id: &str,
            _tag: Option<&str>,
            _search: Option<&str>,
            _limit: usize,
        ) -> Result<Value> {
            Err(crate::ToolError::Tool("db down".to_string()))
        }

        fn list(&self, _agent_id: &str, _tag: Option<&str>, _limit: usize) -> Result<Value> {
            Err(crate::ToolError::Tool("db down".to_string()))
        }

        fn delete(&self, _id: &str) -> Result<Value> {
            Err(crate::ToolError::Tool("db down".to_string()))
        }
    }

    #[tokio::test]
    async fn test_save_memory_error_message() {
        let tool = SaveMemoryTool::new(Arc::new(FailingMemoryStore));
        let output = tool
            .execute(json!({
                "agent_id": "agent-1",
                "title": "title",
                "content": "content"
            }))
            .await
            .expect("tool should return error output");
        assert!(!output.success);
        assert!(
            output
                .error
                .expect("expected error")
                .contains("The storage may be full or temporarily unavailable")
        );
    }

    #[tokio::test]
    async fn test_read_memory_error_message() {
        let tool = ReadMemoryTool::new(Arc::new(FailingMemoryStore));
        let output = tool
            .execute(json!({
                "agent_id": "agent-1",
                "id": "memory-1"
            }))
            .await
            .expect("tool should return error output");
        assert!(!output.success);
        assert!(
            output
                .error
                .expect("expected error")
                .contains("Use list_memories to check available entries")
        );
    }

    #[tokio::test]
    async fn test_list_memory_error_message() {
        let tool = ListMemoryTool::new(Arc::new(FailingMemoryStore));
        let output = tool
            .execute(json!({
                "agent_id": "agent-1"
            }))
            .await
            .expect("tool should return error output");
        assert!(!output.success);
        assert!(
            output
                .error
                .expect("expected error")
                .contains("Storage may be temporarily unavailable")
        );
    }
}
