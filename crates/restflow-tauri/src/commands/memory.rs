//! Memory Tauri commands
//!
//! Provides IPC commands for managing long-term agent memory from the frontend.
//!
//! # Overview
//!
//! This module exposes the memory system to the frontend, including:
//! - Searching memories with relevance scoring
//! - Viewing memory statistics
//! - Exporting memories to Markdown
//! - Managing memory chunks and sessions
//!
//! # Example (TypeScript)
//!
//! ```typescript
//! import { invoke } from '@tauri-apps/api/core';
//! import type { MemorySearchQuery, RankedSearchResult, MemoryStats } from './types/generated';
//!
//! // Search memories
//! const query: MemorySearchQuery = {
//!   agent_id: 'my-agent',
//!   query: 'rust async',
//!   mode: 'keyword',
//!   limit: 20,
//! };
//! const results: RankedSearchResult = await invoke('search_memory', { query });
//!
//! // Get stats
//! const stats: MemoryStats = await invoke('get_memory_stats', { agentId: 'my-agent' });
//!
//! // Export to Markdown
//! const markdown: string = await invoke('export_memory_markdown', { agentId: 'my-agent' });
//! ```

use crate::state::AppState;
use restflow_core::memory::{
    ExportResult, MemoryExporter, MemoryExporterBuilder, RankedSearchResult, SearchEngine,
    SearchEngineBuilder,
};
use restflow_core::models::memory::{
    MemoryChunk, MemorySearchQuery, MemorySession, MemorySource, MemoryStats,
};
use serde::{Deserialize, Serialize};
use tauri::State;

// ============================================================================
// Request/Response Types
// ============================================================================

/// Request to search memories with optional scoring configuration.
#[derive(Debug, Deserialize)]
pub struct SearchMemoryRequest {
    /// The search query
    pub query: MemorySearchQuery,
    /// Optional minimum score threshold (0-100)
    #[serde(default)]
    pub min_score: Option<f64>,
    /// Optional scoring config preset: "default", "frequency_focused", "recency_focused", "balanced"
    #[serde(default)]
    pub scoring_preset: Option<String>,
}

/// Request to create a memory chunk manually.
#[derive(Debug, Deserialize)]
pub struct CreateMemoryChunkRequest {
    /// Agent ID this memory belongs to
    pub agent_id: String,
    /// The memory content
    pub content: String,
    /// Optional session ID
    #[serde(default)]
    pub session_id: Option<String>,
    /// Optional tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Request to create a memory session.
#[derive(Debug, Deserialize)]
pub struct CreateMemorySessionRequest {
    /// Agent ID this session belongs to
    pub agent_id: String,
    /// Session name
    pub name: String,
    /// Optional description
    #[serde(default)]
    pub description: Option<String>,
    /// Optional tags for the session
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Request to export memories with options.
#[derive(Debug, Deserialize)]
pub struct ExportMemoryRequest {
    /// Agent ID to export
    pub agent_id: String,
    /// Optional session ID to export (if None, exports all)
    #[serde(default)]
    pub session_id: Option<String>,
    /// Export options preset: "default", "minimal", "compact"
    #[serde(default)]
    pub preset: Option<String>,
    /// Include metadata as HTML comments
    #[serde(default)]
    pub include_metadata: Option<bool>,
    /// Include timestamps
    #[serde(default)]
    pub include_timestamps: Option<bool>,
    /// Include source information
    #[serde(default)]
    pub include_source: Option<bool>,
    /// Include tags
    #[serde(default)]
    pub include_tags: Option<bool>,
}

/// Response for memory list operations with pagination info.
#[derive(Debug, Serialize)]
pub struct MemoryListResponse<T> {
    /// The items
    pub items: Vec<T>,
    /// Total count
    pub total: u32,
}

// ============================================================================
// Search Commands
// ============================================================================

/// Search memories with relevance scoring.
///
/// Returns ranked results based on keyword frequency, recency, and tag matches.
#[tauri::command]
pub async fn search_memory(
    state: State<'_, AppState>,
    query: MemorySearchQuery,
) -> Result<RankedSearchResult, String> {
    let storage = state.core.storage.memory.clone();
    let engine = SearchEngine::new(storage);
    engine.search_ranked(&query).map_err(|e| e.to_string())
}

/// Search memories with custom scoring configuration.
#[tauri::command]
pub async fn search_memory_advanced(
    state: State<'_, AppState>,
    request: SearchMemoryRequest,
) -> Result<RankedSearchResult, String> {
    let storage = state.core.storage.memory.clone();

    let mut builder = SearchEngineBuilder::new(storage);

    // Apply preset if specified
    builder = match request.scoring_preset.as_deref() {
        Some("frequency_focused") => builder.frequency_focused(),
        Some("recency_focused") => builder.recency_focused(),
        Some("balanced") => builder.balanced(),
        _ => builder, // Use default config
    };

    // Apply min_score if specified
    if let Some(min_score) = request.min_score {
        builder = builder.min_score(min_score);
    }

    let engine = builder.build();
    engine
        .search_ranked(&request.query)
        .map_err(|e| e.to_string())
}

// ============================================================================
// Chunk Commands
// ============================================================================

/// Get a memory chunk by ID.
#[tauri::command]
pub async fn get_memory_chunk(
    state: State<'_, AppState>,
    chunk_id: String,
) -> Result<Option<MemoryChunk>, String> {
    state
        .core
        .storage
        .memory
        .get_chunk(&chunk_id)
        .map_err(|e| e.to_string())
}

/// List all memory chunks for an agent.
#[tauri::command]
pub async fn list_memory_chunks(
    state: State<'_, AppState>,
    agent_id: String,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<MemoryListResponse<MemoryChunk>, String> {
    let chunks = state
        .core
        .storage
        .memory
        .list_chunks(&agent_id)
        .map_err(|e| e.to_string())?;

    let total = chunks.len() as u32;
    let offset = offset.unwrap_or(0) as usize;
    let limit = limit.unwrap_or(50) as usize;

    let items = chunks.into_iter().skip(offset).take(limit).collect();

    Ok(MemoryListResponse { items, total })
}

/// List memory chunks by tag.
#[tauri::command]
pub async fn list_memory_chunks_by_tag(
    state: State<'_, AppState>,
    tag: String,
    limit: Option<u32>,
) -> Result<MemoryListResponse<MemoryChunk>, String> {
    let chunks = state
        .core
        .storage
        .memory
        .list_chunks_by_tag(&tag)
        .map_err(|e| e.to_string())?;

    let total = chunks.len() as u32;
    let limit = limit.unwrap_or(50) as usize;

    let items = chunks.into_iter().take(limit).collect();

    Ok(MemoryListResponse { items, total })
}

/// Create a new memory chunk manually.
#[tauri::command]
pub async fn create_memory_chunk(
    state: State<'_, AppState>,
    request: CreateMemoryChunkRequest,
) -> Result<MemoryChunk, String> {
    let mut chunk = MemoryChunk::new(request.agent_id, request.content)
        .with_source(MemorySource::ManualNote)
        .with_tags(request.tags);

    if let Some(session_id) = request.session_id {
        chunk = chunk.with_session(session_id);
    }

    let chunk_id = state
        .core
        .storage
        .memory
        .store_chunk(&chunk)
        .map_err(|e| e.to_string())?;

    // Return the chunk with the (possibly deduplicated) ID
    if chunk_id != chunk.id {
        // Chunk was deduplicated, fetch the existing one
        state
            .core
            .storage
            .memory
            .get_chunk(&chunk_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Failed to retrieve stored chunk".to_string())
    } else {
        Ok(chunk)
    }
}

/// Delete a memory chunk by ID.
#[tauri::command]
pub async fn delete_memory_chunk(
    state: State<'_, AppState>,
    chunk_id: String,
) -> Result<bool, String> {
    state
        .core
        .storage
        .memory
        .delete_chunk(&chunk_id)
        .map_err(|e| e.to_string())
}

/// Delete all memory chunks for an agent.
#[tauri::command]
pub async fn delete_memory_chunks_for_agent(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<u32, String> {
    state
        .core
        .storage
        .memory
        .delete_chunks_for_agent(&agent_id)
        .map_err(|e| e.to_string())
}

// ============================================================================
// Session Commands
// ============================================================================

/// Get a memory session by ID.
#[tauri::command]
pub async fn get_memory_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Option<MemorySession>, String> {
    state
        .core
        .storage
        .memory
        .get_session(&session_id)
        .map_err(|e| e.to_string())
}

/// List all memory sessions for an agent.
#[tauri::command]
pub async fn list_memory_sessions(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<Vec<MemorySession>, String> {
    state
        .core
        .storage
        .memory
        .list_sessions(&agent_id)
        .map_err(|e| e.to_string())
}

/// List chunks for a specific session.
#[tauri::command]
pub async fn list_memory_chunks_for_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<MemoryChunk>, String> {
    state
        .core
        .storage
        .memory
        .list_chunks_for_session(&session_id)
        .map_err(|e| e.to_string())
}

/// Create a new memory session.
#[tauri::command]
pub async fn create_memory_session(
    state: State<'_, AppState>,
    request: CreateMemorySessionRequest,
) -> Result<MemorySession, String> {
    let mut session = MemorySession::new(request.agent_id, request.name).with_tags(request.tags);

    if let Some(description) = request.description {
        session = session.with_description(description);
    }

    state
        .core
        .storage
        .memory
        .create_session(&session)
        .map_err(|e| e.to_string())?;

    Ok(session)
}

/// Delete a memory session and optionally its chunks.
///
/// By default, deletes the session and all associated chunks.
#[tauri::command]
pub async fn delete_memory_session(
    state: State<'_, AppState>,
    session_id: String,
    delete_chunks: Option<bool>,
) -> Result<bool, String> {
    state
        .core
        .storage
        .memory
        .delete_session(&session_id, delete_chunks.unwrap_or(true))
        .map_err(|e| e.to_string())
}

// ============================================================================
// Statistics Commands
// ============================================================================

/// Get memory statistics for an agent.
#[tauri::command]
pub async fn get_memory_stats(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<MemoryStats, String> {
    state
        .core
        .storage
        .memory
        .get_stats(&agent_id)
        .map_err(|e| e.to_string())
}

// ============================================================================
// Export Commands
// ============================================================================

/// Export memories to Markdown format.
#[tauri::command]
pub async fn export_memory_markdown(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<ExportResult, String> {
    let storage = state.core.storage.memory.clone();
    let exporter = MemoryExporter::new(storage);
    exporter.export_agent(&agent_id).map_err(|e| e.to_string())
}

/// Export a specific session to Markdown format.
#[tauri::command]
pub async fn export_memory_session_markdown(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<ExportResult, String> {
    let storage = state.core.storage.memory.clone();
    let exporter = MemoryExporter::new(storage);
    exporter
        .export_session(&session_id)
        .map_err(|e| e.to_string())
}

/// Export memories with custom options.
#[tauri::command]
pub async fn export_memory_advanced(
    state: State<'_, AppState>,
    request: ExportMemoryRequest,
) -> Result<ExportResult, String> {
    let storage = state.core.storage.memory.clone();

    // Start with builder and apply preset
    let mut builder = MemoryExporterBuilder::new(storage);

    builder = match request.preset.as_deref() {
        Some("minimal") => builder.minimal(),
        Some("compact") => builder.compact(),
        _ => builder, // Use default
    };

    // Apply overrides
    if let Some(v) = request.include_metadata {
        builder = builder.include_metadata(v);
    }
    if let Some(v) = request.include_timestamps {
        builder = builder.include_timestamps(v);
    }
    if let Some(v) = request.include_source {
        builder = builder.include_source(v);
    }
    if let Some(v) = request.include_tags {
        builder = builder.include_tags(v);
    }

    let exporter = builder.build();

    if let Some(session_id) = request.session_id {
        exporter
            .export_session(&session_id)
            .map_err(|e| e.to_string())
    } else {
        exporter
            .export_agent(&request.agent_id)
            .map_err(|e| e.to_string())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_request_deserialization() {
        let json = r#"{
            "query": {
                "agent_id": "test-agent",
                "query": "rust async",
                "mode": "keyword",
                "limit": 20
            },
            "min_score": 10.0,
            "scoring_preset": "frequency_focused"
        }"#;

        let request: SearchMemoryRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.query.agent_id, "test-agent");
        assert_eq!(request.query.query.as_deref(), Some("rust async"));
        assert_eq!(request.min_score, Some(10.0));
        assert_eq!(request.scoring_preset.as_deref(), Some("frequency_focused"));
    }

    #[test]
    fn test_search_request_defaults() {
        let json = r#"{
            "query": {
                "agent_id": "test-agent"
            }
        }"#;

        let request: SearchMemoryRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.min_score, None);
        assert_eq!(request.scoring_preset, None);
    }

    #[test]
    fn test_create_chunk_request_deserialization() {
        let json = r#"{
            "agent_id": "my-agent",
            "content": "Test memory content",
            "session_id": "session-123",
            "tags": ["test", "example"]
        }"#;

        let request: CreateMemoryChunkRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.agent_id, "my-agent");
        assert_eq!(request.content, "Test memory content");
        assert_eq!(request.session_id, Some("session-123".to_string()));
        assert_eq!(request.tags, vec!["test", "example"]);
    }

    #[test]
    fn test_create_chunk_request_minimal() {
        let json = r#"{
            "agent_id": "my-agent",
            "content": "Test"
        }"#;

        let request: CreateMemoryChunkRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.session_id, None);
        assert!(request.tags.is_empty());
    }

    #[test]
    fn test_create_session_request_deserialization() {
        let json = r#"{
            "agent_id": "my-agent",
            "name": "Research Session",
            "description": "A session for research tasks",
            "tags": ["research", "ai"]
        }"#;

        let request: CreateMemorySessionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.agent_id, "my-agent");
        assert_eq!(request.name, "Research Session");
        assert_eq!(
            request.description,
            Some("A session for research tasks".to_string())
        );
        assert_eq!(request.tags, vec!["research", "ai"]);
    }

    #[test]
    fn test_export_request_deserialization() {
        let json = r#"{
            "agent_id": "my-agent",
            "session_id": "session-123",
            "preset": "compact",
            "include_metadata": false,
            "include_tags": true
        }"#;

        let request: ExportMemoryRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.agent_id, "my-agent");
        assert_eq!(request.session_id, Some("session-123".to_string()));
        assert_eq!(request.preset, Some("compact".to_string()));
        assert_eq!(request.include_metadata, Some(false));
        assert_eq!(request.include_tags, Some(true));
    }

    #[test]
    fn test_export_request_minimal() {
        let json = r#"{
            "agent_id": "my-agent"
        }"#;

        let request: ExportMemoryRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.agent_id, "my-agent");
        assert_eq!(request.session_id, None);
        assert_eq!(request.preset, None);
    }

    #[test]
    fn test_memory_list_response_serialization() {
        let response = MemoryListResponse {
            items: vec!["a".to_string(), "b".to_string()],
            total: 10,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"total\":10"));
        assert!(json.contains("\"items\""));
    }
}
