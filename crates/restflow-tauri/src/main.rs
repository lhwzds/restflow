//! RestFlow Tauri Desktop Application Entry Point
//!
//! This is the main entry point for the RestFlow desktop application.
//! It initializes the Tauri runtime and registers all command handlers.
//!
//! # MCP Mode
//!
//! When run with `--mcp-mode`, the application starts as an MCP server
//! instead of the GUI, allowing AI assistants like Claude Code to interact
//! with RestFlow via the Model Context Protocol.

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use clap::Parser;
use restflow_tauri_lib::AppState;
use restflow_tauri_lib::RestFlowMcpServer;
use restflow_tauri_lib::{RealAgentExecutor, TelegramNotifier};
use restflow_tauri_lib::commands;
use restflow_tauri_lib::commands::PtyState;
use restflow_tauri_lib::commands::AuthState;
use restflow_tauri_lib::commands::pty::save_all_terminal_history_sync;
use tauri::Manager;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// RestFlow Desktop Application
#[derive(Parser, Debug)]
#[command(name = "restflow-tauri")]
#[command(about = "RestFlow - Visual workflow automation with AI integration")]
struct Args {
    /// Run as MCP server instead of GUI
    #[arg(long)]
    mcp_mode: bool,

    /// Database path (defaults to app data directory)
    #[arg(long)]
    db_path: Option<String>,
}

fn main() {
    let args = Args::parse();
    // Initialize tracing
    // For MCP mode, use stderr to avoid interfering with stdio transport
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| {
            if args.mcp_mode {
                "restflow_tauri=warn,restflow_core=warn".into()
            } else {
                "restflow_tauri=info,restflow_core=info".into()
            }
        });

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    // If MCP mode is requested, run as MCP server
    if args.mcp_mode {
        run_mcp_server(args.db_path);
        return;
    }

    info!("Starting RestFlow Desktop Application");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_decorum::init())
        .setup(|app| {
            // Set traffic lights position on macOS
            // This keeps them always visible with overlay titlebar
            #[cfg(target_os = "macos")]
            {
                use tauri_plugin_decorum::WebviewWindowExt;
                if let Some(window) = app.get_webview_window("main") {
                    // Position traffic lights at (12, 16) from top-left
                    let _ = window.set_traffic_lights_inset(12.0, 16.0);
                }
            }
            // Initialize application state
            let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");

            let db_path = get_db_path(app);
            info!(db_path = %db_path, "Initializing database");

            let state = rt.block_on(async {
                AppState::new(&db_path)
                    .await
                    .expect("Failed to initialize AppState")
            });

            // Start the agent task runner with real executor and Telegram notifier
            rt.block_on(async {
                let storage = state.core.storage.clone();
                let secrets = std::sync::Arc::new(state.core.storage.secrets.clone());
                let executor = RealAgentExecutor::new(storage);
                let notifier = TelegramNotifier::new(secrets);

                if let Err(e) = state.start_runner(executor, notifier, None).await {
                    tracing::warn!(error = %e, "Failed to start agent task runner");
                } else {
                    info!("Agent task runner started");
                }
            });

            // Mark all running terminal sessions as stopped on startup
            // (PTY processes don't survive app restart)
            match state.core.storage.terminal_sessions.mark_all_stopped() {
                Ok(count) if count > 0 => {
                    info!(count, "Marked stale terminal sessions as stopped");
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to clean up stale terminal sessions");
                }
            }

            app.manage(state);

            // Initialize PTY state
            app.manage(PtyState::new());

            // Initialize Auth Profile Manager state
            app.manage(AuthState::new());

            info!("RestFlow initialized successfully");
            info!("Press Cmd+Option+I (macOS) or Ctrl+Shift+I (Windows/Linux) to toggle DevTools");
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                let app = window.app_handle();

                // Stop the agent task runner gracefully
                if let Some(app_state) = app.try_state::<AppState>() {
                    info!("Stopping agent task runner...");
                    // Use tokio's current thread runtime for sync context
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("Failed to create shutdown runtime");
                    rt.block_on(async {
                        if let Err(e) = app_state.stop_runner().await {
                            tracing::warn!(error = %e, "Error stopping agent task runner");
                        }
                    });
                    info!("Agent task runner stopped");
                }

                // Save all terminal history before closing
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
            // Agent Tasks
            commands::list_agent_tasks,
            commands::list_agent_tasks_by_status,
            commands::get_agent_task,
            commands::create_agent_task,
            commands::update_agent_task,
            commands::delete_agent_task,
            commands::pause_agent_task,
            commands::resume_agent_task,
            commands::cancel_agent_task,
            commands::get_agent_task_events,
            commands::get_runnable_agent_tasks,
            commands::run_agent_task_streaming,
            commands::get_active_agent_tasks,
            commands::get_task_stream_event_name,
            commands::get_heartbeat_event_name,
            commands::emit_test_task_event,
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
            commands::update_terminal_session,
            commands::delete_terminal_session,
            // Memory
            commands::search_memory,
            commands::search_memory_advanced,
            commands::get_memory_chunk,
            commands::list_memory_chunks,
            commands::list_memory_chunks_by_tag,
            commands::create_memory_chunk,
            commands::delete_memory_chunk,
            commands::delete_memory_chunks_for_agent,
            commands::get_memory_session,
            commands::list_memory_sessions,
            commands::list_memory_chunks_for_session,
            commands::create_memory_session,
            commands::delete_memory_session,
            commands::get_memory_stats,
            commands::export_memory_markdown,
            commands::export_memory_session_markdown,
            commands::export_memory_advanced,
            // Chat Sessions
            commands::create_chat_session,
            commands::list_chat_sessions,
            commands::list_chat_session_summaries,
            commands::get_chat_session,
            commands::rename_chat_session,
            commands::delete_chat_session,
            commands::add_chat_message,
            commands::send_chat_message,
            commands::list_chat_sessions_by_agent,
            commands::list_chat_sessions_by_skill,
            commands::get_chat_session_count,
            commands::clear_old_chat_sessions,
            // Auth Profiles
            commands::auth_initialize,
            commands::auth_discover,
            commands::auth_list_profiles,
            commands::auth_get_profiles_for_provider,
            commands::auth_get_available_profiles,
            commands::auth_get_profile,
            commands::auth_add_profile,
            commands::auth_remove_profile,
            commands::auth_update_profile,
            commands::auth_enable_profile,
            commands::auth_disable_profile,
            commands::auth_mark_success,
            commands::auth_mark_failure,
            commands::auth_get_api_key,
            commands::auth_get_summary,
            commands::auth_clear,
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

/// Get the default database path for MCP mode (without Tauri app context)
#[allow(dead_code)]
fn get_default_db_path() -> String {
    // Try to use the same path as Tauri would use
    if let Some(data_dir) = dirs::data_dir() {
        let app_data_dir = data_dir.join("com.restflow.app");
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

/// Get the default database path for MCP mode
/// Uses a separate database file to avoid conflicts with the GUI
fn get_mcp_db_path() -> String {
    // Try to use the same directory as the GUI database, but with a different filename
    if let Some(data_dir) = dirs::data_dir() {
        let app_dir = data_dir.join("com.restflow.app");
        if std::fs::create_dir_all(&app_dir).is_ok() {
            return app_dir.join("restflow-mcp.db").to_string_lossy().to_string();
        }
    }
    // Fallback to current directory
    "restflow-mcp.db".to_string()
}

/// Run RestFlow as an MCP server
fn run_mcp_server(db_path: Option<String>) {
    tracing::info!("Starting RestFlow MCP Server");

    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");

    rt.block_on(async {
        let db_path = db_path.unwrap_or_else(get_mcp_db_path);
        tracing::info!(db_path = %db_path, "Initializing database for MCP server");

        let state = AppState::new(&db_path)
            .await
            .expect("Failed to initialize AppState");

        let mcp_server = RestFlowMcpServer::new(state.core);

        if let Err(e) = mcp_server.run().await {
            tracing::error!(error = %e, "MCP server error");
            std::process::exit(1);
        }
    });
}
