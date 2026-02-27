//! RestFlow Tauri Desktop Application Entry Point
//!
//! This is the main entry point for the RestFlow desktop application.
//! It initializes the Tauri runtime and registers all command handlers.

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use clap::Parser;
use restflow_core::daemon::ensure_daemon_running;
use restflow_tauri_lib::AppState;
use restflow_tauri_lib::commands;
use restflow_tauri_lib::commands::pty::save_all_terminal_history_sync;
use tauri::Manager;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// RestFlow Desktop Application
#[derive(Parser, Debug)]
#[command(name = "restflow-tauri")]
#[command(about = "RestFlow - Visual workflow automation with AI integration")]
struct Args {}

fn main() {
    Args::parse();
    // Initialize tracing
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "restflow_tauri=info,restflow_core=info".into());

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    info!("Starting RestFlow Desktop Application");

    if let Ok(rt) = tokio::runtime::Runtime::new() {
        rt.block_on(async {
            if let Err(err) = ensure_daemon_running().await {
                warn!(error = %err, "Failed to start daemon, continuing with direct access");
            }
        });
    } else {
        warn!("Failed to create runtime for daemon startup");
    }

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
            // Initialize application state (IPC mode, no direct DB access)
            let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
            let state = rt.block_on(async {
                AppState::with_ipc()
                    .await
                    .expect("Failed to initialize AppState")
            });

            // Mark all running terminal sessions as stopped on startup
            rt.block_on(async {
                match state.executor().mark_all_terminal_sessions_stopped().await {
                    Ok(count) if count > 0 => {
                        info!(count, "Marked stale terminal sessions as stopped");
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to clean up stale terminal sessions");
                    }
                }
            });

            app.manage(state);

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
                if let Some(app_state) = app.try_state::<AppState>() {
                    info!("Saving terminal history before close...");
                    save_all_terminal_history_sync(&app_state);
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
            // Agent Tasks
            commands::list_background_agents,
            commands::list_background_agents_by_status,
            commands::get_background_agent,
            commands::create_background_agent,
            commands::update_background_agent,
            commands::delete_background_agent,
            commands::pause_background_agent,
            commands::resume_background_agent,
            commands::cancel_background_agent,
            commands::get_background_agent_events,
            commands::get_runnable_background_agents,
            commands::run_background_agent_streaming,
            commands::get_active_background_agents,
            commands::get_background_agent_stream_event_name,
            commands::get_heartbeat_event_name,
            commands::emit_test_background_agent_event,
            commands::steer_task,
            // Hooks
            commands::list_hooks,
            commands::create_hook,
            commands::update_hook,
            commands::delete_hook,
            commands::test_hook,
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
            commands::update_chat_session,
            commands::rename_chat_session,
            commands::delete_chat_session,
            commands::add_chat_message,
            commands::send_chat_message,
            commands::list_chat_sessions_by_agent,
            commands::list_chat_sessions_by_skill,
            commands::get_chat_session_count,
            commands::clear_old_chat_sessions,
            commands::execute_chat_session,
            commands::list_chat_execution_events,
            commands::send_chat_message_stream,
            commands::steer_chat_stream,
            commands::cancel_chat_stream,
            commands::get_session_change_event_name,
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
            // Voice
            commands::transcribe_audio,
            commands::transcribe_audio_stream,
            commands::save_voice_message,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
