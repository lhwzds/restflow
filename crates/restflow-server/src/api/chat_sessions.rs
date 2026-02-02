use crate::api::{ApiResponse, state::AppState};
use axum::{
    Json,
    extract::{Path, State},
};
use restflow_core::models::{ChatExecutionStatus, ChatMessage, ChatRole, ChatSession, ChatSessionSummary, MessageExecution};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct CreateChatSessionRequest {
    pub agent_id: String,
    pub model: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub skill_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateChatSessionRequest {
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddChatMessageRequest {
    pub role: ChatRole,
    pub content: String,
    #[serde(default)]
    pub execution: Option<MessageExecution>,
}

// GET /api/chat-sessions
pub async fn list_chat_sessions(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<ChatSessionSummary>>> {
    match state.storage.chat_sessions.list_summaries() {
        Ok(summaries) => Json(ApiResponse::ok(summaries)),
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to list chat sessions: {}",
            e
        ))),
    }
}

// POST /api/chat-sessions
pub async fn create_chat_session(
    State(state): State<AppState>,
    Json(request): Json<CreateChatSessionRequest>,
) -> Json<ApiResponse<ChatSession>> {
    let mut session = ChatSession::new(request.agent_id, request.model);

    if let Some(name) = request.name {
        session = session.with_name(name);
    }

    if let Some(skill_id) = request.skill_id {
        session = session.with_skill(skill_id);
    }

    match state.storage.chat_sessions.create(&session) {
        Ok(_) => Json(ApiResponse::ok_with_message(
            session,
            "Chat session created successfully",
        )),
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to create chat session: {}",
            e
        ))),
    }
}

// GET /api/chat-sessions/{id}
pub async fn get_chat_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<ChatSession>> {
    match state.storage.chat_sessions.get(&id) {
        Ok(Some(session)) => Json(ApiResponse::ok(session)),
        Ok(None) => Json(ApiResponse::error(format!(
            "Chat session '{}' not found",
            id
        ))),
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to get chat session: {}",
            e
        ))),
    }
}

// PATCH /api/chat-sessions/{id}
pub async fn update_chat_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateChatSessionRequest>,
) -> Json<ApiResponse<ChatSession>> {
    let mut session = match state.storage.chat_sessions.get(&id) {
        Ok(Some(session)) => session,
        Ok(None) => {
            return Json(ApiResponse::error(format!(
                "Chat session '{}' not found",
                id
            )))
        }
        Err(e) => {
            return Json(ApiResponse::error(format!(
                "Failed to get chat session: {}",
                e
            )))
        }
    };

    let mut updated = false;

    if let Some(agent_id) = request.agent_id {
        session.agent_id = agent_id;
        updated = true;
    }

    if let Some(model) = request.model {
        session.model = model;
        updated = true;
    }

    let has_name_update = request.name.is_some();
    if let Some(name) = request.name {
        session.rename(name);
        updated = true;
    }

    if updated {
        if !has_name_update {
            session.updated_at = chrono::Utc::now().timestamp_millis();
        }

        match state.storage.chat_sessions.update(&session) {
            Ok(_) => Json(ApiResponse::ok(session)),
            Err(e) => Json(ApiResponse::error(format!(
                "Failed to update chat session: {}",
                e
            ))),
        }
    } else {
        Json(ApiResponse::ok(session))
    }
}

// DELETE /api/chat-sessions/{id}
pub async fn delete_chat_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<()>> {
    match state.storage.chat_sessions.delete(&id) {
        Ok(true) => Json(ApiResponse::message("Chat session deleted successfully")),
        Ok(false) => Json(ApiResponse::error(format!(
            "Chat session '{}' not found",
            id
        ))),
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to delete chat session: {}",
            e
        ))),
    }
}

// POST /api/chat-sessions/{id}/messages
pub async fn add_chat_message(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<AddChatMessageRequest>,
) -> Json<ApiResponse<ChatSession>> {
    let mut session = match state.storage.chat_sessions.get(&id) {
        Ok(Some(session)) => session,
        Ok(None) => {
            return Json(ApiResponse::error(format!(
                "Chat session '{}' not found",
                id
            )))
        }
        Err(e) => {
            return Json(ApiResponse::error(format!(
                "Failed to get chat session: {}",
                e
            )))
        }
    };

    let mut message = ChatMessage {
        role: request.role,
        content: request.content,
        timestamp: chrono::Utc::now().timestamp_millis(),
        execution: request.execution,
    };

    if message.role == ChatRole::Assistant && message.execution.is_none() {
        message.execution = Some(MessageExecution {
            steps: Vec::new(),
            duration_ms: 0,
            tokens_used: 0,
            status: ChatExecutionStatus::Completed,
        });
    }

    session.add_message(message);

    if session.name == "New Chat" && session.messages.len() == 1 {
        session.auto_name_from_first_message();
    }

    match state.storage.chat_sessions.update(&session) {
        Ok(_) => Json(ApiResponse::ok(session)),
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to update chat session: {}",
            e
        ))),
    }
}

// GET /api/chat-sessions/{id}/summary
pub async fn get_chat_session_summary(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<ChatSessionSummary>> {
    match state.storage.chat_sessions.get(&id) {
        Ok(Some(session)) => Json(ApiResponse::ok(ChatSessionSummary::from(&session))),
        Ok(None) => Json(ApiResponse::error(format!(
            "Chat session '{}' not found",
            id
        ))),
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to get chat session: {}",
            e
        ))),
    }
}
