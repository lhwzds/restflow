//! RestFlow Tauri Desktop Application Entry Point
//!
//! This is the main entry point for the RestFlow desktop application.
//! It initializes the Tauri runtime and registers all command handlers.

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use restflow_tauri_lib::commands;
use restflow_tauri_lib::AppState;
use tauri::Manager;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "restflow_tauri=info,restflow_workflow=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting RestFlow Desktop Application");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Initialize application state
            let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");

            let db_path = get_db_path(app);
            info!(db_path = %db_path, "Initializing database");

            let state = rt.block_on(async {
                AppState::new(&db_path)
                    .await
                    .expect("Failed to initialize AppState")
            });

            app.manage(state);

            info!("RestFlow initialized successfully");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Skills
            commands::list_skills,
            commands::get_skill,
            commands::create_skill,
            commands::update_skill,
            commands::delete_skill,
            commands::export_skill,
            commands::import_skill,
            // Workflows
            commands::list_workflows,
            commands::get_workflow,
            commands::create_workflow,
            commands::update_workflow,
            commands::delete_workflow,
            commands::execute_workflow,
            commands::get_workflow_executions,
            // Agents
            commands::list_agents,
            commands::get_agent,
            commands::create_agent,
            commands::update_agent,
            commands::delete_agent,
            commands::execute_agent,
            commands::execute_agent_inline,
            // Secrets
            commands::list_secrets,
            commands::create_secret,
            commands::update_secret,
            commands::delete_secret,
            commands::has_secret,
            // Config
            commands::get_config,
            commands::update_config,
            commands::get_available_models,
            commands::get_available_tools,
            commands::check_python_status,
            commands::init_python,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Get the database path for the application
fn get_db_path(app: &tauri::App) -> String {
    // Try to get app data directory
    if let Ok(app_data_dir) = app.path().app_data_dir() {
        // Create the directory if it doesn't exist
        if let Err(e) = std::fs::create_dir_all(&app_data_dir) {
            tracing::warn!(error = %e, "Failed to create app data directory, using current directory");
            return "restflow.db".to_string();
        }

        let db_path = app_data_dir.join("restflow.db");
        return db_path.to_string_lossy().to_string();
    }

    // Fallback to current directory
    "restflow.db".to_string()
}
