use crate::AppCore;
use crate::daemon::http::ApiError;
use crate::models::Skill;
use crate::services::skills as skills_service;
use axum::{
    Json, Router,
    extract::{Extension, Path},
    routing::get,
};
use serde::Deserialize;
use std::sync::Arc;

pub fn router() -> Router {
    Router::new()
        .route("/", get(list_skills).post(create_skill))
        .route(
            "/{id}",
            get(get_skill).put(update_skill).delete(delete_skill),
        )
}

async fn list_skills(
    Extension(core): Extension<Arc<AppCore>>,
) -> Result<Json<Vec<Skill>>, ApiError> {
    let skills = skills_service::list_skills(&core).await?;
    Ok(Json(skills))
}

async fn get_skill(
    Extension(core): Extension<Arc<AppCore>>,
    Path(id): Path<String>,
) -> Result<Json<Skill>, ApiError> {
    let skill = skills_service::get_skill(&core, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("Skill"))?;
    Ok(Json(skill))
}

#[derive(Debug, Deserialize)]
struct CreateSkillRequest {
    id: Option<String>,
    name: String,
    description: Option<String>,
    tags: Option<Vec<String>>,
    content: String,
}

async fn create_skill(
    Extension(core): Extension<Arc<AppCore>>,
    Json(req): Json<CreateSkillRequest>,
) -> Result<Json<Skill>, ApiError> {
    let id = req.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let skill = Skill::new(id, req.name, req.description, req.tags, req.content);
    skills_service::create_skill(&core, skill.clone()).await?;
    Ok(Json(skill))
}

#[derive(Debug, Deserialize)]
struct UpdateSkillRequest {
    name: Option<String>,
    description: Option<Option<String>>,
    tags: Option<Option<Vec<String>>>,
    content: Option<String>,
}

async fn update_skill(
    Extension(core): Extension<Arc<AppCore>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSkillRequest>,
) -> Result<Json<Skill>, ApiError> {
    let mut skill = skills_service::get_skill(&core, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("Skill"))?;

    skill.update(req.name, req.description, req.tags, req.content);
    skills_service::update_skill(&core, &id, &skill).await?;
    Ok(Json(skill))
}

async fn delete_skill(
    Extension(core): Extension<Arc<AppCore>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    skills_service::delete_skill(&core, &id).await?;
    Ok(Json(serde_json::json!({ "deleted": true, "id": id })))
}
