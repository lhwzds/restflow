use crate::api::{ApiResponse, state::AppState};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Router,
};
use restflow_core::memory::{
    ExportResult, MemoryExporterBuilder, RankedSearchResult, SearchEngineBuilder,
    UnifiedSearchEngine,
};
use restflow_core::models::memory::{MemoryChunk, MemorySearchQuery, MemorySource, MemoryStats};
use restflow_core::models::{SearchMode, UnifiedSearchQuery};
use serde::{Deserialize, Serialize};

// Simple query-string based search for GET /api/memory/search
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub agent_id: String,
    #[serde(default)]
    pub include_sessions: bool,
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
}

fn default_limit() -> u32 {
    20
}

// POST body based search for advanced queries
#[derive(Debug, Deserialize)]
pub struct SearchMemoryRequest {
    pub query: MemorySearchQuery,
    #[serde(default)]
    pub min_score: Option<f64>,
    #[serde(default)]
    pub scoring_preset: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateMemoryChunkRequest {
    pub agent_id: String,
    pub content: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ExportMemoryRequest {
    pub agent_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub preset: Option<String>,
    #[serde(default)]
    pub include_metadata: Option<bool>,
    #[serde(default)]
    pub include_timestamps: Option<bool>,
    #[serde(default)]
    pub include_source: Option<bool>,
    #[serde(default)]
    pub include_tags: Option<bool>,
    #[serde(default)]
    pub include_session_headers: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct MemoryStatsRequest {
    pub agent_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ImportMemoryRequest {
    pub agent_id: String,
    #[serde(default)]
    pub chunks: Vec<ImportMemoryChunk>,
}

#[derive(Debug, Deserialize)]
pub struct ImportMemoryChunk {
    pub content: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub source: Option<MemorySource>,
    #[serde(default)]
    pub token_count: Option<u32>,
    #[serde(default)]
    pub created_at: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ImportMemoryResponse {
    pub imported: usize,
    pub chunk_ids: Vec<String>,
}

pub fn memory_routes() -> Router<AppState> {
    Router::new()
        .route("/search", get(search_memory).post(search_memory_advanced))
        .route("/chunks", post(create_memory_chunk))
        .route("/chunks/:id", delete(delete_memory_chunk))
        .route("/stats", get(get_memory_stats))
        .route("/export", get(export_memory))
        .route("/import", post(import_memory))
}

/// Simple unified search via GET query params
pub async fn search_memory(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Json<ApiResponse<restflow_core::memory::UnifiedSearchResults>> {
    let base = MemorySearchQuery::new(query.agent_id)
        .with_query(query.q)
        .with_mode(SearchMode::Keyword)
        .paginate(query.limit, query.offset);
    let unified_query = UnifiedSearchQuery::new(base).with_sessions(query.include_sessions);

    let engine = UnifiedSearchEngine::new(
        state.storage.memory.clone(),
        state.storage.chat_sessions.clone(),
    );

    match engine.search(&unified_query) {
        Ok(results) => Json(ApiResponse::ok(results)),
        Err(error) => Json(ApiResponse::error(format!("Search failed: {}", error))),
    }
}

/// Advanced search via POST body with scoring presets
pub async fn search_memory_advanced(
    State(state): State<AppState>,
    Json(payload): Json<SearchMemoryRequest>,
) -> Result<Json<ApiResponse<RankedSearchResult>>, (StatusCode, String)> {
    let storage = state.storage.memory.clone();
    let mut builder = SearchEngineBuilder::new(storage);

    builder = match payload.scoring_preset.as_deref() {
        Some("frequency_focused") => builder.frequency_focused(),
        Some("recency_focused") => builder.recency_focused(),
        Some("balanced") => builder.balanced(),
        _ => builder,
    };

    if let Some(min_score) = payload.min_score {
        builder = builder.min_score(min_score);
    }

    let engine = builder.build();
    match engine.search_ranked(&payload.query) {
        Ok(results) => Ok(Json(ApiResponse::ok(results))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

pub async fn create_memory_chunk(
    State(state): State<AppState>,
    Json(payload): Json<CreateMemoryChunkRequest>,
) -> Result<Json<ApiResponse<MemoryChunk>>, (StatusCode, String)> {
    let mut chunk = MemoryChunk::new(payload.agent_id, payload.content)
        .with_source(MemorySource::ManualNote)
        .with_tags(payload.tags);

    if let Some(session_id) = payload.session_id {
        chunk = chunk.with_session(session_id);
    }

    match state.storage.memory.store_chunk(&chunk) {
        Ok(chunk_id) => {
            if chunk_id != chunk.id {
                match state.storage.memory.get_chunk(&chunk_id) {
                    Ok(Some(existing)) => Ok(Json(ApiResponse::ok(existing))),
                    Ok(None) => Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed to retrieve stored chunk".to_string(),
                    )),
                    Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
                }
            } else {
                Ok(Json(ApiResponse::ok(chunk)))
            }
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

pub async fn delete_memory_chunk(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<bool>>, (StatusCode, String)> {
    match state.storage.memory.delete_chunk(&id) {
        Ok(deleted) => Ok(Json(ApiResponse::ok(deleted))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

pub async fn get_memory_stats(
    State(state): State<AppState>,
    Query(params): Query<MemoryStatsRequest>,
) -> Result<Json<ApiResponse<MemoryStats>>, (StatusCode, String)> {
    match state.storage.memory.get_stats(&params.agent_id) {
        Ok(stats) => Ok(Json(ApiResponse::ok(stats))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

pub async fn export_memory(
    State(state): State<AppState>,
    Query(params): Query<ExportMemoryRequest>,
) -> Result<Json<ApiResponse<ExportResult>>, (StatusCode, String)> {
    let storage = state.storage.memory.clone();
    let mut builder = MemoryExporterBuilder::new(storage);

    builder = match params.preset.as_deref() {
        Some("minimal") => builder.minimal(),
        Some("compact") => builder.compact(),
        _ => builder,
    };

    if let Some(v) = params.include_metadata {
        builder = builder.include_metadata(v);
    }
    if let Some(v) = params.include_timestamps {
        builder = builder.include_timestamps(v);
    }
    if let Some(v) = params.include_source {
        builder = builder.include_source(v);
    }
    if let Some(v) = params.include_tags {
        builder = builder.include_tags(v);
    }
    if let Some(v) = params.include_session_headers {
        builder = builder.include_session_headers(v);
    }

    let exporter = builder.build();
    let result = if let Some(session_id) = params.session_id {
        exporter.export_session(&session_id)
    } else {
        exporter.export_agent(&params.agent_id)
    };

    match result {
        Ok(result) => Ok(Json(ApiResponse::ok(result))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

pub async fn import_memory(
    State(state): State<AppState>,
    Json(payload): Json<ImportMemoryRequest>,
) -> Result<Json<ApiResponse<ImportMemoryResponse>>, (StatusCode, String)> {
    let mut chunk_ids = Vec::with_capacity(payload.chunks.len());

    for incoming in payload.chunks {
        let mut chunk = MemoryChunk::new(payload.agent_id.clone(), incoming.content)
            .with_source(incoming.source.unwrap_or(MemorySource::ManualNote))
            .with_tags(incoming.tags);

        if let Some(session_id) = incoming.session_id {
            chunk = chunk.with_session(session_id);
        }
        if let Some(token_count) = incoming.token_count {
            chunk = chunk.with_token_count(token_count);
        }
        if let Some(created_at) = incoming.created_at {
            chunk = chunk.with_created_at(created_at);
        }

        let stored_id = state
            .storage
            .memory
            .store_chunk(&chunk)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        chunk_ids.push(stored_id);
    }

    Ok(Json(ApiResponse::ok(ImportMemoryResponse {
        imported: chunk_ids.len(),
        chunk_ids,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_core::AppCore;
    use std::sync::Arc;
    use tempfile::{tempdir, TempDir};

    async fn create_test_app() -> (Arc<AppCore>, TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let app = Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap());
        (app, temp_dir)
    }

    #[tokio::test]
    async fn test_create_memory_chunk() {
        let (app, _tmp_dir) = create_test_app().await;

        let request = CreateMemoryChunkRequest {
            agent_id: "agent-1".to_string(),
            content: "Test memory".to_string(),
            session_id: None,
            tags: vec!["tag".to_string()],
        };

        let result = create_memory_chunk(State(app), Json(request)).await;
        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert!(response.success);
        assert_eq!(response.data.unwrap().content, "Test memory");
    }

    #[tokio::test]
    async fn test_import_memory() {
        let (app, _tmp_dir) = create_test_app().await;

        let request = ImportMemoryRequest {
            agent_id: "agent-1".to_string(),
            chunks: vec![ImportMemoryChunk {
                content: "Imported memory".to_string(),
                session_id: None,
                tags: vec!["import".to_string()],
                source: None,
                token_count: Some(12),
                created_at: None,
            }],
        };

        let result = import_memory(State(app.clone()), Json(request)).await;
        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert!(response.success);
        assert_eq!(response.data.unwrap().imported, 1);

        let stats = get_memory_stats(
            State(app),
            Query(MemoryStatsRequest {
                agent_id: "agent-1".to_string(),
            }),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(stats.data.unwrap().chunk_count, 1);
    }
}
