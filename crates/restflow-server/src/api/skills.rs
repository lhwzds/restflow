//! Skills API handlers for CRUD operations and import/export.

use crate::api::{response::ApiResponse, state::AppState};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use restflow_core::{models::Skill, services};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Request to create a new skill
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateSkillRequest {
    /// Optional ID. If not provided, a UUID will be auto-generated.
    pub id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub content: String,
}

/// Generate a short UUID (8 characters) for skill ID
fn generate_skill_id() -> String {
    Uuid::new_v4().to_string()[..8].to_lowercase()
}

/// Request to update an existing skill
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateSkillRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub content: Option<String>,
}

/// Request to import a skill from markdown
#[derive(Debug, Serialize, Deserialize)]
pub struct ImportSkillRequest {
    /// Optional ID. If not provided, a UUID will be auto-generated.
    pub id: Option<String>,
    pub markdown: String,
}

/// Response for export endpoint
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportSkillResponse {
    pub id: String,
    pub filename: String,
    pub markdown: String,
}

/// List all skills
pub async fn list_skills(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<Skill>>>, (StatusCode, String)> {
    match services::skills::list_skills(&state).await {
        Ok(skills) => Ok(Json(ApiResponse::ok(skills))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

/// Get a single skill by ID
pub async fn get_skill(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Skill>>, (StatusCode, String)> {
    match services::skills::get_skill(&state, &id).await {
        Ok(Some(skill)) => Ok(Json(ApiResponse::ok(skill))),
        Ok(None) => Err((StatusCode::NOT_FOUND, format!("Skill {} not found", id))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

/// Create a new skill
pub async fn create_skill(
    State(state): State<AppState>,
    Json(payload): Json<CreateSkillRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Generate ID if not provided, otherwise validate format
    let id = match payload.id {
        Some(id) => {
            if !is_valid_skill_id(&id) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Invalid skill ID. Use lowercase letters, numbers, and hyphens only.".to_string(),
                ));
            }
            id
        }
        None => generate_skill_id(),
    };

    // Check if skill already exists
    match services::skills::skill_exists(&state, &id).await {
        Ok(true) => {
            return Err((
                StatusCode::CONFLICT,
                format!("Skill {} already exists", id),
            ))
        }
        Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
        _ => {}
    }

    let skill = Skill::new(
        id,
        payload.name,
        payload.description,
        payload.tags,
        payload.content,
    );

    match services::skills::create_skill(&state, skill.clone()).await {
        Ok(_) => Ok((StatusCode::CREATED, Json(ApiResponse::ok(skill)))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

/// Update an existing skill
pub async fn update_skill(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateSkillRequest>,
) -> Result<Json<ApiResponse<Skill>>, (StatusCode, String)> {
    // Get existing skill
    let mut skill = match services::skills::get_skill(&state, &id).await {
        Ok(Some(s)) => s,
        Ok(None) => return Err((StatusCode::NOT_FOUND, format!("Skill {} not found", id))),
        Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    };

    // Update fields
    skill.update(
        payload.name,
        payload.description.map(Some),
        payload.tags.map(Some),
        payload.content,
    );

    match services::skills::update_skill(&state, &id, &skill).await {
        Ok(_) => Ok(Json(ApiResponse::ok(skill))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

/// Delete a skill
pub async fn delete_skill(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, String)> {
    // Check if skill exists
    match services::skills::skill_exists(&state, &id).await {
        Ok(false) => return Err((StatusCode::NOT_FOUND, format!("Skill {} not found", id))),
        Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
        _ => {}
    }

    match services::skills::delete_skill(&state, &id).await {
        Ok(_) => Ok(Json(ApiResponse::message("Skill deleted successfully"))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

/// Export a skill to markdown format
pub async fn export_skill(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ExportSkillResponse>>, (StatusCode, String)> {
    let skill = match services::skills::get_skill(&state, &id).await {
        Ok(Some(s)) => s,
        Ok(None) => return Err((StatusCode::NOT_FOUND, format!("Skill {} not found", id))),
        Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    };

    let markdown = services::skills::export_skill_to_markdown(&skill);
    let response = ExportSkillResponse {
        id: skill.id.clone(),
        filename: format!("{}.md", skill.id),
        markdown,
    };

    Ok(Json(ApiResponse::ok(response)))
}

/// Import a skill from markdown format
pub async fn import_skill(
    State(state): State<AppState>,
    Json(payload): Json<ImportSkillRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Generate ID if not provided, otherwise validate format
    let id = match payload.id {
        Some(id) => {
            if !is_valid_skill_id(&id) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Invalid skill ID. Use lowercase letters, numbers, and hyphens only.".to_string(),
                ));
            }
            id
        }
        None => generate_skill_id(),
    };

    // Check if skill already exists
    match services::skills::skill_exists(&state, &id).await {
        Ok(true) => {
            return Err((
                StatusCode::CONFLICT,
                format!("Skill {} already exists", id),
            ))
        }
        Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
        _ => {}
    }

    // Parse markdown
    let skill = match services::skills::import_skill_from_markdown(&id, &payload.markdown) {
        Ok(s) => s,
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("Invalid markdown format: {}", e),
            ))
        }
    };

    match services::skills::create_skill(&state, skill.clone()).await {
        Ok(_) => Ok((StatusCode::CREATED, Json(ApiResponse::ok(skill)))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

/// Validate skill ID format (lowercase, numbers, hyphens)
fn is_valid_skill_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !id.starts_with('-')
        && !id.ends_with('-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_core::AppCore;
    use std::sync::Arc;
    use tempfile::{TempDir, tempdir};

    async fn create_test_app() -> (Arc<AppCore>, TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let app = Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap());
        (app, temp_dir)
    }

    #[tokio::test]
    async fn test_list_skills_empty() {
        let (app, _tmp_dir) = create_test_app().await;
        let result = list_skills(State(app)).await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert!(response.success);
        assert_eq!(response.data.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_create_skill() {
        let (app, _tmp_dir) = create_test_app().await;

        let request = CreateSkillRequest {
            id: Some("test-skill".to_string()),
            name: "Test Skill".to_string(),
            description: Some("A test skill".to_string()),
            tags: Some(vec!["test".to_string()]),
            content: "# Test Content".to_string(),
        };

        let result = create_skill(State(app), Json(request)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_skill_auto_id() {
        let (app, _tmp_dir) = create_test_app().await;

        let request = CreateSkillRequest {
            id: None, // Auto-generate ID
            name: "Auto ID Skill".to_string(),
            description: None,
            tags: None,
            content: "# Auto ID".to_string(),
        };

        let result = create_skill(State(app), Json(request)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_skill_invalid_id() {
        let (app, _tmp_dir) = create_test_app().await;

        let request = CreateSkillRequest {
            id: Some("Invalid_ID".to_string()),
            name: "Test".to_string(),
            description: None,
            tags: None,
            content: "# Test".to_string(),
        };

        let result = create_skill(State(app), Json(request)).await;
        assert!(result.is_err());
        if let Err((status, _)) = result {
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }
    }

    #[tokio::test]
    async fn test_get_skill() {
        let (app, _tmp_dir) = create_test_app().await;

        let create_req = CreateSkillRequest {
            id: Some("test-skill".to_string()),
            name: "Test Skill".to_string(),
            description: None,
            tags: None,
            content: "# Test".to_string(),
        };
        create_skill(State(app.clone()), Json(create_req))
            .await
            .unwrap();

        let result = get_skill(State(app), Path("test-skill".to_string())).await;
        assert!(result.is_ok());
        let skill = result.unwrap().0.data.unwrap();
        assert_eq!(skill.id, "test-skill");
        assert_eq!(skill.name, "Test Skill");
    }

    #[tokio::test]
    async fn test_get_skill_not_found() {
        let (app, _tmp_dir) = create_test_app().await;

        let result = get_skill(State(app), Path("nonexistent".to_string())).await;
        assert!(result.is_err());
        if let Err((status, _)) = result {
            assert_eq!(status, StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn test_update_skill() {
        let (app, _tmp_dir) = create_test_app().await;

        let create_req = CreateSkillRequest {
            id: Some("test-skill".to_string()),
            name: "Test Skill".to_string(),
            description: None,
            tags: None,
            content: "# Test".to_string(),
        };
        create_skill(State(app.clone()), Json(create_req))
            .await
            .unwrap();

        let update_req = UpdateSkillRequest {
            name: Some("Updated Name".to_string()),
            description: Some("New description".to_string()),
            tags: None,
            content: None,
        };

        let result = update_skill(
            State(app),
            Path("test-skill".to_string()),
            Json(update_req),
        )
        .await;

        assert!(result.is_ok());
        let skill = result.unwrap().0.data.unwrap();
        assert_eq!(skill.name, "Updated Name");
        assert_eq!(skill.description, Some("New description".to_string()));
    }

    #[tokio::test]
    async fn test_delete_skill() {
        let (app, _tmp_dir) = create_test_app().await;

        let create_req = CreateSkillRequest {
            id: Some("test-skill".to_string()),
            name: "Test Skill".to_string(),
            description: None,
            tags: None,
            content: "# Test".to_string(),
        };
        create_skill(State(app.clone()), Json(create_req))
            .await
            .unwrap();

        let result = delete_skill(State(app.clone()), Path("test-skill".to_string())).await;
        assert!(result.is_ok());

        let get_result = get_skill(State(app), Path("test-skill".to_string())).await;
        assert!(get_result.is_err());
    }

    #[tokio::test]
    async fn test_export_skill() {
        let (app, _tmp_dir) = create_test_app().await;

        let create_req = CreateSkillRequest {
            id: Some("test-skill".to_string()),
            name: "Test Skill".to_string(),
            description: Some("A test".to_string()),
            tags: Some(vec!["test".to_string()]),
            content: "# Test Content".to_string(),
        };
        create_skill(State(app.clone()), Json(create_req))
            .await
            .unwrap();

        let result = export_skill(State(app), Path("test-skill".to_string())).await;
        assert!(result.is_ok());
        let export = result.unwrap().0.data.unwrap();
        assert_eq!(export.id, "test-skill");
        assert_eq!(export.filename, "test-skill.md");
        assert!(export.markdown.contains("name: Test Skill"));
        assert!(export.markdown.contains("# Test Content"));
    }

    #[tokio::test]
    async fn test_import_skill() {
        let (app, _tmp_dir) = create_test_app().await;

        let markdown = r#"---
name: Imported Skill
description: An imported skill
tags:
  - imported
---

# Imported Content"#;

        let request = ImportSkillRequest {
            id: Some("imported-skill".to_string()),
            markdown: markdown.to_string(),
        };

        let result = import_skill(State(app.clone()), Json(request)).await;
        assert!(result.is_ok());

        let get_result = get_skill(State(app), Path("imported-skill".to_string())).await;
        assert!(get_result.is_ok());
        let skill = get_result.unwrap().0.data.unwrap();
        assert_eq!(skill.name, "Imported Skill");
        assert_eq!(skill.description, Some("An imported skill".to_string()));
    }

    #[tokio::test]
    async fn test_import_skill_auto_id() {
        let (app, _tmp_dir) = create_test_app().await;

        let markdown = r#"---
name: Auto Import
---

# Content"#;

        let request = ImportSkillRequest {
            id: None, // Auto-generate ID
            markdown: markdown.to_string(),
        };

        let result = import_skill(State(app), Json(request)).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_is_valid_skill_id() {
        assert!(is_valid_skill_id("my-skill"));
        assert!(is_valid_skill_id("skill123"));
        assert!(is_valid_skill_id("my-skill-v2"));

        assert!(!is_valid_skill_id(""));
        assert!(!is_valid_skill_id("My-Skill"));
        assert!(!is_valid_skill_id("my_skill"));
        assert!(!is_valid_skill_id("-my-skill"));
        assert!(!is_valid_skill_id("my-skill-"));
    }
}
