// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use restflow_core::{AppCore, services};
use std::sync::Arc;
use tauri::State;

// Workflow commands
#[tauri::command(rename_all = "snake_case")]
async fn list_workflows(
    core: State<'_, Arc<AppCore>>,
) -> Result<Vec<restflow_core::models::Workflow>, String> {
    services::workflow::list_workflows(&core).await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn get_workflow(
    id: String,
    core: State<'_, Arc<AppCore>>,
) -> Result<restflow_core::models::Workflow, String> {
    services::workflow::get_workflow(&core, &id).await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn create_workflow(
    workflow: restflow_core::models::Workflow,
    core: State<'_, Arc<AppCore>>,
) -> Result<restflow_core::models::Workflow, String> {
    services::workflow::create_workflow(&core, workflow).await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn update_workflow(
    id: String,
    workflow: restflow_core::models::Workflow,
    core: State<'_, Arc<AppCore>>,
) -> Result<restflow_core::models::Workflow, String> {
    services::workflow::update_workflow(&core, &id, workflow).await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn delete_workflow(id: String, core: State<'_, Arc<AppCore>>) -> Result<(), String> {
    services::workflow::delete_workflow(&core, &id).await
        .map_err(|e| e.to_string())
}

// Execution commands
#[tauri::command(rename_all = "snake_case")]
async fn execute_workflow_sync(
    workflow_id: String,
    input: serde_json::Value,
    core: State<'_, Arc<AppCore>>,
) -> Result<serde_json::Value, String> {
    services::workflow::execute_workflow_by_id(&core, &workflow_id, input).await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn submit_workflow(
    workflow_id: String,
    input: serde_json::Value,
    core: State<'_, Arc<AppCore>>,
) -> Result<String, String> {
    services::workflow::submit_workflow(&core, &workflow_id, input).await
        .map_err(|e| e.to_string())
}

// Task commands
#[tauri::command(rename_all = "snake_case")]
async fn get_task_status(
    task_id: String,
    core: State<'_, Arc<AppCore>>,
) -> Result<restflow_core::models::Task, String> {
    services::task::get_task_status(&core, &task_id).await
        .map_err(|e| e.to_string())
}

// Config commands
#[tauri::command(rename_all = "snake_case")]
async fn get_config(
    core: State<'_, Arc<AppCore>>,
) -> Result<restflow_core::storage::config::SystemConfig, String> {
    services::config::get_config(&core).await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn update_config(
    config: restflow_core::storage::config::SystemConfig,
    core: State<'_, Arc<AppCore>>,
) -> Result<(), String> {
    services::config::update_config(&core, config).await
        .map_err(|e| e.to_string())
}

// Trigger commands
#[tauri::command(rename_all = "snake_case")]
async fn activate_workflow(
    workflow_id: String,
    core: State<'_, Arc<AppCore>>,
) -> Result<(), String> {
    services::triggers::activate_workflow(&core, &workflow_id).await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn deactivate_workflow(
    workflow_id: String,
    core: State<'_, Arc<AppCore>>,
) -> Result<(), String> {
    services::triggers::deactivate_workflow(&core, &workflow_id).await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn get_trigger_status(
    workflow_id: String,
    core: State<'_, Arc<AppCore>>,
) -> Result<Option<restflow_core::engine::trigger_manager::TriggerStatus>, String> {
    services::triggers::get_workflow_trigger_status(&core, &workflow_id).await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn test_workflow(
    workflow_id: String,
    test_data: serde_json::Value,
    core: State<'_, Arc<AppCore>>,
) -> Result<String, String> {
    services::triggers::test_workflow_trigger(&core, &workflow_id, test_data).await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn list_active_triggers(
    core: State<'_, Arc<AppCore>>,
) -> Result<Vec<restflow_core::models::ActiveTrigger>, String> {
    services::triggers::list_active_triggers(&core).await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn list_tasks(
    execution_id: Option<String>,
    status: Option<restflow_core::models::TaskStatus>,
    limit: Option<u32>,
    core: State<'_, Arc<AppCore>>,
) -> Result<Vec<restflow_core::models::Task>, String> {
    services::task::list_tasks(&core, execution_id, status, limit).await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn execute_node(
    node: restflow_core::models::Node,
    input: serde_json::Value,
    core: State<'_, Arc<AppCore>>,
) -> Result<String, String> {
    services::task::execute_node(&core, node, input).await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn get_execution_status(
    execution_id: String,
    core: State<'_, Arc<AppCore>>,
) -> Result<Vec<restflow_core::models::Task>, String> {
    services::task::get_execution_status(&core, &execution_id).await
        .map_err(|e| e.to_string())
}

// Agent commands
#[tauri::command(rename_all = "snake_case")]
async fn list_agents(
    core: State<'_, Arc<AppCore>>,
) -> Result<Vec<restflow_core::storage::agent::StoredAgent>, String> {
    services::agent::list_agents(&core).await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn get_agent(
    id: String,
    core: State<'_, Arc<AppCore>>,
) -> Result<restflow_core::storage::agent::StoredAgent, String> {
    services::agent::get_agent(&core, &id).await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn create_agent(
    name: String,
    agent: restflow_core::node::agent::AgentNode,
    core: State<'_, Arc<AppCore>>,
) -> Result<restflow_core::storage::agent::StoredAgent, String> {
    services::agent::create_agent(&core, name, agent).await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn update_agent(
    id: String,
    name: Option<String>,
    agent: Option<restflow_core::node::agent::AgentNode>,
    core: State<'_, Arc<AppCore>>,
) -> Result<restflow_core::storage::agent::StoredAgent, String> {
    services::agent::update_agent(&core, &id, name, agent).await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn delete_agent(id: String, core: State<'_, Arc<AppCore>>) -> Result<(), String> {
    services::agent::delete_agent(&core, &id).await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn execute_agent(
    id: String,
    input: String,
    core: State<'_, Arc<AppCore>>,
) -> Result<String, String> {
    services::agent::execute_agent(&core, &id, &input).await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn execute_agent_inline(
    agent: restflow_core::node::agent::AgentNode,
    input: String,
    core: State<'_, Arc<AppCore>>,
) -> Result<String, String> {
    services::agent::execute_agent_inline(&core, agent, &input).await
        .map_err(|e| e.to_string())
}

fn main() {
    // Initialize backend core
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create runtime");

    let core = runtime.block_on(async {
        // Use the same database as the server mode
        let db_path = "restflow.db".to_string();

        Arc::new(
            AppCore::new(&db_path)
                .await
                .expect("Failed to initialize app core"),
        )
    });

    // Build Tauri application
    tauri::Builder::default()
        .manage(core)
        .invoke_handler(tauri::generate_handler![
            list_workflows,
            get_workflow,
            create_workflow,
            update_workflow,
            delete_workflow,
            execute_workflow_sync,
            submit_workflow,
            get_task_status,
            get_config,
            update_config,
            activate_workflow,
            deactivate_workflow,
            get_trigger_status,
            test_workflow,
            list_active_triggers,
            list_tasks,
            execute_node,
            get_execution_status,
            list_agents,
            get_agent,
            create_agent,
            update_agent,
            delete_agent,
            execute_agent,
            execute_agent_inline,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
