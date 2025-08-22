use axum::Json;
use serde::Serialize;
use serde_json::Value;

/// Create a success response with data
pub fn success<T: Serialize>(data: T) -> Json<Value> {
    Json(serde_json::json!({
        "status": "success",
        "data": data
    }))
}

/// Create a success response with data and message
pub fn success_with_message<T: Serialize>(data: T, message: String) -> Json<Value> {
    Json(serde_json::json!({
        "status": "success",
        "message": message,
        "data": data
    }))
}

/// Create an error response
pub fn error(message: String) -> Json<Value> {
    Json(serde_json::json!({
        "status": "error",
        "message": message
    }))
}

/// Create a not found response
pub fn not_found(message: String) -> Json<Value> {
    Json(serde_json::json!({
        "status": "error",
        "message": message
    }))
}