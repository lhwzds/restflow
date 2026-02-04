use crate::daemon::http::ApiError;
use crate::models::chat_session::{ChatSession, ChatSessionSummary};
use crate::AppCore;
use axum::{
    extract::{Extension, Path, Query},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;

pub fn router() -> Router {
    Router::new()
        .route("/", get(list_sessions).post(create_session))
        .route("/search", get(search_sessions))
        .route("/{id}", get(get_session).delete(delete_session))
}

async fn list_sessions(
    Extension(core): Extension<Arc<AppCore>>,
) -> Result<Json<Vec<ChatSessionSummary>>, ApiError> {
    let sessions = core.storage.chat_sessions.list_summaries()?;
    Ok(Json(sessions))
}

async fn get_session(
    Extension(core): Extension<Arc<AppCore>>,
    Path(id): Path<String>,
) -> Result<Json<ChatSession>, ApiError> {
    let session = core
        .storage
        .chat_sessions
        .get(&id)?
        .ok_or_else(|| ApiError::not_found("Session"))?;
    Ok(Json(session))
}

#[derive(Debug, Deserialize)]
struct CreateSessionRequest {
    agent_id: String,
    model: String,
}

async fn create_session(
    Extension(core): Extension<Arc<AppCore>>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<ChatSession>, ApiError> {
    let session = ChatSession::new(req.agent_id, req.model);
    core.storage.chat_sessions.create(&session)?;
    Ok(Json(session))
}

async fn delete_session(
    Extension(core): Extension<Arc<AppCore>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let deleted = core.storage.chat_sessions.delete(&id)?;
    Ok(Json(serde_json::json!({ "deleted": deleted, "id": id })))
}

#[derive(Debug, Deserialize)]
struct SearchSessionsQuery {
    q: String,
}

async fn search_sessions(
    Extension(core): Extension<Arc<AppCore>>,
    Query(query): Query<SearchSessionsQuery>,
) -> Result<Json<Vec<ChatSessionSummary>>, ApiError> {
    let normalized = query.q.to_lowercase();
    let sessions = core.storage.chat_sessions.list()?;
    
    let matches: Vec<ChatSessionSummary> = sessions
        .into_iter()
        .filter(|session| {
            session.name.to_lowercase().contains(&normalized)
                || session
                    .messages
                    .iter()
                    .any(|msg| msg.content.to_lowercase().contains(&normalized))
        })
        .map(|session| ChatSessionSummary::from(&session))
        .collect();

    Ok(Json(matches))
}
