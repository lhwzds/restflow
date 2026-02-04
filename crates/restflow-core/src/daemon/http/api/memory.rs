use crate::daemon::http::ApiError;
use crate::models::memory::{MemoryChunk, MemorySearchQuery, MemorySearchResult};
use crate::services::agent as agent_service;
use crate::AppCore;
use axum::{
    extract::{Extension, Query},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;

pub fn router() -> Router {
    Router::new()
        .route("/", get(list_memory).delete(clear_memory))
        .route("/search", get(search_memory))
        .route("/stats", get(memory_stats))
}

#[derive(Debug, Deserialize)]
struct ListMemoryQuery {
    agent_id: Option<String>,
    tag: Option<String>,
}

async fn list_memory(
    Extension(core): Extension<Arc<AppCore>>,
    Query(query): Query<ListMemoryQuery>,
) -> Result<Json<Vec<MemoryChunk>>, ApiError> {
    let chunks = match (query.agent_id, query.tag) {
        (Some(agent_id), Some(tag)) => core
            .storage
            .memory
            .list_chunks(&agent_id)?
            .into_iter()
            .filter(|chunk| chunk.tags.iter().any(|t| t == &tag))
            .collect(),
        (Some(agent_id), None) => core.storage.memory.list_chunks(&agent_id)?,
        (None, Some(tag)) => core.storage.memory.list_chunks_by_tag(&tag)?,
        (None, None) => {
            let agent_id = resolve_agent_id(&core, None).await?;
            core.storage.memory.list_chunks(&agent_id)?
        }
    };
    Ok(Json(chunks))
}

#[derive(Debug, Deserialize)]
struct SearchMemoryQuery {
    q: String,
    agent_id: Option<String>,
    limit: Option<usize>,
}

async fn search_memory(
    Extension(core): Extension<Arc<AppCore>>,
    Query(query): Query<SearchMemoryQuery>,
) -> Result<Json<MemorySearchResult>, ApiError> {
    let agent_id = resolve_agent_id(&core, query.agent_id).await?;
    let mut search = MemorySearchQuery::new(agent_id).with_query(query.q);
    if let Some(limit) = query.limit {
        search.limit = limit as u32;
    }
    let results = core.storage.memory.search(&search)?;
    Ok(Json(results))
}

#[derive(Debug, Deserialize)]
struct ClearMemoryQuery {
    agent_id: Option<String>,
}

async fn clear_memory(
    Extension(core): Extension<Arc<AppCore>>,
    Query(query): Query<ClearMemoryQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let agent_id = resolve_agent_id(&core, query.agent_id).await?;
    let deleted = core.storage.memory.delete_chunks_for_agent(&agent_id)?;
    Ok(Json(serde_json::json!({
        "deleted": deleted,
        "agent_id": agent_id
    })))
}

#[derive(Debug, Deserialize)]
struct MemoryStatsQuery {
    agent_id: Option<String>,
}

async fn memory_stats(
    Extension(core): Extension<Arc<AppCore>>,
    Query(query): Query<MemoryStatsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let agent_id = resolve_agent_id(&core, query.agent_id).await?;
    let stats = core.storage.memory.get_stats(&agent_id)?;
    Ok(Json(serde_json::to_value(stats).unwrap()))
}

async fn resolve_agent_id(core: &Arc<AppCore>, agent_id: Option<String>) -> Result<String, ApiError> {
    if let Some(id) = agent_id {
        return Ok(id);
    }

    let agents = agent_service::list_agents(core).await?;
    if agents.is_empty() {
        return Err(ApiError::bad_request("No agents available"));
    }

    Ok(agents[0].id.clone())
}
