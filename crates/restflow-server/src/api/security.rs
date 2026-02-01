use crate::api::{ApiResponse, state::AppState};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use restflow_core::models::security::{CommandPattern, PendingApproval};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct RejectApprovalRequest {
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAllowlistRequest {
    pub allowlist: Vec<CommandPattern>,
}

// GET /api/security/approvals
pub async fn list_pending_approvals(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<PendingApproval>>> {
    let approvals = state
        .security_checker
        .approval_manager()
        .get_all_pending()
        .await;
    Json(ApiResponse::ok(approvals))
}

// POST /api/security/approvals/{id}/approve
pub async fn approve_security_approval(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<PendingApproval>>, (StatusCode, String)> {
    let approval = state
        .security_checker
        .approval_manager()
        .approve(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match approval {
        Some(approval) => Ok(Json(ApiResponse::ok_with_message(
            approval,
            "Approval granted",
        ))),
        None => Err((StatusCode::NOT_FOUND, "Approval not found".to_string())),
    }
}

// POST /api/security/approvals/{id}/reject
pub async fn reject_security_approval(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<RejectApprovalRequest>,
) -> Result<Json<ApiResponse<PendingApproval>>, (StatusCode, String)> {
    let approval = state
        .security_checker
        .approval_manager()
        .reject(&id, request.reason)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match approval {
        Some(approval) => Ok(Json(ApiResponse::ok_with_message(
            approval,
            "Approval rejected",
        ))),
        None => Err((StatusCode::NOT_FOUND, "Approval not found".to_string())),
    }
}

// GET /api/security/allowlist
pub async fn get_security_allowlist(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<CommandPattern>>> {
    let policy = state.security_checker.get_policy().await;
    Json(ApiResponse::ok(policy.allowlist))
}

// PUT /api/security/allowlist
pub async fn update_security_allowlist(
    State(state): State<AppState>,
    Json(request): Json<UpdateAllowlistRequest>,
) -> Json<ApiResponse<Vec<CommandPattern>>> {
    let mut policy = state.security_checker.get_policy().await;
    policy.allowlist = request.allowlist;
    state.security_checker.set_policy(policy.clone()).await;

    Json(ApiResponse::ok_with_message(
        policy.allowlist,
        "Allowlist updated successfully",
    ))
}
