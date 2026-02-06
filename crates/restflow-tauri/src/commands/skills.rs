//! Skill-related Tauri commands

use crate::state::AppState;
use restflow_core::Skill;
use tauri::State;

/// List all skills
#[tauri::command]
pub async fn list_skills(state: State<'_, AppState>) -> Result<Vec<Skill>, String> {
    state
        .executor()
        .list_skills()
        .await
        .map_err(|e| e.to_string())
}

/// Get a skill by ID
#[tauri::command]
pub async fn get_skill(state: State<'_, AppState>, id: String) -> Result<Skill, String> {
    state
        .executor()
        .get_skill(id.clone())
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Skill '{}' not found", id))
}

/// Create a new skill
#[tauri::command]
pub async fn create_skill(state: State<'_, AppState>, skill: Skill) -> Result<Skill, String> {
    state
        .executor()
        .create_skill(skill.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(skill)
}

/// Update an existing skill
#[tauri::command]
pub async fn update_skill(
    state: State<'_, AppState>,
    id: String,
    skill: Skill,
) -> Result<Skill, String> {
    // Ensure the ID matches
    let mut updated_skill = skill;
    updated_skill.id = id.clone();

    state
        .executor()
        .update_skill(id, updated_skill.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(updated_skill)
}

/// Delete a skill by ID
#[tauri::command]
pub async fn delete_skill(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .executor()
        .delete_skill(id)
        .await
        .map_err(|e| e.to_string())
}

/// Export a skill to JSON
#[tauri::command]
pub async fn export_skill(state: State<'_, AppState>, id: String) -> Result<String, String> {
    let skill = state
        .executor()
        .get_skill(id.clone())
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Skill '{}' not found", id))?;

    serde_json::to_string_pretty(&skill).map_err(|e| e.to_string())
}

/// Import a skill from JSON
#[tauri::command]
pub async fn import_skill(state: State<'_, AppState>, json: String) -> Result<Skill, String> {
    let skill: Skill = serde_json::from_str(&json).map_err(|e| e.to_string())?;

    state
        .executor()
        .create_skill(skill.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(skill)
}
