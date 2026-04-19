use super::composer::ComposerMode;
use super::keymap::Action;
use super::slash_command::{SLASH_COMMAND_SPECS, SlashCommand, parse_slash_command};
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
    SessionPickerLoaded {
        sessions: Vec<ChatSessionSummary>,
        status: String,
    },
    SessionDeleted {
        session_id: String,
        deleted: bool,
        sessions: Vec<ChatSessionSummary>,
        status: String,
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
    DaemonStopped {
        status: String,
    },
    CommandPicked {
        text: String,
    },
    OpenDaemonPicker,
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
    SubmitMessage { message: String },
    ExecuteSlashCommand(SlashCommand),
    DeleteSession { session_id: String },
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
        ShellAction::SessionPickerLoaded { sessions, status } => {
            state.sessions = sessions;
            state.open_session_picker();
            state.status = status;
        }
        ShellAction::SessionDeleted {
            session_id,
            deleted,
            sessions,
            status,
        } => {
            state.apply_session_delete_result(&session_id, sessions);
            state.status = status.clone();
            if deleted {
                state.push_info(status);
            } else {
                state.push_error(status);
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
        ShellAction::DaemonStopped { status } => {
            let agent_override = state.default_agent_id.clone();
            let session_override = state.current_session_id().map(ToOwned::to_owned);
            state.enter_startup(agent_override, session_override);
            state.status = status.clone();
            state.push_info(status);
        }
        ShellAction::CommandPicked { text } => {
            state.clear_overlay();
            state.composer.replace(text);
            state.status = "Command selected".to_string();
        }
        ShellAction::OpenDaemonPicker => {
            state.composer.clear();
            state.open_daemon_picker();
            state.status = "Select daemon action".to_string();
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
            if state.overlay.is_some() {
                state.clear_overlay();
                if matches!(state.composer.mode(), ComposerMode::Command) {
                    state.composer.clear();
                }
                state.status = "Closed overlay".to_string();
            } else if !state.composer.is_blank() {
                let was_command_mode = matches!(state.composer.mode(), ComposerMode::Command);
                state.composer.clear();
                state.status = if was_command_mode {
                    "Returned to message mode".to_string()
                } else {
                    "Cleared input".to_string()
                };
            } else {
                state.status = "Input already empty. Press Ctrl-C to quit.".to_string();
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
            }
        }
        Action::NavDown => {
            if state.overlay.is_some() {
                state.move_overlay_selection(1);
            } else if state.composer.is_navigating_history() {
                state.composer.history_next();
            }
        }
        Action::MoveLeft => {
            state.composer.move_left();
        }
        Action::MoveRight => {
            state.composer.move_right();
        }
        Action::ScrollUp | Action::ScrollDown => {}
        Action::DeleteSelected => {
            if matches!(
                state.overlay,
                Some(crate::state::OverlayState::SessionPicker { .. })
            ) {
                let selected = state.selected_session_summary().cloned();
                if let Some(session) = selected {
                    if state.is_session_delete_pending(&session.id) {
                        output.effects.push(ShellEffect::DeleteSession {
                            session_id: session.id,
                        });
                    } else {
                        state.mark_session_delete_pending(session.id.clone());
                        state.status = format!("Press d again to delete session {}", session.name);
                    }
                }
            } else if state.overlay.is_none()
                || matches!(
                    state.overlay,
                    Some(crate::state::OverlayState::CommandPicker { .. })
                )
            {
                state.composer.insert_char('d');
                state.sync_command_picker_to_draft(SLASH_COMMAND_SPECS);
            }
        }
        Action::InputChar(ch) => {
            if state.overlay.is_none()
                || matches!(
                    state.overlay,
                    Some(crate::state::OverlayState::CommandPicker { .. })
                )
            {
                state.composer.insert_char(ch);
                if ch == '/' && state.composer.draft().trim() == "/" {
                    state.open_command_picker();
                }
                state.sync_command_picker_to_draft(SLASH_COMMAND_SPECS);
            }
        }
        Action::Paste(text) => {
            if state.overlay.is_none()
                || matches!(
                    state.overlay,
                    Some(crate::state::OverlayState::CommandPicker { .. })
                )
            {
                for ch in text.chars() {
                    state.composer.insert_char(ch);
                }
                if state.composer.draft().trim_start().starts_with('/') {
                    if state.overlay.is_none() {
                        state.open_command_picker();
                    }
                    state.sync_command_picker_to_draft(SLASH_COMMAND_SPECS);
                }
            }
        }
        Action::InputBackspace => {
            if state.overlay.is_none()
                || matches!(
                    state.overlay,
                    Some(crate::state::OverlayState::CommandPicker { .. })
                )
            {
                state.composer.backspace();
                state.sync_command_picker_to_draft(SLASH_COMMAND_SPECS);
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
                if matches!(
                    state.overlay,
                    Some(crate::state::OverlayState::CommandPicker { .. })
                ) {
                    let input = state.composer.draft().trim().to_string();
                    if input != "/" && parse_slash_command(&input).is_ok() {
                        state.composer.clear();
                        state.clear_overlay();
                        output.actions.push(ShellAction::SubmitText { text: input });
                    } else {
                        output.effects.push(ShellEffect::ActivateOverlaySelection);
                    }
                } else {
                    output.effects.push(ShellEffect::ActivateOverlaySelection);
                }
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
            "Daemon is offline. Use /daemon to launch it.".to_string()
        } else {
            "No active session. Use /resume or configure a default agent.".to_string()
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
    use restflow_core::models::{ChatSession, ChatSessionSummary};

    fn session_summary(id: &str, name: &str) -> ChatSessionSummary {
        ChatSessionSummary {
            id: id.to_string(),
            name: name.to_string(),
            agent_id: "agent-1".to_string(),
            provider: "provider".to_string(),
            model: "model".to_string(),
            skill_id: None,
            message_count: 1,
            updated_at: 1,
            last_message_preview: Some("preview".to_string()),
            source_channel: None,
            source_conversation_id: None,
            archived_at: None,
        }
    }

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
    fn slash_opens_command_picker() {
        let mut state = AppState::empty();

        let output = reduce(&mut state, ShellAction::Ui(Action::InputChar('/')));

        assert!(output.actions.is_empty());
        assert!(output.effects.is_empty());
        assert!(matches!(
            state.overlay,
            Some(crate::state::OverlayState::CommandPicker { selected: 0 })
        ));
    }

    #[test]
    fn command_picker_selection_moves_with_navigation() {
        let mut state = AppState::empty();
        state.composer.insert_char('/');
        state.open_command_picker();

        let output = reduce(&mut state, ShellAction::Ui(Action::NavDown));

        assert!(!output.should_quit);
        assert!(matches!(
            state.overlay,
            Some(crate::state::OverlayState::CommandPicker { selected: 1 })
        ));
    }

    #[test]
    fn command_picker_tracks_typed_prefix() {
        let mut state = AppState::empty();

        for ch in "/daemon".chars() {
            reduce(&mut state, ShellAction::Ui(Action::InputChar(ch)));
        }

        assert_eq!(state.composer.draft(), "/daemon");
        assert!(matches!(
            state.overlay,
            Some(crate::state::OverlayState::CommandPicker { selected: 0 })
        ));
    }

    #[test]
    fn command_picker_moves_to_resume_when_typed() {
        let mut state = AppState::empty();

        for ch in "/resume".chars() {
            reduce(&mut state, ShellAction::Ui(Action::InputChar(ch)));
        }

        assert_eq!(state.composer.draft(), "/resume");
        assert!(matches!(
            state.overlay,
            Some(crate::state::OverlayState::CommandPicker { selected: 2 })
        ));
    }

    #[test]
    fn command_picker_submit_prefers_typed_alias_over_selected_item() {
        let mut state = AppState::empty();
        for ch in "/session".chars() {
            reduce(&mut state, ShellAction::Ui(Action::InputChar(ch)));
        }

        let output = reduce(&mut state, ShellAction::Ui(Action::Submit));

        assert!(state.overlay.is_none());
        assert!(state.composer.draft().is_empty());
        assert!(matches!(
            output.actions.as_slice(),
            [ShellAction::SubmitText { text }] if text == "/session"
        ));
        assert!(output.effects.is_empty());
    }

    #[test]
    fn command_picked_replaces_composer_draft() {
        let mut state = AppState::empty();
        state.composer.insert_char('/');
        state.open_command_picker();

        let output = reduce(
            &mut state,
            ShellAction::CommandPicked {
                text: "/task ".to_string(),
            },
        );

        assert!(!output.should_quit);
        assert!(state.overlay.is_none());
        assert_eq!(state.composer.draft(), "/task ");
        assert_eq!(state.status, "Command selected");
    }

    #[test]
    fn open_daemon_picker_clears_command_draft() {
        let mut state = AppState::empty();
        state.composer.insert_char('/');
        state.open_command_picker();

        let output = reduce(&mut state, ShellAction::OpenDaemonPicker);

        assert!(!output.should_quit);
        assert_eq!(state.composer.draft(), "");
        assert!(matches!(
            state.overlay,
            Some(crate::state::OverlayState::DaemonPicker { selected: 0 })
        ));
        assert_eq!(state.status, "Select daemon action");
    }

    #[test]
    fn delete_selected_session_requires_confirmation() {
        let mut state = AppState::empty();
        state.sessions = vec![session_summary("session-1", "First")];
        state.open_session_picker();

        let output = reduce(&mut state, ShellAction::Ui(Action::DeleteSelected));

        assert!(output.effects.is_empty());
        assert_eq!(state.status, "Press d again to delete session First");
    }

    #[test]
    fn delete_selected_session_second_press_creates_effect() {
        let mut state = AppState::empty();
        state.sessions = vec![session_summary("session-1", "First")];
        state.open_session_picker();
        reduce(&mut state, ShellAction::Ui(Action::DeleteSelected));

        let output = reduce(&mut state, ShellAction::Ui(Action::DeleteSelected));

        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::DeleteSession { session_id }] if session_id == "session-1"
        ));
    }

    #[test]
    fn delete_selected_in_plain_composer_inserts_d() {
        let mut state = AppState::empty();

        let output = reduce(&mut state, ShellAction::Ui(Action::DeleteSelected));

        assert!(output.effects.is_empty());
        assert_eq!(state.composer.draft(), "d");
    }

    #[test]
    fn session_deleted_updates_picker_sessions() {
        let mut state = AppState::empty();
        state.sessions = vec![
            session_summary("session-1", "First"),
            session_summary("session-2", "Second"),
        ];
        state.open_session_picker();

        let output = reduce(
            &mut state,
            ShellAction::SessionDeleted {
                session_id: "session-1".to_string(),
                deleted: true,
                sessions: vec![session_summary("session-2", "Second")],
                status: "Deleted session session-1".to_string(),
            },
        );

        assert!(!output.should_quit);
        assert_eq!(state.sessions.len(), 1);
        assert_eq!(state.sessions[0].id, "session-2");
        assert_eq!(state.status, "Deleted session session-1");
        assert!(matches!(
            state.overlay,
            Some(crate::state::OverlayState::SessionPicker { selected: 0 })
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
    fn submit_daemon_command_routes_to_daemon_picker() {
        let mut state = AppState::empty();
        let output = reduce(
            &mut state,
            ShellAction::SubmitText {
                text: "/daemon".to_string(),
            },
        );

        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::ExecuteSlashCommand(SlashCommand::Daemon)]
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
    fn esc_in_compose_mode_clears_draft_instead_of_quitting() {
        let mut state = AppState::empty();
        for ch in "hello".chars() {
            state.composer.insert_char(ch);
        }

        let output = reduce(&mut state, ShellAction::Ui(Action::CloseOverlay));

        assert!(!output.should_quit);
        assert!(state.composer.draft().is_empty());
        assert_eq!(state.status, "Cleared input");
    }

    #[test]
    fn esc_with_empty_composer_does_not_quit() {
        let mut state = AppState::empty();

        let output = reduce(&mut state, ShellAction::Ui(Action::CloseOverlay));

        assert!(!output.should_quit);
        assert_eq!(state.status, "Input already empty. Press Ctrl-C to quit.");
    }

    #[test]
    fn esc_closes_overlay_and_clears_command_draft() {
        let mut state = AppState::empty();
        state.composer.replace("/daemon");
        state.open_daemon_picker();

        let output = reduce(&mut state, ShellAction::Ui(Action::CloseOverlay));

        assert!(!output.should_quit);
        assert!(state.overlay.is_none());
        assert_eq!(state.composer.draft(), "");
        assert_eq!(state.status, "Closed overlay");
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

        for ch in "/daemon start".chars() {
            state.composer.insert_char(ch);
        }

        let output = reduce(&mut state, ShellAction::Ui(Action::Submit));

        assert!(matches!(
            output.actions.as_slice(),
            [ShellAction::SubmitText { text }] if text == "/daemon start"
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
        assert!(state.runtime_cells[0].cell.body.contains("/daemon"));
    }

    #[test]
    fn daemon_stopped_enters_startup_mode_and_records_notice() {
        let mut state = AppState::empty();
        let session = ChatSession::new("agent-1".to_string(), "model".to_string());
        let session_id = session.id.clone();
        state.set_default_agent(Some("agent-1".to_string()), Some("Agent".to_string()));
        state.set_current_session(session);

        let output = reduce(
            &mut state,
            ShellAction::DaemonStopped {
                status: "RestFlow daemon stopped".to_string(),
            },
        );

        assert!(!output.should_quit);
        assert!(state.is_startup_mode());
        assert_eq!(state.status, "RestFlow daemon stopped");
        assert_eq!(
            state
                .startup_state()
                .and_then(|startup| startup.agent_override.as_deref()),
            Some("agent-1")
        );
        assert_eq!(
            state
                .startup_state()
                .and_then(|startup| startup.session_override.as_deref()),
            Some(session_id.as_str())
        );
        assert_eq!(state.runtime_cells.len(), 1);
        assert!(state.runtime_cells[0].cell.body.contains("daemon stopped"));
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
