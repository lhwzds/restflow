use crate::models::{AIModel, ModelMetadataDTO};
use axum::{Json, Router, routing::get};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct ModelsResponse {
    success: bool,
    data: Vec<ModelMetadataDTO>,
}

pub fn router() -> Router {
    Router::new().route("/", get(list_models))
}

async fn list_models() -> Json<ModelsResponse> {
    Json(ModelsResponse {
        success: true,
        data: AIModel::all_with_metadata(),
    })
}
