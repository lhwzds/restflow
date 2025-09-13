// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use backend::{AppCore, services};
use std::sync::Arc;
use tauri::State;

// Workflow commands
#[tauri::command(rename_all = "snake_case")]
async fn list_workflows(
    core: State<'_, Arc<AppCore>>,
) -> Result<Vec<backend::models::Workflow>, String> {
    services::workflow::list_workflows(&core).await
}

#[tauri::command(rename_all = "snake_case")]
async fn get_workflow(
    id: String,
    core: State<'_, Arc<AppCore>>,
) -> Result<backend::models::Workflow, String> {
    services::workflow::get_workflow(&core, &id).await
}

#[tauri::command(rename_all = "snake_case")]
async fn create_workflow(
    workflow: backend::models::Workflow,
    core: State<'_, Arc<AppCore>>,
) -> Result<backend::models::Workflow, String> {
    services::workflow::create_workflow(&core, workflow).await
}

#[tauri::command(rename_all = "snake_case")]
async fn update_workflow(
    id: String,
    workflow: backend::models::Workflow,
    core: State<'_, Arc<AppCore>>,
) -> Result<backend::models::Workflow, String> {
    services::workflow::update_workflow(&core, &id, workflow).await
}

#[tauri::command(rename_all = "snake_case")]
async fn delete_workflow(id: String, core: State<'_, Arc<AppCore>>) -> Result<(), String> {
    services::workflow::delete_workflow(&core, &id).await
}

// Execution commands
#[tauri::command(rename_all = "snake_case")]
async fn execute_workflow_sync(
    workflow_id: String,
    input: serde_json::Value,
    core: State<'_, Arc<AppCore>>,
) -> Result<serde_json::Value, String> {
    services::workflow::execute_workflow_by_id(&core, &workflow_id, input).await
}

#[tauri::command(rename_all = "snake_case")]
async fn submit_workflow(
    workflow_id: String,
    input: serde_json::Value,
    core: State<'_, Arc<AppCore>>,
) -> Result<String, String> {
    services::workflow::submit_workflow(&core, &workflow_id, input).await
}

// Task commands
#[tauri::command(rename_all = "snake_case")]
async fn get_task_status(
    task_id: String,
    core: State<'_, Arc<AppCore>>,
) -> Result<backend::models::Task, String> {
    services::task::get_task_status(&core, &task_id).await
}

// Config commands
#[tauri::command(rename_all = "snake_case")]
async fn get_config(
    core: State<'_, Arc<AppCore>>,
) -> Result<backend::storage::config::SystemConfig, String> {
    services::config::get_config(&core).await
}

#[tauri::command(rename_all = "snake_case")]
async fn update_config(
    config: backend::storage::config::SystemConfig,
    core: State<'_, Arc<AppCore>>,
) -> Result<(), String> {
    services::config::update_config(&core, config).await
}

// Trigger commands
#[tauri::command(rename_all = "snake_case")]
async fn activate_workflow(
    workflow_id: String,
    core: State<'_, Arc<AppCore>>,
) -> Result<(), String> {
    services::triggers::activate_workflow(&core, &workflow_id).await
}

#[tauri::command(rename_all = "snake_case")]
async fn deactivate_workflow(
    workflow_id: String,
    core: State<'_, Arc<AppCore>>,
) -> Result<(), String> {
    services::triggers::deactivate_workflow(&core, &workflow_id).await
}

#[tauri::command(rename_all = "snake_case")]
async fn get_trigger_status(
    workflow_id: String,
    core: State<'_, Arc<AppCore>>,
) -> Result<Option<backend::engine::trigger_manager::TriggerStatus>, String> {
    services::triggers::get_workflow_trigger_status(&core, &workflow_id).await
}

#[tauri::command(rename_all = "snake_case")]
async fn test_workflow(
    workflow_id: String,
    test_data: serde_json::Value,
    core: State<'_, Arc<AppCore>>,
) -> Result<String, String> {
    services::triggers::test_workflow_trigger(&core, &workflow_id, test_data).await
}

#[tauri::command(rename_all = "snake_case")]
async fn list_active_triggers(
    core: State<'_, Arc<AppCore>>,
) -> Result<Vec<backend::models::ActiveTrigger>, String> {
    services::triggers::list_active_triggers(&core).await
}

#[tauri::command(rename_all = "snake_case")]
async fn list_tasks(
    execution_id: Option<String>,
    status: Option<backend::models::TaskStatus>,
    limit: Option<u32>,
    core: State<'_, Arc<AppCore>>,
) -> Result<Vec<backend::models::Task>, String> {
    services::task::list_tasks(&core, execution_id, status, limit).await
}

#[tauri::command(rename_all = "snake_case")]
async fn execute_node(
    node: backend::models::Node,
    input: serde_json::Value,
    core: State<'_, Arc<AppCore>>,
) -> Result<String, String> {
    services::task::execute_node(&core, node, input).await
}

#[tauri::command(rename_all = "snake_case")]
async fn get_execution_status(
    execution_id: String,
    core: State<'_, Arc<AppCore>>,
) -> Result<Vec<backend::models::Task>, String> {
    services::task::get_execution_status(&core, &execution_id).await
}

// Agent commands
#[tauri::command(rename_all = "snake_case")]
async fn list_agents(
    core: State<'_, Arc<AppCore>>,
) -> Result<Vec<backend::storage::agent::StoredAgent>, String> {
    services::agent::list_agents(&core).await
}

#[tauri::command(rename_all = "snake_case")]
async fn get_agent(
    id: String,
    core: State<'_, Arc<AppCore>>,
) -> Result<backend::storage::agent::StoredAgent, String> {
    services::agent::get_agent(&core, &id).await
}

#[tauri::command(rename_all = "snake_case")]
async fn create_agent(
    name: String,
    agent: backend::node::agent::AgentNode,
    core: State<'_, Arc<AppCore>>,
) -> Result<backend::storage::agent::StoredAgent, String> {
    services::agent::create_agent(&core, name, agent).await
}

#[tauri::command(rename_all = "snake_case")]
async fn update_agent(
    id: String,
    name: Option<String>,
    agent: Option<backend::node::agent::AgentNode>,
    core: State<'_, Arc<AppCore>>,
) -> Result<backend::storage::agent::StoredAgent, String> {
    services::agent::update_agent(&core, &id, name, agent).await
}

#[tauri::command(rename_all = "snake_case")]
async fn delete_agent(id: String, core: State<'_, Arc<AppCore>>) -> Result<bool, String> {
    services::agent::delete_agent(&core, &id).await
}

#[tauri::command(rename_all = "snake_case")]
async fn execute_agent(
    id: String,
    input: String,
    core: State<'_, Arc<AppCore>>,
) -> Result<String, String> {
    services::agent::execute_agent(&core, &id, &input).await
}

#[tauri::command(rename_all = "snake_case")]
async fn execute_agent_inline(
    agent: backend::node::agent::AgentNode,
    input: String,
    _core: State<'_, Arc<AppCore>>,
) -> Result<String, String> {
    services::agent::execute_agent_inline(agent, &input).await
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
