use crate::api::{state::AppState, ApiResponse};
use axum::{extract::State, Json};
use restflow_core::models::{AIModel, ModelMetadataDTO};

/// GET /api/models - List all available AI models with their metadata
pub async fn list_models(State(_state): State<AppState>) -> Json<ApiResponse<Vec<ModelMetadataDTO>>> {
    let models = AIModel::all_with_metadata();
    Json(ApiResponse::ok(models))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::state::AppState;
    use restflow_core::AppCore;
    use std::sync::Arc;
    use tempfile::tempdir;

    async fn create_test_state() -> AppState {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap())
    }

    #[tokio::test]
    async fn test_list_models() {
        let state = create_test_state().await;

        let response = list_models(State(state)).await;
        let body = response.0;

        assert!(body.success);
        assert!(body.data.is_some());

        let models = body.data.unwrap();
        assert_eq!(models.len(), 12); // We have 12 models

        // Verify structure of first model
        let first_model = &models[0];
        assert!(!first_model.name.is_empty());

        // Verify we have models from all providers
        let has_openai = models.iter().any(|m| m.provider == restflow_core::models::Provider::OpenAI);
        let has_anthropic = models.iter().any(|m| m.provider == restflow_core::models::Provider::Anthropic);
        let has_deepseek = models.iter().any(|m| m.provider == restflow_core::models::Provider::DeepSeek);

        assert!(has_openai);
        assert!(has_anthropic);
        assert!(has_deepseek);
    }
}
