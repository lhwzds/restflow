//! RestFlow Tauri Desktop Application Entry Point
//!
//! This is the main entry point for the RestFlow desktop application.
//! It initializes the Tauri runtime and registers all command handlers.

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use restflow_tauri_lib::AppState;
use restflow_tauri_lib::commands;
use restflow_tauri_lib::commands::PtyState;
use restflow_tauri_lib::commands::pty::save_all_terminal_history_sync;
use tauri::Manager;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "restflow_tauri=info,restflow_core=info".into()),
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

            // Initialize PTY state
            app.manage(PtyState::new());

            info!("RestFlow initialized successfully");
            info!("Press Cmd+Option+I (macOS) or Ctrl+Shift+I (Windows/Linux) to toggle DevTools");
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                // Save all terminal history before closing
                let app = window.app_handle();
                if let (Some(pty_state), Some(app_state)) =
                    (app.try_state::<PtyState>(), app.try_state::<AppState>())
                {
                    info!("Saving terminal history before close...");
                    save_all_terminal_history_sync(&pty_state, &app_state);
                    info!("Terminal history saved");
                }
            }
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
            // Shell
            commands::execute_shell,
            // PTY
            commands::spawn_pty,
            commands::write_pty,
            commands::resize_pty,
            commands::close_pty,
            commands::get_pty_status,
            commands::get_pty_history,
            commands::save_terminal_history,
            commands::save_all_terminal_history,
            commands::restart_terminal,
            // Terminal Sessions
            commands::list_terminal_sessions,
            commands::get_terminal_session,
            commands::create_terminal_session,
            commands::rename_terminal_session,
            commands::delete_terminal_session,
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
