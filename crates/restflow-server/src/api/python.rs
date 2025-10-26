use axum::{extract::{Path, State}, http::StatusCode, response::Json};
use restflow_core::python::TemplateInfo;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::api::{ApiResponse, state::AppState};

#[derive(Debug, Deserialize)]
pub struct ExecuteRequest {
    pub script_name: String,
    pub input: Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecuteResponse {
    pub result: Value,
}

/// Internal use only - executes scripts within workflow context
pub async fn execute_script(
    State(state): State<AppState>,
    Json(req): Json<ExecuteRequest>,
) -> Result<Json<ApiResponse<ExecuteResponse>>, StatusCode> {
    let manager = match state.get_python_manager().await {
        Ok(m) => m,
        Err(e) => {
            return Ok(Json(ApiResponse::error(format!(
                "Python not available: {}",
                e
            ))));
        }
    };

    match manager.execute_script(&req.script_name, req.input).await {
        Ok(result) => Ok(Json(ApiResponse::ok(ExecuteResponse { result }))),
        Err(e) => Ok(Json(ApiResponse::error(e.to_string()))),
    }
}

/// For debugging purposes
pub async fn list_scripts(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<String>>>, StatusCode> {
    let manager = state
        .get_python_manager()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    match manager.list_scripts().await {
        Ok(scripts) => Ok(Json(ApiResponse::ok(scripts))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// List all available Python script templates
pub async fn list_templates(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<TemplateInfo>>>, StatusCode> {
    let manager = match state.get_python_manager().await {
        Ok(m) => m,
        Err(e) => {
            return Ok(Json(ApiResponse::error(format!(
                "Python not available: {}",
                e
            ))));
        }
    };

    match manager.list_templates().await {
        Ok(templates) => Ok(Json(ApiResponse::ok(templates))),
        Err(e) => Ok(Json(ApiResponse::error(e.to_string()))),
    }
}

/// Get a specific template by ID
pub async fn get_template(
    State(state): State<AppState>,
    Path(template_id): Path<String>,
) -> Result<Json<ApiResponse<HashMap<String, String>>>, StatusCode> {
    let manager = match state.get_python_manager().await {
        Ok(m) => m,
        Err(e) => {
            return Ok(Json(ApiResponse::error(format!(
                "Python not available: {}",
                e
            ))));
        }
    };

    match manager.get_template(&template_id).await {
        Ok(template) => Ok(Json(ApiResponse::ok(template))),
        Err(e) => Ok(Json(ApiResponse::error(e.to_string()))),
    }
}
