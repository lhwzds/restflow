use crate::api::{ApiResponse, state::AppState};
use axum::{
    Json,
    extract::{Query, State},
};
use restflow_core::memory::UnifiedSearchEngine;
use restflow_core::models::{MemorySearchQuery, SearchMode, UnifiedSearchQuery};
use serde::Deserialize;

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
