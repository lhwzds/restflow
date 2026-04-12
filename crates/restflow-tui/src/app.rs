use anyhow::Result;
use crossterm::cursor::{Hide, Show};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{self, Stdout};

use crate::controller::ShellController;
use crate::daemon_client::TuiDaemonClient;
use crate::event_loop::run_event_loop;
use crate::state::AppState;

use super::TuiLaunchOptions;

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, Hide)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen, Show);
        let _ = self.terminal.show_cursor();
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
                state.status = "Connected to daemon".to_string();
            }
        } else {
            state.status =
                "No default agent configured. Create one from the standard CLI.".to_string();
            state.push_info(
                "No default agent configured. Create one from the standard CLI before using the TUI.",
            );
        }
    } else {
        state.enter_startup(options.agent, options.session);
    }

    let mut terminal = TerminalGuard::new()?;
    run_event_loop(&mut terminal.terminal, controller, state).await
}
