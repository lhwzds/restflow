use super::composer::ComposerMode;
use super::keymap::Action;
use super::slash_command::{SlashCommand, parse_slash_command};
use super::state::AppState;
use super::transcript::ShellMessage;
use restflow_core::daemon::{ChatSessionEvent, StreamFrame};
use restflow_core::models::{ChatSession, ChatSessionSummary, ExecutionThread, RunSummary};
use restflow_core::runtime::TaskStreamEvent;
use restflow_core::storage::agent::StoredAgent;
use restflow_traits::{TeamAssignment, TeamMessage, TeamState};

#[derive(Debug)]
pub enum ShellAction {
    Ui(Action),
    StreamFrame(StreamFrame),
    SessionEvent(ChatSessionEvent),
    TaskEvent(TaskStreamEvent),
    StateRefreshed {
        sessions: Vec<ChatSessionSummary>,
        runs: Vec<RunSummary>,
    },
    CurrentSessionReloaded {
        session: Option<Box<ChatSession>>,
        runs: Vec<RunSummary>,
    },
    SessionOpened {
        session: Box<ChatSession>,
        runs: Vec<RunSummary>,
        status: String,
    },
    RunOpened {
        session: Option<Box<ChatSession>>,
        run_id: String,
        thread: Box<ExecutionThread>,
        child_runs: Vec<RunSummary>,
        status: String,
    },
    TaskControlCompleted {
        task_id: String,
        status: String,
    },
    TeamSnapshotLoaded {
        team_state: Option<TeamState>,
        messages: Vec<TeamMessage>,
        assignments: Vec<TeamAssignment>,
        status: String,
        open_overlay: bool,
    },
    MessageAppended(ShellMessage),
    StatusUpdated(String),
    DaemonStarted {
        agent: Option<Box<StoredAgent>>,
        session: Option<Box<ChatSession>>,
        status: String,
    },
    SubmitText {
        text: String,
    },
    RefreshTick,
    Error(String),
}

#[derive(Debug, Clone)]
pub enum ShellEffect {
    ClearScreen,
    RefreshState,
    ReloadCurrentSession,
    ActivateOverlaySelection,
    SubmitMessage {
        message: String,
    },
    ExecuteSlashCommand(SlashCommand),
    ListSessionsInline,
    ListRunsInline,
    ListApprovalsInline,
    ShowTeamInline,
}

#[derive(Debug, Default)]
pub struct ReducerOutput {
    pub should_quit: bool,
    pub actions: Vec<ShellAction>,
    pub effects: Vec<ShellEffect>,
}

pub fn reduce(state: &mut AppState, action: ShellAction) -> ReducerOutput {
    let mut output = ReducerOutput::default();
    match action {
        ShellAction::Ui(action) => reduce_ui(state, action, &mut output),
        ShellAction::StreamFrame(frame) => state.apply_stream_frame(frame),
        ShellAction::SessionEvent(event) => {
            let refresh_current = state.current_session_id() == Some(session_id_of(&event));
            let is_message_added = matches!(event, ChatSessionEvent::MessageAdded { .. });
            state.apply_session_event(event);
            if !is_message_added {
                output.effects.push(if refresh_current {
                    ShellEffect::ReloadCurrentSession
                } else {
                    ShellEffect::RefreshState
                });
            }
        }
        ShellAction::TaskEvent(event) => {
            state.apply_task_event(event);
            output.effects.push(ShellEffect::RefreshState);
        }
        ShellAction::StateRefreshed { sessions, runs } => {
            state.sessions = sessions;
            if state.current_session_id().is_some() {
                state.set_session_runs(runs);
            } else {
                state.thread.runs.clear();
                state.thread.child_runs.clear();
                state.thread.execution_thread = None;
            }
        }
        ShellAction::CurrentSessionReloaded { session, runs } => {
            if let Some(session) = session {
                state.refresh_current_session(*session);
                state.set_session_runs(runs);
            } else {
                state.clear_current_session("The active session is no longer available.");
            }
        }
        ShellAction::SessionOpened {
            session,
            runs,
            status,
        } => {
            state.set_current_session(*session);
            state.set_session_runs(runs);
            state.clear_overlay();
            state.status = status;
        }
        ShellAction::RunOpened {
            session,
            run_id,
            thread,
            child_runs,
            status,
        } => {
            if let Some(session) = session {
                state.set_current_session(*session);
            }
            state.set_run_focus(run_id, *thread, child_runs);
            state.clear_overlay();
            state.status = status;
        }
        ShellAction::TaskControlCompleted { task_id, status } => {
            state.status = format!("Task {task_id} -> {status}");
        }
        ShellAction::TeamSnapshotLoaded {
            team_state,
            messages,
            assignments,
            status,
            open_overlay,
        } => state.apply_team_snapshot(team_state, messages, assignments, status, open_overlay),
        ShellAction::MessageAppended(message) => state.push_message(message),
        ShellAction::StatusUpdated(status) => state.status = status,
        ShellAction::DaemonStarted {
            agent,
            session,
            status,
        } => {
            state.exit_startup();
            if let Some(agent) = agent {
                state.set_default_agent(Some(agent.id.clone()), Some(agent.name.clone()));
            } else {
                state.set_default_agent(None, None);
            }
            if let Some(session) = session {
                state.set_current_session(*session);
            }
            state.status = status;
            if let Some(message) = state.take_pending_initial_message()
                && !message.trim().is_empty()
                && state.current_session_id().is_some()
            {
                output
                    .actions
                    .push(ShellAction::SubmitText { text: message });
            }
        }
        ShellAction::SubmitText { text } => reduce_submit_text(state, text, &mut output),
        ShellAction::RefreshTick => {
            if !state.is_startup_mode() {
                output.effects.push(ShellEffect::RefreshState);
            }
        }
        ShellAction::Error(message) => {
            if state.is_startup_mode() {
                state.set_startup_error(message);
            } else {
                state.status = message.clone();
                state.push_error(message);
            }
        }
    }
    output
}

fn session_id_of(event: &ChatSessionEvent) -> &str {
    match event {
        ChatSessionEvent::Created { session_id }
        | ChatSessionEvent::Updated { session_id }
        | ChatSessionEvent::MessageAdded { session_id, .. }
        | ChatSessionEvent::Deleted { session_id } => session_id,
    }
}

fn reduce_ui(state: &mut AppState, action: Action, output: &mut ReducerOutput) {
    match action {
        Action::Quit => output.should_quit = true,
        Action::CloseOverlay => {
            if matches!(state.composer.mode(), ComposerMode::Command)
                && !state.composer.is_blank()
            {
                state.composer.clear();
                state.status = "Returned to message mode".to_string();
            } else {
                output.should_quit = true;
            }
        }
        Action::OpenSessions => output.effects.push(ShellEffect::ListSessionsInline),
        Action::OpenRuns => output.effects.push(ShellEffect::ListRunsInline),
        Action::OpenApprovals => output.effects.push(ShellEffect::ListApprovalsInline),
        Action::OpenTeam => output.effects.push(ShellEffect::ShowTeamInline),
        Action::OpenHelp => output
            .effects
            .push(ShellEffect::ExecuteSlashCommand(SlashCommand::Help)),
        Action::Resize => output.effects.push(ShellEffect::ClearScreen),
        Action::Redraw => {
            state.status = "Screen redrawn".to_string();
            output.effects.push(ShellEffect::ClearScreen);
        }
        Action::NavUp => {
            if state.overlay.is_some() {
                state.move_overlay_selection(-1);
            } else if state.composer.is_blank() {
                state.composer.history_previous();
            } else {
                state.scroll_transcript(-1);
            }
        }
        Action::NavDown => {
            if state.overlay.is_some() {
                state.move_overlay_selection(1);
            } else if state.composer.is_navigating_history() {
                state.composer.history_next();
            } else {
                state.scroll_transcript(1);
            }
        }
        Action::MoveLeft => {
            state.composer.move_left();
        }
        Action::MoveRight => {
            state.composer.move_right();
        }
        Action::ScrollUp | Action::ScrollDown => {}
        Action::InputChar(ch) => {
            if state.overlay.is_none() {
                state.composer.insert_char(ch);
            }
        }
        Action::Paste(text) => {
            if state.overlay.is_none() {
                for ch in text.chars() {
                    state.composer.insert_char(ch);
                }
            }
        }
        Action::InputBackspace => {
            if state.overlay.is_none() {
                state.composer.backspace();
            }
        }
        Action::Newline => {
            if state.overlay.is_none() {
                state.composer.insert_newline();
            }
        }
        Action::RejectSelected => {
            state.composer.insert_char('r');
        }
        Action::OverlaySelect => {}
        Action::Submit => {
            if state.overlay.is_some() {
                output.effects.push(ShellEffect::ActivateOverlaySelection);
            } else {
                let input = state.composer.take_submission();
                if !input.trim().is_empty() {
                    state.composer.remember_submission(&input);
                    output.actions.push(ShellAction::SubmitText { text: input });
                }
            }
        }
        Action::Noop => {}
    }
}

fn reduce_submit_text(state: &mut AppState, text: String, output: &mut ReducerOutput) {
    if super::composer::ComposerState::is_command_text(&text) {
        match parse_slash_command(&text) {
            Ok(command) => output
                .effects
                .push(ShellEffect::ExecuteSlashCommand(command)),
            Err(error) => {
                state.status = error.to_string();
                state.push_error(error.to_string());
            }
        }
    } else if state.current_session_id().is_none() {
        let message = if state.is_startup_mode() {
            "Daemon is offline. Use /start to launch it.".to_string()
        } else {
            "No active session. Use /sessions or configure a default agent.".to_string()
        };
        state.status = message.clone();
        state.push_error(message);
    } else {
        state.push_local_user_message(text.clone());
        state.status = "Sending message...".to_string();
        output
            .effects
            .push(ShellEffect::SubmitMessage { message: text });
    }
}

#[cfg(test)]
mod tests {
    use super::{ShellAction, ShellEffect, reduce};
    use crate::keymap::Action;
    use crate::slash_command::SlashCommand;
    use crate::state::AppState;
    use restflow_core::daemon::ChatSessionEvent;
    use restflow_core::models::ChatSession;

    #[test]
    fn submit_plain_message_creates_send_effect() {
        let mut state = AppState::empty();
        state.composer.insert_char('h');
        state.composer.insert_char('i');

        let output = reduce(&mut state, ShellAction::Ui(Action::Submit));

        assert!(state.conversation_cells.is_empty());
        assert!(state.runtime_cells.is_empty());
        assert!(state.active_cell.is_none());
        assert!(matches!(
            output.actions.as_slice(),
            [ShellAction::SubmitText { text }] if text == "hi"
        ));
        assert!(output.effects.is_empty());
    }

    #[test]
    fn submit_slash_command_creates_command_effect() {
        let mut state = AppState::empty();
        for ch in "/help".chars() {
            state.composer.insert_char(ch);
        }

        let output = reduce(&mut state, ShellAction::Ui(Action::Submit));

        assert!(matches!(
            output.actions.as_slice(),
            [ShellAction::SubmitText { text }] if text == "/help"
        ));
    }

    #[test]
    fn invalid_slash_command_pushes_error() {
        let mut state = AppState::empty();
        for ch in "/run nope".chars() {
            state.composer.insert_char(ch);
        }

        let output = reduce(&mut state, ShellAction::Ui(Action::Submit));

        assert!(matches!(
            output.actions.as_slice(),
            [ShellAction::SubmitText { text }] if text == "/run nope"
        ));
        assert!(output.effects.is_empty());
        assert!(state.conversation_cells.is_empty());
        assert!(state.runtime_cells.is_empty());
        assert!(state.active_cell.is_none());
    }

    #[test]
    fn submit_text_routes_slash_command_through_parser() {
        let mut state = AppState::empty();
        let output = reduce(
            &mut state,
            ShellAction::SubmitText {
                text: "/help".to_string(),
            },
        );

        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::ExecuteSlashCommand(SlashCommand::Help)]
        ));
    }

    #[test]
    fn submit_text_creates_send_effect_for_plain_message() {
        let mut state = AppState::empty();
        let session = ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session);
        let output = reduce(
            &mut state,
            ShellAction::SubmitText {
                text: "hi".to_string(),
            },
        );

        assert!(state.conversation_cells.is_empty());
        assert_eq!(state.pending_user_cells.len(), 1);
        assert_eq!(state.pending_user_cells[0].cell.body, "hi");
        assert!(state.runtime_cells.is_empty());
        assert!(state.active_cell.is_none());
        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::SubmitMessage { message }] if message == "hi"
        ));
    }

    #[test]
    fn esc_in_command_mode_clears_draft_instead_of_quitting() {
        let mut state = AppState::empty();
        for ch in "/help".chars() {
            state.composer.insert_char(ch);
        }

        let output = reduce(&mut state, ShellAction::Ui(Action::CloseOverlay));

        assert!(!output.should_quit);
        assert!(state.composer.draft().is_empty());
        assert_eq!(state.status, "Returned to message mode");
    }

    #[test]
    fn message_added_event_does_not_reload_current_session() {
        let mut state = AppState::empty();
        let session = ChatSession::new("agent-1".to_string(), "model".to_string());
        let session_id = session.id.clone();
        state.set_current_session(session);

        let output = reduce(
            &mut state,
            ShellAction::SessionEvent(ChatSessionEvent::MessageAdded {
                session_id,
                source: "ipc".to_string(),
            }),
        );

        assert!(output.effects.is_empty());
    }

    #[test]
    fn startup_submit_triggers_start_daemon_effect() {
        let mut state = AppState::empty();
        state.enter_startup(Some("agent-1".to_string()), Some("session-1".to_string()));

        for ch in "/start".chars() {
            state.composer.insert_char(ch);
        }

        let output = reduce(&mut state, ShellAction::Ui(Action::Submit));

        assert!(matches!(
            output.actions.as_slice(),
            [ShellAction::SubmitText { text }] if text == "/start"
        ));
    }

    #[test]
    fn resize_clears_screen_without_changing_status() {
        let mut state = AppState::empty();
        state.status = "Connected to daemon".to_string();

        let output = reduce(&mut state, ShellAction::Ui(Action::Resize));

        assert_eq!(state.status, "Connected to daemon");
        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::ClearScreen]
        ));
    }

    #[test]
    fn plain_text_when_daemon_offline_pushes_start_hint() {
        let mut state = AppState::empty();
        state.enter_startup(None, None);

        let output = reduce(
            &mut state,
            ShellAction::SubmitText {
                text: "hello".to_string(),
            },
        );

        assert!(output.effects.is_empty());
        assert_eq!(state.runtime_cells.len(), 1);
        assert!(state.runtime_cells[0].body.contains("/start"));
    }

    #[test]
    fn paste_inserts_text_without_submitting() {
        let mut state = AppState::empty();

        let output = reduce(
            &mut state,
            ShellAction::Ui(Action::Paste("hello\nworld".to_string())),
        );

        assert_eq!(state.composer.draft(), "hello\nworld");
        assert!(output.actions.is_empty());
        assert!(output.effects.is_empty());
    }
}
