use anyhow::Result;
use crossterm::cursor::{Hide, Show};
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io;

use crate::controller::ShellController;
use crate::daemon_client::TuiDaemonClient;
use crate::event_loop::run_event_loop;
use crate::state::AppState;

use super::TuiLaunchOptions;

struct TerminalGuard {
    stdout: io::Stdout,
}

impl TerminalGuard {
    fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnableBracketedPaste, EnableMouseCapture, Hide)?;
        Ok(Self { stdout })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.stdout,
            DisableBracketedPaste,
            DisableMouseCapture,
            Show
        );
    }
}

pub async fn run_tui(options: TuiLaunchOptions) -> Result<()> {
    let controller = ShellController::new(TuiDaemonClient::new()?);

    let mut state = AppState::empty();
    state.set_pending_initial_message(options.message);

    if controller.daemon_running().await {
        let agent = controller
            .resolve_default_agent(options.agent.as_deref())
            .await?;
        if let Some(agent) = agent {
            state.set_default_agent(Some(agent.id.clone()), Some(agent.name.clone()));
            if let Some(session) = controller
                .resolve_or_create_session(&agent, options.session.as_deref())
                .await?
            {
                state.set_current_session(session);
            } else {
                state.set_pending_session_from_agent(&agent);
            }
            state.status = "Connected to daemon".to_string();
        } else {
            state.status =
                "No default agent configured. Create one from the standard CLI.".to_string();
            state.push_info(
                "No default agent configured. Create one from the standard CLI before using the TUI.",
            );
        }
    } else {
        state.enter_startup(options.agent, options.session);
        state.push_info("Daemon offline. Use /daemon to launch it.");
    }

    let _terminal = TerminalGuard::new()?;
    run_event_loop(controller, state).await
}
