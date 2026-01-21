//! Workflow-related Tauri commands

use crate::state::AppState;
use restflow_workflow::models::ExecutionHistoryPage;
use restflow_workflow::Workflow;
use serde_json::Value;
use tauri::State;

/// List all workflows
#[tauri::command]
pub async fn list_workflows(state: State<'_, AppState>) -> Result<Vec<Workflow>, String> {
    state
        .core
        .storage
        .workflows
        .list_workflows()
        .map_err(|e| e.to_string())
}

/// Get a workflow by ID
#[tauri::command]
pub async fn get_workflow(state: State<'_, AppState>, id: String) -> Result<Workflow, String> {
    state
        .core
        .storage
        .workflows
        .get_workflow(&id)
        .map_err(|e| e.to_string())
}

/// Create a new workflow
#[tauri::command]
pub async fn create_workflow(
    state: State<'_, AppState>,
    workflow: Workflow,
) -> Result<Workflow, String> {
    state
        .core
        .storage
        .workflows
        .create_workflow(&workflow)
        .map_err(|e| e.to_string())?;

    Ok(workflow)
}

/// Update an existing workflow
#[tauri::command]
pub async fn update_workflow(
    state: State<'_, AppState>,
    id: String,
    workflow: Workflow,
) -> Result<Workflow, String> {
    // Ensure the ID matches
    let mut updated_workflow = workflow;
    updated_workflow.id = id.clone();

    state
        .core
        .storage
        .workflows
        .update_workflow(&id, &updated_workflow)
        .map_err(|e| e.to_string())?;

    Ok(updated_workflow)
}

/// Delete a workflow by ID
#[tauri::command]
pub async fn delete_workflow(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .core
        .storage
        .workflows
        .delete_workflow(&id)
        .map_err(|e| e.to_string())
}

/// Execute a workflow
#[tauri::command]
pub async fn execute_workflow(
    state: State<'_, AppState>,
    id: String,
    input: Option<Value>,
) -> Result<String, String> {
    let input_value = input.unwrap_or(Value::Null);

    state
        .core
        .executor
        .submit(id, input_value)
        .await
        .map_err(|e| e.to_string())
}

/// Get workflow execution history (paginated)
#[tauri::command]
pub async fn get_workflow_executions(
    state: State<'_, AppState>,
    workflow_id: String,
    page: Option<usize>,
    page_size: Option<usize>,
) -> Result<ExecutionHistoryPage, String> {
    let page = page.unwrap_or(1);
    let page_size = page_size.unwrap_or(50);

    state
        .core
        .storage
        .execution_history
        .list_paginated(&workflow_id, page, page_size)
        .map_err(|e| e.to_string())
}
