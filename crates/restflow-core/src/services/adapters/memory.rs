//! Memory adapters: MemoryManager and MemoryStore backed by MemoryStorage.

use crate::memory::MemoryExporter;
use crate::storage::MemoryStorage;
use restflow_ai::tools::{MemoryClearRequest, MemoryCompactRequest, MemoryExportRequest, MemoryManager, MemoryStore};
use restflow_tools::ToolError;
use serde_json::{Value, json};

// ============== Memory Manager Adapter ==============

#[derive(Clone)]
pub struct MemoryManagerAdapter {
    storage: MemoryStorage,
}

impl MemoryManagerAdapter {
    pub fn new(storage: MemoryStorage) -> Self {
        Self { storage }
    }
}

impl MemoryManager for MemoryManagerAdapter {
    fn stats(&self, agent_id: &str) -> restflow_tools::Result<Value> {
        let stats = self
            .storage
            .get_stats(agent_id)
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        serde_json::to_value(stats).map_err(ToolError::from)
    }

    fn export(&self, request: MemoryExportRequest) -> restflow_tools::Result<Value> {
        let exporter = MemoryExporter::new(self.storage.clone());
        let result = if let Some(session_id) = &request.session_id {
            exporter
                .export_session(session_id)
                .map_err(|e| ToolError::Tool(e.to_string()))?
        } else {
            exporter
                .export_agent(&request.agent_id)
                .map_err(|e| ToolError::Tool(e.to_string()))?
        };
        serde_json::to_value(result).map_err(ToolError::from)
    }

    fn clear(&self, request: MemoryClearRequest) -> restflow_tools::Result<Value> {
        if let Some(session_id) = &request.session_id {
            let delete_chunks = request.delete_sessions.unwrap_or(true);
            let deleted = self
                .storage
                .delete_session(session_id, delete_chunks)
                .map_err(|e| ToolError::Tool(e.to_string()))?;
            Ok(json!({
                "agent_id": request.agent_id,
                "session_id": session_id,
                "deleted": deleted
            }))
        } else {
            let deleted = self
                .storage
                .delete_chunks_for_agent(&request.agent_id)
                .map_err(|e| ToolError::Tool(e.to_string()))?;
            Ok(json!({
                "agent_id": request.agent_id,
                "chunks_deleted": deleted
            }))
        }
    }

    fn compact(&self, request: MemoryCompactRequest) -> restflow_tools::Result<Value> {
        let chunks = self
            .storage
            .list_chunks(&request.agent_id)
            .map_err(|e| ToolError::Tool(e.to_string()))?;

        let keep_recent = request.keep_recent.unwrap_or(10) as usize;
        let before_ms = request.before_ms;

        let mut to_delete: Vec<String> = Vec::new();

        if chunks.len() > keep_recent {
            let mut sorted = chunks.clone();
            sorted.sort_by_key(|c| c.created_at);

            let removable = sorted.len() - keep_recent;
            for chunk in sorted.into_iter().take(removable) {
                if let Some(threshold) = before_ms {
                    if chunk.created_at < threshold {
                        to_delete.push(chunk.id.clone());
                    }
                } else {
                    to_delete.push(chunk.id.clone());
                }
            }
        }

        let deleted_count = to_delete.len();
        for chunk_id in &to_delete {
            self.storage
                .delete_chunk(chunk_id)
                .map_err(|e| ToolError::Tool(e.to_string()))?;
        }

        Ok(json!({
            "agent_id": request.agent_id,
            "total_chunks": chunks.len(),
            "deleted": deleted_count,
            "remaining": chunks.len() - deleted_count
        }))
    }
}

// ============== DB Memory Store Adapter ==============

/// Database-backed implementation of MemoryStore.
///
/// Stores memories as MemoryChunks in the redb database, enabling interoperability
/// with memory_search and manage_memory tools. Title is stored as a `__title:{value}` tag.
#[derive(Clone)]
pub struct DbMemoryStoreAdapter {
    storage: MemoryStorage,
}

impl DbMemoryStoreAdapter {
    pub fn new(storage: MemoryStorage) -> Self {
        Self { storage }
    }

    /// Extract title from tags (stored as `__title:{value}`)
    fn extract_title(tags: &[String]) -> String {
        tags.iter()
            .find(|t| t.starts_with("__title:"))
            .map(|t| t.trim_start_matches("__title:").to_string())
            .unwrap_or_default()
    }

    /// Build tags list: prepend __title tag, then user tags
    fn build_tags(title: &str, user_tags: &[String]) -> Vec<String> {
        let mut tags = vec![format!("__title:{}", title)];
        tags.extend(user_tags.iter().cloned());
        tags
    }

    /// Filter out internal __title tags from user-visible output
    fn user_tags(tags: &[String]) -> Vec<String> {
        tags.iter()
            .filter(|t| !t.starts_with("__title:"))
            .cloned()
            .collect()
    }

    /// Format a MemoryChunk as a memory entry JSON (matching file memory output)
    fn chunk_to_entry_json(chunk: &crate::models::memory::MemoryChunk) -> Value {
        let title = Self::extract_title(&chunk.tags);
        let user_tags = Self::user_tags(&chunk.tags);
        json!({
            "id": chunk.id,
            "title": title,
            "content": chunk.content,
            "tags": user_tags,
            "created_at": chrono::DateTime::from_timestamp_millis(chunk.created_at)
                .unwrap_or_default()
                .to_rfc3339(),
            "updated_at": chrono::DateTime::from_timestamp_millis(chunk.created_at)
                .unwrap_or_default()
                .to_rfc3339(),
            "agent_id": chunk.agent_id,
            "session_id": chunk.session_id,
        })
    }

    /// Format a MemoryChunk as metadata-only JSON (for list operations)
    fn chunk_to_meta_json(chunk: &crate::models::memory::MemoryChunk) -> Value {
        let title = Self::extract_title(&chunk.tags);
        let user_tags = Self::user_tags(&chunk.tags);
        json!({
            "id": chunk.id,
            "title": title,
            "tags": user_tags,
            "created_at": chrono::DateTime::from_timestamp_millis(chunk.created_at)
                .unwrap_or_default()
                .to_rfc3339(),
            "updated_at": chrono::DateTime::from_timestamp_millis(chunk.created_at)
                .unwrap_or_default()
                .to_rfc3339(),
        })
    }
}

impl MemoryStore for DbMemoryStoreAdapter {
    fn save(
        &self,
        agent_id: &str,
        title: &str,
        content: &str,
        tags: &[String],
    ) -> restflow_tools::Result<Value> {
        use crate::models::memory::MemorySource;

        let db_tags = Self::build_tags(title, tags);
        let chunk =
            crate::models::memory::MemoryChunk::new(agent_id.to_string(), content.to_string())
                .with_tags(db_tags)
                .with_source(MemorySource::AgentGenerated {
                    tool_name: "save_to_memory".to_string(),
                });

        let stored_id = self
            .storage
            .store_chunk(&chunk)
            .map_err(|e| ToolError::Tool(e.to_string()))?;

        let is_dedup = stored_id != chunk.id;
        let message = if is_dedup {
            "Duplicate content, returning existing memory"
        } else {
            "Memory saved successfully"
        };

        Ok(json!({
            "success": true,
            "id": stored_id,
            "title": title,
            "message": message
        }))
    }

    fn read_by_id(&self, id: &str) -> restflow_tools::Result<Option<Value>> {
        let chunk = self
            .storage
            .get_chunk(id)
            .map_err(|e| ToolError::Tool(e.to_string()))?;

        match chunk {
            Some(c) => {
                let entry = Self::chunk_to_entry_json(&c);
                Ok(Some(json!({
                    "found": true,
                    "entry": entry
                })))
            }
            None => Ok(None),
        }
    }

    fn search(
        &self,
        agent_id: &str,
        tag: Option<&str>,
        search: Option<&str>,
        limit: usize,
    ) -> restflow_tools::Result<Value> {
        let mut chunks = self
            .storage
            .list_chunks(agent_id)
            .map_err(|e| ToolError::Tool(e.to_string()))?;

        if let Some(tag_filter) = tag {
            let tag_lower = tag_filter.to_lowercase();
            chunks.retain(|c| {
                Self::user_tags(&c.tags)
                    .iter()
                    .any(|t| t.to_lowercase().contains(&tag_lower))
            });
        }

        if let Some(search_text) = search {
            let search_lower = search_text.to_lowercase();
            chunks.retain(|c| {
                Self::extract_title(&c.tags)
                    .to_lowercase()
                    .contains(&search_lower)
            });
        }

        chunks.truncate(limit);

        let results: Vec<Value> = chunks.iter().map(Self::chunk_to_meta_json).collect();

        Ok(json!({
            "count": results.len(),
            "memories": results
        }))
    }

    fn list(
        &self,
        agent_id: &str,
        tag: Option<&str>,
        limit: usize,
    ) -> restflow_tools::Result<Value> {
        let chunks = self
            .storage
            .list_chunks(agent_id)
            .map_err(|e| ToolError::Tool(e.to_string()))?;

        let total = chunks.len();
        let mut filtered = chunks;

        if let Some(tag_filter) = tag {
            let tag_lower = tag_filter.to_lowercase();
            filtered.retain(|c| {
                Self::user_tags(&c.tags)
                    .iter()
                    .any(|t| t.to_lowercase().contains(&tag_lower))
            });
        }

        filtered.truncate(limit);

        let results: Vec<Value> = filtered.iter().map(Self::chunk_to_meta_json).collect();

        Ok(json!({
            "total": total,
            "count": results.len(),
            "memories": results
        }))
    }

    fn delete(&self, id: &str) -> restflow_tools::Result<Value> {
        let deleted = self
            .storage
            .delete_chunk(id)
            .map_err(|e| ToolError::Tool(e.to_string()))?;

        if deleted {
            Ok(json!({
                "deleted": true,
                "id": id,
                "message": "Memory deleted successfully"
            }))
        } else {
            Ok(json!({
                "deleted": false,
                "message": format!("No memory found with ID: {}", id)
            }))
        }
    }
}
