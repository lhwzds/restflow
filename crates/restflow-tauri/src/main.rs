//! RestFlow Tauri Desktop Application Entry Point
//!
//! This is the main entry point for the RestFlow desktop application.
//! It initializes the Tauri runtime and registers all command handlers.

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use clap::Parser;
use restflow_tauri_lib::AppState;
use restflow_tauri_lib::commands::pty::save_all_terminal_history_sync;
use restflow_tauri_lib::ipc_bindings::{build_ipc_builder, export_ipc_bindings};
use tauri::{AppHandle, Manager};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// RestFlow Desktop Application
#[derive(Parser, Debug)]
#[command(name = "restflow-tauri")]
#[command(about = "RestFlow - Visual workflow automation with AI integration")]
struct Args {}

const MAIN_WINDOW_LABEL: &str = "main";
fn shutdown_runtime_state<R: tauri::Runtime>(app: &AppHandle<R>) {
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

    #[cfg(debug_assertions)]
    export_ipc_bindings().expect("failed to export tauri-specta bindings");

    let ipc_builder = build_ipc_builder();

    // Change CWD to ~/.restflow/ so that WebKit temp files (e.g. from MediaRecorder)
    // don't land in the project or user home directory.
    if let Ok(dir) = restflow_core::paths::ensure_restflow_dir() {
        let _ = std::env::set_current_dir(&dir);
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
            let state = rt
                .block_on(async { AppState::with_ipc().await })
                .map_err(|err| std::io::Error::other(err.to_string()))?;

            let daemon_status = rt
                .block_on(async { state.executor().ensure_daemon_handshake().await })
                .map_err(|err| std::io::Error::other(err.to_string()))?;
            info!(
                daemon_pid = daemon_status.pid,
                daemon_version = %daemon_status.daemon_version,
                protocol_version = %daemon_status.protocol_version,
                "Daemon handshake completed"
            );

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
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { .. } if window.label() == MAIN_WINDOW_LABEL => {
                shutdown_runtime_state(window.app_handle());
            }
            _ => {}
        })
        .invoke_handler(ipc_builder.invoke_handler())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
