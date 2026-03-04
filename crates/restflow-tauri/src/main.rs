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
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// RestFlow Desktop Application
#[derive(Parser, Debug)]
#[command(name = "restflow-tauri")]
#[command(about = "RestFlow - Visual workflow automation with AI integration")]
struct Args {}

const MAIN_WINDOW_LABEL: &str = "main";
const TRAY_WINDOW_LABEL: &str = "tray-dashboard";
const TRAY_ICON_ID: &str = "restflow-tray";

const TRAY_MENU_OPEN_DASHBOARD: &str = "tray.open-dashboard";
const TRAY_MENU_OPEN_MAIN: &str = "tray.open-main";
const TRAY_MENU_HIDE_MAIN: &str = "tray.hide-main";
const TRAY_MENU_QUIT: &str = "tray.quit";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayMenuAction {
    OpenDashboard,
    OpenMain,
    HideMain,
    Quit,
    Ignore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayIconAction {
    ToggleDashboard,
    OpenMain,
    Ignore,
}

fn tray_menu_action(menu_id: &str) -> TrayMenuAction {
    match menu_id {
        TRAY_MENU_OPEN_DASHBOARD => TrayMenuAction::OpenDashboard,
        TRAY_MENU_OPEN_MAIN => TrayMenuAction::OpenMain,
        TRAY_MENU_HIDE_MAIN => TrayMenuAction::HideMain,
        TRAY_MENU_QUIT => TrayMenuAction::Quit,
        _ => TrayMenuAction::Ignore,
    }
}

fn tray_icon_action_from_input(
    button: MouseButton,
    button_state: Option<MouseButtonState>,
    is_double_click: bool,
) -> TrayIconAction {
    if is_double_click && button == MouseButton::Left {
        return TrayIconAction::OpenMain;
    }

    if !is_double_click && button == MouseButton::Left && button_state == Some(MouseButtonState::Up)
    {
        return TrayIconAction::ToggleDashboard;
    }

    TrayIconAction::Ignore
}

fn tray_icon_action(event: &TrayIconEvent) -> TrayIconAction {
    match event {
        TrayIconEvent::Click {
            button,
            button_state,
            ..
        } => tray_icon_action_from_input(*button, Some(*button_state), false),
        TrayIconEvent::DoubleClick { button, .. } => {
            tray_icon_action_from_input(*button, None, true)
        }
        _ => TrayIconAction::Ignore,
    }
}

fn ensure_tray_dashboard_window<R: tauri::Runtime>(
    app: &AppHandle<R>,
) -> tauri::Result<WebviewWindow<R>> {
    if let Some(window) = app.get_webview_window(TRAY_WINDOW_LABEL) {
        return Ok(window);
    }

    WebviewWindowBuilder::new(
        app,
        TRAY_WINDOW_LABEL,
        // The tray window uses the same entry point and routes by window label in frontend bootstrap.
        WebviewUrl::App("index.html".into()),
    )
    .title("RestFlow Mini Dashboard")
    .inner_size(420.0, 560.0)
    .min_inner_size(360.0, 440.0)
    .visible(false)
    .focused(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(true)
    .build()
}

fn hide_tray_dashboard_window<R: tauri::Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(TRAY_WINDOW_LABEL)
        && window.is_visible().unwrap_or(false)
    {
        window.hide()?;
    }

    Ok(())
}

fn show_tray_dashboard_window<R: tauri::Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let window = ensure_tray_dashboard_window(app)?;

    window.show()?;
    let _ = window.unminimize();
    let _ = window.set_focus();

    Ok(())
}

fn toggle_tray_dashboard_window<R: tauri::Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(TRAY_WINDOW_LABEL)
        && window.is_visible().unwrap_or(false)
    {
        window.hide()?;
        return Ok(());
    }

    show_tray_dashboard_window(app)
}

fn show_main_window<R: tauri::Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        window.show()?;
        let _ = window.unminimize();
        let _ = window.set_focus();
    }

    hide_tray_dashboard_window(app)
}

fn hide_main_window<R: tauri::Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        window.hide()?;
    }

    Ok(())
}

fn setup_tray<R: tauri::Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let open_dashboard =
        MenuItemBuilder::with_id(TRAY_MENU_OPEN_DASHBOARD, "Open Dashboard").build(app)?;
    let open_main = MenuItemBuilder::with_id(TRAY_MENU_OPEN_MAIN, "Open Main Window").build(app)?;
    let hide_main = MenuItemBuilder::with_id(TRAY_MENU_HIDE_MAIN, "Hide Main Window").build(app)?;
    let quit = MenuItemBuilder::with_id(TRAY_MENU_QUIT, "Quit").build(app)?;

    let menu = MenuBuilder::new(app)
        .items(&[&open_dashboard, &open_main, &hide_main])
        .separator()
        .item(&quit)
        .build()?;

    let mut tray_builder = TrayIconBuilder::with_id(TRAY_ICON_ID)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("RestFlow");

    if let Some(icon) = app.default_window_icon().cloned() {
        tray_builder = tray_builder.icon(icon);
    }

    tray_builder.build(app)?;
    Ok(())
}

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
            setup_tray(app.handle())?;

            info!("RestFlow initialized successfully");
            info!("Press Cmd+Option+I (macOS) or Ctrl+Shift+I (Windows/Linux) to toggle DevTools");
            Ok(())
        })
        .on_menu_event(|app, event| match tray_menu_action(event.id().as_ref()) {
            TrayMenuAction::OpenDashboard => {
                if let Err(err) = show_tray_dashboard_window(app) {
                    tracing::warn!(error = %err, "Failed to show tray dashboard window");
                }
            }
            TrayMenuAction::OpenMain => {
                if let Err(err) = show_main_window(app) {
                    tracing::warn!(error = %err, "Failed to show main window");
                }
            }
            TrayMenuAction::HideMain => {
                if let Err(err) = hide_main_window(app) {
                    tracing::warn!(error = %err, "Failed to hide main window");
                }
            }
            TrayMenuAction::Quit => {
                shutdown_runtime_state(app);
                app.exit(0);
            }
            TrayMenuAction::Ignore => {}
        })
        .on_tray_icon_event(|app, event| match tray_icon_action(&event) {
            TrayIconAction::ToggleDashboard => {
                if let Err(err) = toggle_tray_dashboard_window(app) {
                    tracing::warn!(error = %err, "Failed to toggle tray dashboard window");
                }
            }
            TrayIconAction::OpenMain => {
                if let Err(err) = show_main_window(app) {
                    tracing::warn!(error = %err, "Failed to show main window from tray double click");
                }
            }
            TrayIconAction::Ignore => {}
        })
        .on_window_event(|window, event| {
            match event {
                tauri::WindowEvent::Focused(false) if window.label() == TRAY_WINDOW_LABEL => {
                    let _ = window.hide();
                }
                tauri::WindowEvent::CloseRequested { api, .. } if window.label() == TRAY_WINDOW_LABEL => {
                    api.prevent_close();
                    let _ = window.hide();
                }
                tauri::WindowEvent::CloseRequested { api, .. } if window.label() == MAIN_WINDOW_LABEL => {
                    // Keep app alive in tray when users close the main window.
                    api.prevent_close();
                    let _ = window.hide();
                }
                _ => {}
            }
        })
        .invoke_handler(ipc_builder.invoke_handler())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_menu_action_maps_known_ids() {
        assert_eq!(
            tray_menu_action(TRAY_MENU_OPEN_DASHBOARD),
            TrayMenuAction::OpenDashboard
        );
        assert_eq!(
            tray_menu_action(TRAY_MENU_OPEN_MAIN),
            TrayMenuAction::OpenMain
        );
        assert_eq!(
            tray_menu_action(TRAY_MENU_HIDE_MAIN),
            TrayMenuAction::HideMain
        );
        assert_eq!(tray_menu_action(TRAY_MENU_QUIT), TrayMenuAction::Quit);
    }

    #[test]
    fn tray_menu_action_ignores_unknown_ids() {
        assert_eq!(tray_menu_action("unknown"), TrayMenuAction::Ignore);
    }

    #[test]
    fn tray_icon_action_toggles_dashboard_on_left_button_up_click() {
        let action =
            tray_icon_action_from_input(MouseButton::Left, Some(MouseButtonState::Up), false);
        assert_eq!(action, TrayIconAction::ToggleDashboard);
    }

    #[test]
    fn tray_icon_action_opens_main_on_left_double_click() {
        let action = tray_icon_action_from_input(MouseButton::Left, None, true);
        assert_eq!(action, TrayIconAction::OpenMain);
    }

    #[test]
    fn tray_icon_action_ignores_non_matching_inputs() {
        let right_click_action =
            tray_icon_action_from_input(MouseButton::Right, Some(MouseButtonState::Up), false);
        assert_eq!(right_click_action, TrayIconAction::Ignore);

        let left_button_down_action =
            tray_icon_action_from_input(MouseButton::Left, Some(MouseButtonState::Down), false);
        assert_eq!(left_button_down_action, TrayIconAction::Ignore);
    }
}
