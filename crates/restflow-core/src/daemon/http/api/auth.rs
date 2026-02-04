use crate::auth::{AuthManagerConfig, AuthProfile, AuthProvider, Credential, CredentialSource};
use crate::daemon::http::ApiError;
use crate::auth::manager::ProfileUpdate;
use crate::AppCore;
use axum::{
    extract::{Extension, Path},
    routing::{get, post},
    Json, Router,
};
use restflow_storage::AuthProfileStorage;
use std::sync::Arc;

pub fn router() -> Router {
    Router::new()
        .route("/profiles", get(list_profiles).post(create_profile))
        .route(
            "/profiles/{id}",
            get(get_profile).put(update_profile).delete(delete_profile),
        )
        .route("/profiles/{id}/test", post(test_profile))
        .route("/discover", post(discover_auth))
}

async fn list_profiles(
    Extension(core): Extension<Arc<AppCore>>,
) -> Result<Json<Vec<AuthProfile>>, ApiError> {
    let manager = build_auth_manager(&core).await?;
    Ok(Json(manager.list_profiles().await))
}

async fn get_profile(
    Extension(core): Extension<Arc<AppCore>>,
    Path(id): Path<String>,
) -> Result<Json<AuthProfile>, ApiError> {
    let manager = build_auth_manager(&core).await?;
    manager
        .get_profile(&id)
        .await
        .map(Json)
        .ok_or_else(|| ApiError::not_found("Auth profile"))
}

#[derive(Debug, serde::Deserialize)]
struct CreateProfileRequest {
    name: String,
    credential: Credential,
    source: CredentialSource,
    provider: AuthProvider,
}

async fn create_profile(
    Extension(core): Extension<Arc<AppCore>>,
    Json(req): Json<CreateProfileRequest>,
) -> Result<Json<AuthProfile>, ApiError> {
    let manager = build_auth_manager(&core).await?;
    let id = manager
        .add_profile_from_credential(req.name, req.credential, req.source, req.provider)
        .await?;
    manager
        .get_profile(&id)
        .await
        .map(Json)
        .ok_or_else(|| ApiError::internal("Profile created but not found"))
}

async fn update_profile(
    Extension(core): Extension<Arc<AppCore>>,
    Path(id): Path<String>,
    Json(update): Json<ProfileUpdate>,
) -> Result<Json<AuthProfile>, ApiError> {
    let manager = build_auth_manager(&core).await?;
    let profile = manager.update_profile(&id, update).await?;
    Ok(Json(profile))
}

async fn delete_profile(
    Extension(core): Extension<Arc<AppCore>>,
    Path(id): Path<String>,
) -> Result<Json<AuthProfile>, ApiError> {
    let manager = build_auth_manager(&core).await?;
    let profile = manager.remove_profile(&id).await?;
    Ok(Json(profile))
}

async fn test_profile(
    Extension(core): Extension<Arc<AppCore>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let manager = build_auth_manager(&core).await?;
    let profile = manager
        .get_profile(&id)
        .await
        .ok_or_else(|| ApiError::not_found("Auth profile"))?;

    profile.get_api_key(manager.resolver())?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn discover_auth(
    Extension(core): Extension<Arc<AppCore>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let manager = build_auth_manager(&core).await?;
    let summary = manager.discover().await?;
    Ok(Json(serde_json::to_value(summary).unwrap()))
}

async fn build_auth_manager(core: &Arc<AppCore>) -> Result<crate::auth::AuthProfileManager, ApiError> {
    let config = AuthManagerConfig {
        auto_discover: false,
        ..AuthManagerConfig::default()
    };
    let db = core.storage.get_db();
    let secrets = Arc::new(core.storage.secrets.clone());
    let profile_storage = AuthProfileStorage::new(db)?;
    let manager = crate::auth::AuthProfileManager::with_storage(
        config,
        secrets,
        Some(profile_storage),
    );
    manager.initialize().await?;
    Ok(manager)
}
