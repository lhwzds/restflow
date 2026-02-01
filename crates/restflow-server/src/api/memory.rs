use crate::api::{ApiResponse, state::AppState};
use axum::{
    Json,
    extract::{Query, State},
};
use restflow_core::models::memory::{
    MemoryChunk, MemorySearchQuery, MemorySearchResult, MemorySource, MemoryStats,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct CreateMemoryChunkRequest {
    pub agent_id: String,
    pub content: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub source: Option<MemorySource>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub token_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct MemoryStatsQuery {
    pub agent_id: String,
}

// POST /api/memory/search
pub async fn search_memory(
    State(state): State<AppState>,
    Json(query): Json<MemorySearchQuery>,
) -> Json<ApiResponse<MemorySearchResult>> {
    match state.storage.memory.search(&query) {
        Ok(result) => Json(ApiResponse::ok(result)),
        Err(e) => Json(ApiResponse::error(format!("Failed to search memory: {}", e))),
    }
}

// POST /api/memory/chunks
pub async fn create_memory_chunk(
    State(state): State<AppState>,
    Json(request): Json<CreateMemoryChunkRequest>,
) -> Json<ApiResponse<MemoryChunk>> {
    let mut chunk = MemoryChunk::new(request.agent_id, request.content);

    if let Some(session_id) = request.session_id {
        chunk = chunk.with_session(session_id);
    }

    if let Some(source) = request.source {
        chunk = chunk.with_source(source);
    }

    if let Some(tags) = request.tags {
        chunk = chunk.with_tags(tags);
    }

    if let Some(token_count) = request.token_count {
        chunk = chunk.with_token_count(token_count);
    }

    match state.storage.memory.store_chunk(&chunk) {
        Ok(chunk_id) => match state.storage.memory.get_chunk(&chunk_id) {
            Ok(Some(stored)) => Json(ApiResponse::ok_with_message(
                stored,
                "Memory chunk stored successfully",
            )),
            Ok(None) => Json(ApiResponse::error(format!(
                "Stored memory chunk '{}' not found",
                chunk_id
            ))),
            Err(e) => Json(ApiResponse::error(format!(
                "Failed to load stored memory chunk: {}",
                e
            ))),
        },
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to store memory chunk: {}",
            e
        ))),
    }
}

// GET /api/memory/stats
pub async fn get_memory_stats(
    State(state): State<AppState>,
    Query(query): Query<MemoryStatsQuery>,
) -> Json<ApiResponse<MemoryStats>> {
    match state.storage.memory.get_stats(&query.agent_id) {
        Ok(stats) => Json(ApiResponse::ok(stats)),
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to get memory stats: {}",
            e
        ))),
    }
}
