use axum::{
    extract::State,
    response::Json,
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ExecuteRequest {
    pub script_name: String,
    pub input: Value,
}

#[derive(Debug, Serialize)]
pub struct ExecuteResponse {
    pub success: bool,
    pub data: Option<Value>,
    pub error: Option<String>,
}

/// Internal use only - executes scripts within workflow context
pub async fn execute_script(
    State(state): State<AppState>,
    Json(req): Json<ExecuteRequest>,
) -> Result<Json<ExecuteResponse>, StatusCode> {
    let manager = match state.get_python_manager().await {
        Ok(m) => m,
        Err(e) => {
            return Ok(Json(ExecuteResponse {
                success: false,
                data: None,
                error: Some(format!("Python not available: {}", e)),
            }));
        }
    };
    
    match manager.execute_script(&req.script_name, req.input).await {
        Ok(result) => Ok(Json(ExecuteResponse {
            success: true,
            data: Some(result),
            error: None,
        })),
        Err(e) => Ok(Json(ExecuteResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        })),
    }
}

/// For debugging purposes
pub async fn list_scripts(
    State(state): State<AppState>,
) -> Result<Json<Vec<String>>, StatusCode> {
    let manager = state.get_python_manager().await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    
    manager.list_scripts().await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}