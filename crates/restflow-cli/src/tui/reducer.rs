use super::keymap::Action;
use super::slash_command::{SlashCommand, parse_slash_command};
use super::state::{AppState, OverlayState};
use super::transcript::ShellMessage;
use restflow_core::models::{ChatSession, ChatSessionSummary, ExecutionThread, RunSummary};
use restflow_core::daemon::{ChatSessionEvent, StreamFrame};
use restflow_core::runtime::TaskStreamEvent;
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
    SubmitText { text: String },
    RefreshTick,
    ReloadCurrentSession,
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
    RejectSelectedApproval,
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
            state.apply_session_event(event);
            output.effects.push(if refresh_current {
                ShellEffect::ReloadCurrentSession
            } else {
                ShellEffect::RefreshState
            });
        }
        ShellAction::TaskEvent(event) => {
            state.apply_task_event(event);
            output.effects.push(ShellEffect::RefreshState);
        }
        ShellAction::StateRefreshed {
            sessions,
            runs,
        } => {
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
        ShellAction::SubmitText { text } => reduce_submit_text(state, text, &mut output),
        ShellAction::RefreshTick => output.effects.push(ShellEffect::RefreshState),
        ShellAction::ReloadCurrentSession => output.effects.push(ShellEffect::ReloadCurrentSession),
        ShellAction::Error(message) => {
            state.status = message.clone();
            state.push_error(message);
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
            } else {
                output.should_quit = true;
            }
        }
        Action::OpenSessions => state.open_session_picker(),
        Action::OpenRuns => state.open_run_picker(),
        Action::OpenApprovals => state.open_approval_picker(),
        Action::OpenTeam => state.open_team_overlay(),
        Action::OpenHelp => state.open_help_overlay(),
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
            if matches!(state.overlay, Some(OverlayState::TeamView { .. })) {
                state.cycle_team_tab(false);
            } else {
                state.composer.move_left();
            }
        }
        Action::MoveRight => {
            if matches!(state.overlay, Some(OverlayState::TeamView { .. })) {
                state.cycle_team_tab(true);
            } else {
                state.composer.move_right();
            }
        }
        Action::ScrollUp => state.scroll_transcript(-10),
        Action::ScrollDown => state.scroll_transcript(10),
        Action::InputChar(ch) => {
            if state.overlay.is_none() {
                state.composer.insert_char(ch);
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
            if matches!(state.overlay, Some(OverlayState::ApprovalPicker { .. })) {
                output.effects.push(ShellEffect::RejectSelectedApproval);
            } else if state.overlay.is_none() {
                state.composer.insert_char('r');
            }
        }
        Action::OverlaySelect => {
            if state.overlay.is_some() {
                output.effects.push(ShellEffect::ActivateOverlaySelection);
            }
        }
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
            Ok(command) => output.effects.push(ShellEffect::ExecuteSlashCommand(command)),
            Err(error) => {
                state.status = error.to_string();
                state.push_error(error.to_string());
            }
        }
    } else {
        state.push_message(ShellMessage::UserMessage {
            content: text.clone(),
        });
        state.status = "Sending message...".to_string();
        output.effects.push(ShellEffect::SubmitMessage { message: text });
    }
}

#[cfg(test)]
mod tests {
    use super::{ShellAction, ShellEffect, reduce};
    use crate::tui::keymap::Action;
    use crate::tui::slash_command::SlashCommand;
    use crate::tui::state::AppState;

    #[test]
    fn submit_plain_message_creates_send_effect() {
        let mut state = AppState::empty();
        state.composer.insert_char('h');
        state.composer.insert_char('i');

        let output = reduce(&mut state, ShellAction::Ui(Action::Submit));

        assert!(state.transcript.is_empty());
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
        assert!(state.transcript.is_empty());
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
        let output = reduce(
            &mut state,
            ShellAction::SubmitText {
                text: "hi".to_string(),
            },
        );

        assert_eq!(state.transcript.len(), 1);
        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::SubmitMessage { message }] if message == "hi"
        ));
    }
}
