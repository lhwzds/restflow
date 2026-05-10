use super::composer::ComposerMode;
use super::keymap::Action;
use super::slash_command::{SLASH_COMMAND_SPECS, SlashCommand, parse_slash_command};
use super::state::{
    AppState, ModelPickerItem, PendingSessionState, ProviderPickerItem, SkillManagerSelection,
    SkillPickerItem,
};
use runtime::StoredAgent;
use types::{ChatSession, ChatSessionSummary, ExecutionThread, ModelMetadataDTO, RunSummary};
use types::{ChatSessionEvent, StreamFrame};

const MESSAGE_SCROLL_PAGE_ROWS: usize = 8;
const MESSAGE_SCROLL_WHEEL_ROWS: usize = 1;

#[derive(Debug)]
pub enum ShellAction {
    Ui(Action),
    StreamFrame(StreamFrame),
    SessionEvent(ChatSessionEvent),
    StateRefreshed {
        sessions: Vec<ChatSessionSummary>,
        runs: Vec<RunSummary>,
        child_runs: Vec<RunSummary>,
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
        child_runs: Vec<RunSummary>,
    },
    SessionOpened {
        session: Box<ChatSession>,
        runs: Vec<RunSummary>,
        child_runs: Vec<RunSummary>,
        status: String,
    },
    SessionCreatedForSubmit {
        session: Box<ChatSession>,
        runs: Vec<RunSummary>,
        child_runs: Vec<RunSummary>,
        message: String,
    },
    RunOpened {
        session: Option<Box<ChatSession>>,
        run_id: String,
        thread: Box<ExecutionThread>,
        child_runs: Vec<RunSummary>,
        status: String,
    },
    RunPickerLoaded {
        runs: Vec<RunSummary>,
        child_runs: Vec<RunSummary>,
        status: String,
    },
    SkillPickerLoaded {
        skills: Vec<SkillPickerItem>,
        status: String,
    },
    SkillMentionPickerLoaded {
        skills: Vec<SkillPickerItem>,
        status: String,
    },
    SkillDetailLoaded {
        skill: Box<types::Skill>,
        status: String,
    },
    ProviderPickerLoaded {
        items: Vec<ProviderPickerItem>,
        available_models: Vec<ModelMetadataDTO>,
        sessions: Vec<ChatSessionSummary>,
        status: String,
    },
    ModelPickerLoaded {
        provider: String,
        items: Vec<ModelPickerItem>,
        status: String,
    },
    ModelSwitched {
        session: Box<ChatSession>,
        status: String,
    },
    PendingSessionModelSelected {
        provider: String,
        model: String,
        model_name: String,
        status: String,
    },
    StatusUpdated(String),
    DaemonStarted {
        agent: Option<Box<StoredAgent>>,
        session: Option<Box<ChatSession>>,
        pending_session: Option<PendingSessionState>,
        status: String,
    },
    DaemonStopped {
        status: String,
    },
    CommandPicked {
        text: String,
    },
    NewChatStarted {
        status: String,
    },
    OpenHelpOverlay,
    OpenDaemonPicker,
    SubmitText {
        text: String,
    },
    Quit,
    RefreshTick,
    Error(String),
}

#[derive(Debug, Clone)]
pub enum ShellEffect {
    ClearScreen,
    RefreshState,
    ReloadCurrentSession,
    ActivateOverlaySelection,
    CreateSessionForSubmit {
        message: String,
    },
    SubmitMessage {
        message: String,
        stream_id: String,
    },
    SteerMessage {
        session_id: String,
        instruction: String,
    },
    CancelStream {
        stream_id: String,
    },
    ExecuteSlashCommand(SlashCommand),
    DeleteSession {
        session_id: String,
    },
    ListSkillsForMention,
    ListSessionsInline,
    ListRunsInline,
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
        ShellAction::StreamFrame(frame) => {
            let should_reload_session = matches!(
                frame,
                StreamFrame::ToolResult { .. } | StreamFrame::Done { .. } | StreamFrame::Error(_)
            );
            let could_reload_session = should_reload_active_session(state);
            let applied = state.apply_stream_frame(frame);
            if applied
                && should_reload_session
                && (could_reload_session || should_reload_active_session(state))
            {
                output.effects.push(ShellEffect::ReloadCurrentSession);
            }
        }
        ShellAction::SessionEvent(event) => {
            let refresh_current = state.active_refresh_session_id() == Some(session_id_of(&event))
                || (state.active_turn_has_tool_call()
                    && (state.is_streaming || state.active_turn.is_some()));
            let is_message_added = matches!(event, ChatSessionEvent::MessageAdded { .. });
            state.apply_session_event(event);
            if !is_message_added {
                output.effects.push(if refresh_current {
                    ShellEffect::ReloadCurrentSession
                } else {
                    ShellEffect::RefreshState
                });
            } else if refresh_current && (state.is_streaming || state.active_turn.is_some()) {
                output.effects.push(ShellEffect::ReloadCurrentSession);
            }
        }
        ShellAction::StateRefreshed {
            sessions,
            runs,
            child_runs,
        } => {
            state.sessions = sessions;
            if state.current_session_id().is_some() {
                state.set_session_runs_and_child_runs(runs, child_runs);
            } else {
                state.clear_thread_runs();
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
        ShellAction::CurrentSessionReloaded {
            session,
            runs,
            child_runs,
        } => {
            if let Some(session) = session {
                state.refresh_current_session(*session);
                state.set_session_runs_and_child_runs(runs, child_runs);
            } else if state.is_streaming || state.active_turn.is_some() {
                state.set_session_runs_and_child_runs(runs, child_runs);
            } else {
                state.clear_current_session("The active session is no longer available.");
            }
        }
        ShellAction::SessionOpened {
            session,
            runs,
            child_runs,
            status,
        } => {
            state.set_current_session(*session);
            state.set_session_runs_and_child_runs(runs, child_runs);
            state.clear_overlay();
            state.status = status;
        }
        ShellAction::SessionCreatedForSubmit {
            session,
            runs,
            child_runs,
            message,
        } => {
            state.set_current_session(*session);
            state.set_session_runs_and_child_runs(runs, child_runs);
            state.push_local_user_message(message.clone());
            state.start_assistant_typing();
            state.status = "Sending message...".to_string();
            output.effects.push(submit_message_effect(state, message));
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
            state.overlay = Some(crate::state::OverlayState::RunDetail);
            state.status = status;
        }
        ShellAction::RunPickerLoaded {
            runs,
            child_runs,
            status,
        } => {
            if state.current_session_id().is_some() {
                state.set_session_runs_and_child_runs(runs, child_runs);
            } else {
                state.clear_thread_runs();
            }
            state.open_run_picker();
            state.status = status;
        }
        ShellAction::SkillPickerLoaded { skills, status } => {
            state.skills = skills;
            state.skills_loaded = true;
            state.open_skill_manager();
            state.status = status;
        }
        ShellAction::SkillMentionPickerLoaded { skills, status } => {
            state.skills = skills;
            state.skills_loaded = true;
            if state.composer.current_skill_mention_query().is_some() {
                state.open_skill_mention_picker();
                state.sync_skill_mention_picker_to_draft();
            }
            state.status = status;
        }
        ShellAction::SkillDetailLoaded { skill, status } => {
            state.composer.clear();
            state.open_skill_detail(*skill);
            state.status = status;
        }
        ShellAction::ProviderPickerLoaded {
            items,
            available_models,
            sessions,
            status,
        } => {
            state.provider_items = items;
            state.available_models = available_models;
            state.sessions = sessions;
            state.open_provider_picker();
            state.status = status;
        }
        ShellAction::ModelPickerLoaded {
            provider,
            items,
            status,
        } => {
            state.model_items = items;
            state.open_model_picker(provider);
            state.status = status;
        }
        ShellAction::ModelSwitched { session, status } => {
            state.refresh_current_session(*session);
            state.clear_overlay();
            state.status = status;
        }
        ShellAction::PendingSessionModelSelected {
            provider,
            model,
            model_name,
            status,
        } => {
            if state.update_pending_session_model(provider, model, model_name) {
                state.clear_overlay();
                state.status = status;
            } else {
                state.status =
                    "No default agent is available. Start the daemon or send a message first."
                        .to_string();
            }
        }
        ShellAction::StatusUpdated(status) => state.status = status,
        ShellAction::DaemonStarted {
            agent,
            session,
            pending_session,
            status,
        } => {
            state.exit_startup();
            if let Some(agent) = agent.as_ref() {
                state.set_default_agent(Some(agent.id.clone()), Some(agent.name.clone()));
            } else {
                state.set_default_agent(None, None);
            }
            if let Some(session) = session {
                state.set_current_session(*session);
            } else if let Some(pending_session) = pending_session {
                state.set_pending_session(Some(pending_session));
            } else if let Some(agent) = agent.as_ref() {
                state.set_pending_session(Some(PendingSessionState::from_agent(agent)));
            }
            state.status = status;
            if let Some(message) = state.take_pending_initial_message()
                && !message.trim().is_empty()
                && state.default_agent_id.is_some()
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
        ShellAction::NewChatStarted { status } => {
            state.start_new_chat();
            state.status = status;
        }
        ShellAction::OpenHelpOverlay => {
            state.composer.clear();
            state.open_help_overlay();
            state.status = "Showing help".to_string();
        }
        ShellAction::OpenDaemonPicker => {
            state.composer.clear();
            state.open_daemon_picker();
            state.status = "Select daemon action".to_string();
        }
        ShellAction::SubmitText { text } => reduce_submit_text(state, text, &mut output),
        ShellAction::Quit => output.should_quit = true,
        ShellAction::RefreshTick => {
            if !state.is_startup_mode() {
                if should_reload_active_session(state) {
                    output.effects.push(ShellEffect::ReloadCurrentSession);
                } else {
                    output.effects.push(ShellEffect::RefreshState);
                }
            }
        }
        ShellAction::Error(message) => {
            if state.is_startup_mode() {
                state.set_startup_error(message);
            } else {
                state.cancel_active_response();
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

fn should_reload_active_session(state: &AppState) -> bool {
    (state.active_refresh_session_id().is_some() || state.active_turn_has_tool_call())
        && (state.is_streaming || state.active_turn.is_some())
}

fn reduce_ui(state: &mut AppState, action: Action, output: &mut ReducerOutput) {
    match action {
        Action::Quit => {
            if state.is_streaming || state.active_turn.is_some() {
                cancel_active_response(state, output);
            } else {
                output.should_quit = true;
            }
        }
        Action::CloseOverlay => {
            if state.is_streaming || state.active_turn.is_some() {
                cancel_active_response(state, output);
            } else if state.overlay.is_some() {
                state.clear_overlay();
                if matches!(state.composer.mode(), ComposerMode::Command) {
                    state.composer.clear();
                }
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
        Action::OpenHelp => output.actions.push(ShellAction::OpenHelpOverlay),
        Action::CycleInputMode => {
            if state.overlay.is_none() && !response_in_progress(state) {
                state.cycle_input_mode();
            }
        }
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
        Action::MoveStart => {
            state.composer.move_start();
        }
        Action::MoveEnd => {
            state.composer.move_end();
        }
        Action::ScrollUp => {
            if state.overlay.is_none() {
                state.scroll_message_up(MESSAGE_SCROLL_PAGE_ROWS);
            }
        }
        Action::ScrollDown => {
            if state.overlay.is_none() {
                state.scroll_message_down(MESSAGE_SCROLL_PAGE_ROWS);
            }
        }
        Action::WheelUp => {
            if state.overlay.is_none() {
                state.scroll_message_up(MESSAGE_SCROLL_WHEEL_ROWS);
            }
        }
        Action::WheelDown => {
            if state.overlay.is_none() {
                state.scroll_message_down(MESSAGE_SCROLL_WHEEL_ROWS);
            }
        }
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
            } else if matches!(
                state.overlay,
                Some(crate::state::OverlayState::SkillManager { .. })
            ) {
                match state.selected_skill_manager_item() {
                    Some(SkillManagerSelection::Skill(skill)) => {
                        state.status = format!(
                            "Skill {} is managed by skrun; edit or remove it from ~/.restflow/skills",
                            skill.id
                        );
                    }
                    None => {}
                }
            } else if state.overlay.is_none()
                || matches!(
                    state.overlay,
                    Some(
                        crate::state::OverlayState::CommandPicker { .. }
                            | crate::state::OverlayState::SkillMentionPicker { .. }
                    )
                )
            {
                state.composer.insert_char('d');
                sync_composer_overlay(state, output);
            }
        }
        Action::InputChar(ch) => {
            if state.overlay.is_none()
                || matches!(
                    state.overlay,
                    Some(
                        crate::state::OverlayState::CommandPicker { .. }
                            | crate::state::OverlayState::SkillMentionPicker { .. }
                    )
                )
            {
                state.composer.insert_char(ch);
                sync_composer_overlay(state, output);
            }
        }
        Action::Paste(text) => {
            if state.overlay.is_none()
                || matches!(
                    state.overlay,
                    Some(
                        crate::state::OverlayState::CommandPicker { .. }
                            | crate::state::OverlayState::SkillMentionPicker { .. }
                    )
                )
            {
                for ch in text.chars() {
                    state.composer.insert_char(ch);
                }
                sync_composer_overlay(state, output);
            }
        }
        Action::InputBackspace => {
            if state.overlay.is_none()
                || matches!(
                    state.overlay,
                    Some(
                        crate::state::OverlayState::CommandPicker { .. }
                            | crate::state::OverlayState::SkillMentionPicker { .. }
                    )
                )
            {
                state.composer.backspace();
                sync_composer_overlay(state, output);
            }
        }
        Action::Newline => {
            if state.overlay.is_none() {
                state.composer.insert_newline();
            }
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
                } else if matches!(
                    state.overlay,
                    Some(crate::state::OverlayState::SkillMentionPicker { .. })
                ) {
                    if let Some(skill) = state.selected_skill_mention_item()
                        && state.composer.replace_current_skill_mention(&skill.id)
                    {
                        state.status = format!("Inserted @{}", skill.id);
                    }
                    state.clear_overlay();
                } else {
                    if matches!(
                        state.overlay,
                        Some(crate::state::OverlayState::ModelPicker { .. })
                    ) {
                        state.status = "Switching model...".to_string();
                    } else if matches!(
                        state.overlay,
                        Some(crate::state::OverlayState::ProviderPicker { .. })
                    ) {
                        state.status = "Loading models...".to_string();
                    } else if matches!(
                        state.overlay,
                        Some(crate::state::OverlayState::SkillManager { .. })
                    ) {
                        state.status = "Loading skill...".to_string();
                    }
                    output.effects.push(ShellEffect::ActivateOverlaySelection);
                }
            } else if response_in_progress(state) {
                steer_active_response(state, output);
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

fn response_in_progress(state: &AppState) -> bool {
    state.is_streaming || state.active_turn.is_some()
}

fn steer_active_response(state: &mut AppState, output: &mut ReducerOutput) {
    if state.composer.draft().trim().is_empty() {
        return;
    }

    let Some(session_id) = state.current_session_id().map(ToOwned::to_owned) else {
        let message =
            "Response is still starting. Press Esc to cancel before sending another message.";
        state.status = message.to_string();
        state.push_info(message.to_string());
        return;
    };

    let instruction = state.composer.take_submission();
    state.composer.remember_submission(&instruction);
    state.queue_active_turn_update(instruction.clone());
    state.status = "Queued update for current response. Press Esc to interrupt.".to_string();
    output.effects.push(ShellEffect::SteerMessage {
        session_id,
        instruction,
    });
}

fn cancel_active_response(state: &mut AppState, output: &mut ReducerOutput) {
    let stream_id = state.current_stream_id.clone();
    state.cancel_active_response();
    state.push_info("Canceled current response.");
    if let Some(stream_id) = stream_id {
        state.status = "Canceling response...".to_string();
        output.effects.push(ShellEffect::CancelStream { stream_id });
    } else {
        state.status = "Canceled current response.".to_string();
    }
}

fn reduce_submit_text(state: &mut AppState, text: String, output: &mut ReducerOutput) {
    if super::composer::ComposerState::is_command_text(&text) {
        match parse_slash_command(&text) {
            Ok(command) => {
                state.status = slash_command_pending_status(&command).to_string();
                output
                    .effects
                    .push(ShellEffect::ExecuteSlashCommand(command));
            }
            Err(error) => {
                state.status = error.to_string();
                state.push_error(error.to_string());
            }
        }
    } else if state.current_session_id().is_none() {
        if state.is_startup_mode() {
            let message = "Daemon is offline. Use /daemon to launch it.".to_string();
            state.status = message.clone();
            state.push_error(message);
        } else if state.default_agent_id.is_some() {
            state.push_local_user_message(text.clone());
            state.start_assistant_typing();
            state.status = "Creating session...".to_string();
            output
                .effects
                .push(ShellEffect::CreateSessionForSubmit { message: text });
        } else {
            let message =
                "No active session. Use /resume or configure a default agent.".to_string();
            state.status = message.clone();
            state.push_error(message);
        }
    } else {
        state.push_local_user_message(text.clone());
        state.start_assistant_typing();
        state.status = "Sending message...".to_string();
        output.effects.push(submit_message_effect(state, text));
    }
}

fn sync_composer_overlay(state: &mut AppState, output: &mut ReducerOutput) {
    if state.composer.draft().trim_start().starts_with('/') {
        if state.overlay.is_none() {
            state.open_command_picker();
        }
        state.sync_command_picker_to_draft(SLASH_COMMAND_SPECS);
        return;
    }

    if state.composer.current_skill_mention_query().is_some() {
        if !matches!(
            state.overlay,
            Some(crate::state::OverlayState::SkillMentionPicker { .. })
        ) {
            state.open_skill_mention_picker();
        }
        if !state.skills_loaded {
            state.status = "Loading skills...".to_string();
            output.effects.push(ShellEffect::ListSkillsForMention);
        }
        state.sync_skill_mention_picker_to_draft();
        return;
    }

    if matches!(
        state.overlay,
        Some(
            crate::state::OverlayState::CommandPicker { .. }
                | crate::state::OverlayState::SkillMentionPicker { .. }
        )
    ) {
        state.clear_overlay();
    }
}

fn submit_message_effect(state: &mut AppState, message: String) -> ShellEffect {
    let stream_id = uuid::Uuid::new_v4().to_string();
    state.begin_stream(stream_id.clone());
    ShellEffect::SubmitMessage { message, stream_id }
}

fn slash_command_pending_status(command: &SlashCommand) -> &'static str {
    match command {
        SlashCommand::NewChat => "Starting new chat...",
        SlashCommand::ListSkills => "Loading skills...",
        SlashCommand::ListModels => "Loading providers...",
        SlashCommand::ListModelsForProvider { .. } => "Loading models...",
        SlashCommand::SwitchModel { .. } => "Switching model...",
        SlashCommand::ListSessions => "Loading sessions...",
        SlashCommand::Quit => "Exiting...",
        _ => "Running command...",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MESSAGE_SCROLL_PAGE_ROWS, MESSAGE_SCROLL_WHEEL_ROWS, ShellAction, ShellEffect, reduce,
    };
    use crate::keymap::Action;
    use crate::slash_command::SlashCommand;
    use crate::state::{AppState, InputMode, PendingSessionState, SkillPickerItem};
    use types::{ChatSession, ChatSessionSummary, Skill, SkillSource};
    use types::{ChatSessionEvent, StreamFrame};

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
        assert!(state.active_turn.is_none());
        assert!(matches!(
            output.actions.as_slice(),
            [ShellAction::SubmitText { text }] if text == "hi"
        ));
        assert!(output.effects.is_empty());
    }

    #[test]
    fn shift_tab_cycles_input_mode_without_overlay() {
        let mut state = AppState::empty();

        reduce(&mut state, ShellAction::Ui(Action::CycleInputMode));
        assert_eq!(state.input_mode, InputMode::Plan);
        reduce(&mut state, ShellAction::Ui(Action::CycleInputMode));
        assert_eq!(state.input_mode, InputMode::Chat);
    }

    #[test]
    fn submit_while_response_is_running_steers_current_session() {
        let mut state = AppState::empty();
        let session = ChatSession::new("agent-1".to_string(), "model".to_string());
        let session_id = session.id.clone();
        state.set_current_session(session);
        state.begin_stream("turn-1".to_string());
        state.push_local_user_message("first".to_string());
        for ch in "second".chars() {
            state.composer.insert_char(ch);
        }

        let output = reduce(&mut state, ShellAction::Ui(Action::Submit));

        assert!(state.composer.draft().is_empty());
        assert!(output.actions.is_empty());
        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::SteerMessage {
                session_id: effect_session_id,
                instruction
            }] if effect_session_id == &session_id && instruction == "second"
        ));
        assert_eq!(
            state.status,
            "Queued update for current response. Press Esc to interrupt."
        );
        let active_turn = state.active_turn.as_ref().expect("active turn");
        assert_eq!(active_turn.cells[0].body, "first");
        assert_eq!(active_turn.queued_updates, vec!["second"]);
    }

    #[test]
    fn submit_while_response_is_starting_keeps_draft_and_does_not_send() {
        let mut state = AppState::empty();
        state.begin_stream("turn-1".to_string());
        state.push_local_user_message("first".to_string());
        for ch in "second".chars() {
            state.composer.insert_char(ch);
        }

        let output = reduce(&mut state, ShellAction::Ui(Action::Submit));

        assert_eq!(state.composer.draft(), "second");
        assert!(output.actions.is_empty());
        assert!(output.effects.is_empty());
        assert_eq!(
            state.status,
            "Response is still starting. Press Esc to cancel before sending another message."
        );
        let active_turn = state.active_turn.as_ref().expect("active turn");
        assert_eq!(active_turn.cells.len(), 1);
        assert_eq!(active_turn.cells[0].body, "first");
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
    fn model_slash_command_sets_loading_status_before_effect() {
        let mut state = AppState::empty();

        let output = reduce(
            &mut state,
            ShellAction::SubmitText {
                text: "/model".to_string(),
            },
        );

        assert_eq!(state.status, "Loading providers...");
        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::ExecuteSlashCommand(SlashCommand::ListModels)]
        ));
    }

    #[test]
    fn skill_slash_command_sets_loading_status_before_effect() {
        let mut state = AppState::empty();

        let output = reduce(
            &mut state,
            ShellAction::SubmitText {
                text: "/skill".to_string(),
            },
        );

        assert_eq!(state.status, "Loading skills...");
        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::ExecuteSlashCommand(SlashCommand::ListSkills)]
        ));
    }

    #[test]
    fn skill_picker_loaded_opens_manager_overlay_without_history() {
        let mut state = AppState::empty();

        let output = reduce(
            &mut state,
            ShellAction::SkillPickerLoaded {
                skills: vec![SkillPickerItem {
                    id: "team".to_string(),
                    name: "Team".to_string(),
                    description: Some("Coordinate subagents".to_string()),
                    source: SkillSource::System,
                    read_only: true,
                }],
                status: "View skills".to_string(),
            },
        );

        assert!(output.actions.is_empty());
        assert!(output.effects.is_empty());
        assert!(matches!(
            state.overlay,
            Some(crate::state::OverlayState::SkillManager { selected: 0 })
        ));
        assert!(state.conversation_cells.is_empty());
        assert!(state.runtime_cells.is_empty());
    }

    #[test]
    fn run_picker_loaded_opens_transient_overlay_without_history() {
        let mut state = AppState::empty();

        let output = reduce(
            &mut state,
            ShellAction::RunPickerLoaded {
                runs: Vec::new(),
                child_runs: Vec::new(),
                status: "Work picker opened.".to_string(),
            },
        );

        assert!(output.actions.is_empty());
        assert!(output.effects.is_empty());
        assert!(matches!(
            state.overlay,
            Some(crate::state::OverlayState::RunPicker { selected: 0 })
        ));
        assert!(state.conversation_cells.is_empty());
        assert!(state.runtime_cells.is_empty());
    }

    #[test]
    fn skill_detail_loaded_opens_transient_overlay_without_history() {
        let mut state = AppState::empty();
        let mut skill = Skill::new(
            "team".to_string(),
            "Team".to_string(),
            Some("Coordinate subagents".to_string()),
            None,
            "# Team".to_string(),
        );
        skill.source = SkillSource::System;
        skill.read_only = true;

        let output = reduce(
            &mut state,
            ShellAction::SkillDetailLoaded {
                skill: Box::new(skill),
                status: "Showing skill team".to_string(),
            },
        );

        assert!(output.actions.is_empty());
        assert!(output.effects.is_empty());
        assert!(matches!(
            state.overlay,
            Some(crate::state::OverlayState::SkillDetail)
        ));
        assert_eq!(
            state.selected_skill.as_ref().map(|skill| skill.id.as_str()),
            Some("team")
        );
        assert!(state.conversation_cells.is_empty());
        assert!(state.runtime_cells.is_empty());
    }

    #[test]
    fn submitting_skill_manager_activates_selected_skill() {
        let mut state = AppState::empty();
        state.skills = vec![SkillPickerItem {
            id: "team".to_string(),
            name: "Team".to_string(),
            description: Some("Coordinate subagents".to_string()),
            source: SkillSource::System,
            read_only: true,
        }];
        state.open_skill_manager();
        state.move_overlay_selection(1);

        let output = reduce(&mut state, ShellAction::Ui(Action::Submit));

        assert_eq!(state.status, "Loading skill...");
        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::ActivateOverlaySelection]
        ));
    }

    #[test]
    fn at_opens_skill_mention_picker_and_loads_skills() {
        let mut state = AppState::empty();

        let output = reduce(&mut state, ShellAction::Ui(Action::InputChar('@')));

        assert!(matches!(
            state.overlay,
            Some(crate::state::OverlayState::SkillMentionPicker { selected: 0 })
        ));
        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::ListSkillsForMention]
        ));
    }

    #[test]
    fn submitting_skill_mention_inserts_selected_skill_id() {
        let mut state = AppState::empty();
        state.skills = vec![SkillPickerItem {
            id: "team".to_string(),
            name: "Team".to_string(),
            description: Some("Coordinate subagents".to_string()),
            source: SkillSource::System,
            read_only: true,
        }];
        state.skills_loaded = true;
        state.composer.replace("use @tea");
        state.open_skill_mention_picker();

        let output = reduce(&mut state, ShellAction::Ui(Action::Submit));

        assert!(output.actions.is_empty());
        assert!(output.effects.is_empty());
        assert_eq!(state.composer.draft(), "use @team ");
        assert!(state.overlay.is_none());
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
    fn page_scroll_updates_history_offset_without_overlay() {
        let mut state = AppState::empty();

        reduce(&mut state, ShellAction::Ui(Action::ScrollUp));
        assert_eq!(state.message_scroll_from_bottom, MESSAGE_SCROLL_PAGE_ROWS);

        reduce(&mut state, ShellAction::Ui(Action::ScrollDown));
        assert_eq!(state.message_scroll_from_bottom, 0);
    }

    #[test]
    fn wheel_scroll_uses_fine_grained_offset() {
        let mut state = AppState::empty();

        reduce(&mut state, ShellAction::Ui(Action::WheelUp));
        assert_eq!(state.message_scroll_from_bottom, MESSAGE_SCROLL_WHEEL_ROWS);

        reduce(&mut state, ShellAction::Ui(Action::WheelDown));
        assert_eq!(state.message_scroll_from_bottom, 0);
    }

    #[test]
    fn page_scroll_is_ignored_while_overlay_is_open() {
        let mut state = AppState::empty();
        state.open_command_picker();

        reduce(&mut state, ShellAction::Ui(Action::ScrollUp));

        assert_eq!(state.message_scroll_from_bottom, 0);
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
            Some(crate::state::OverlayState::CommandPicker { selected: 4 })
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
    fn submitting_model_picker_shows_switching_status_before_effect() {
        let mut state = AppState::empty();
        state.open_model_picker("codex");

        let output = reduce(&mut state, ShellAction::Ui(Action::Submit));

        assert!(!output.should_quit);
        assert!(matches!(
            state.overlay,
            Some(crate::state::OverlayState::ModelPicker { .. })
        ));
        assert_eq!(state.status, "Switching model...");
        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::ActivateOverlaySelection]
        ));
    }

    #[test]
    fn submitting_provider_picker_shows_loading_status_before_effect() {
        let mut state = AppState::empty();
        state.open_provider_picker();

        let output = reduce(&mut state, ShellAction::Ui(Action::Submit));

        assert!(!output.should_quit);
        assert!(matches!(
            state.overlay,
            Some(crate::state::OverlayState::ProviderPicker { .. })
        ));
        assert_eq!(state.status, "Loading models...");
        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::ActivateOverlaySelection]
        ));
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
        assert!(state.active_turn.is_none());
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
    fn submit_text_routes_quit_slash_command() {
        let mut state = AppState::empty();
        let output = reduce(
            &mut state,
            ShellAction::SubmitText {
                text: "/quit".to_string(),
            },
        );

        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::ExecuteSlashCommand(SlashCommand::Quit)]
        ));
        assert!(!output.should_quit);
        assert_eq!(state.status, "Exiting...");
    }

    #[test]
    fn quit_action_exits_shell() {
        let mut state = AppState::empty();
        let output = reduce(&mut state, ShellAction::Quit);

        assert!(output.should_quit);
    }

    #[test]
    fn ctrl_c_cancels_active_stream_before_quitting() {
        let mut state = AppState::empty();
        state.is_streaming = true;
        state.current_stream_id = Some("stream-1".to_string());
        state.apply_stream_frame(StreamFrame::Ack {
            content: String::new(),
        });
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-1".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({"command": "sleep 20"}),
        });

        let output = reduce(&mut state, ShellAction::Ui(Action::Quit));

        assert!(!output.should_quit);
        assert!(!state.is_streaming);
        assert!(state.current_stream_id.is_none());
        assert!(state.active_turn.is_none());
        assert!(state.runtime_cells.iter().any(|entry| {
            entry.cell.title == "Info" && entry.cell.body.contains("Canceled current response")
        }));
        assert_eq!(state.status, "Canceling response...");
        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::CancelStream { stream_id }] if stream_id == "stream-1"
        ));
    }

    #[test]
    fn help_overlay_does_not_append_runtime_info() {
        let mut state = AppState::empty();

        let output = reduce(&mut state, ShellAction::OpenHelpOverlay);

        assert!(output.effects.is_empty());
        assert!(state.conversation_cells.is_empty());
        assert!(state.runtime_cells.is_empty());
        assert!(matches!(
            state.overlay,
            Some(crate::state::OverlayState::Help)
        ));
        assert_eq!(state.status, "Showing help");
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
        assert!(state.runtime_cells.is_empty());
        let active_turn = state.active_turn.as_ref().expect("active turn");
        assert_eq!(active_turn.cells[0].body, "hi");
        let active = active_turn.cells.last().expect("active assistant");
        assert!(active.is_active);
        assert!(
            active
                .subtitle
                .as_deref()
                .is_some_and(|text| text.contains("typing"))
        );
        assert!(state.is_streaming);
        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::SubmitMessage { message, stream_id }] if message == "hi" && !stream_id.is_empty()
        ));
        match output.effects.as_slice() {
            [ShellEffect::SubmitMessage { stream_id, .. }] => {
                assert_eq!(state.current_stream_id.as_deref(), Some(stream_id.as_str()));
            }
            _ => unreachable!("asserted submit effect above"),
        }
    }

    #[test]
    fn esc_after_submit_cancels_before_start_frame() {
        let mut state = AppState::empty();
        let session = ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session);
        let output = reduce(
            &mut state,
            ShellAction::SubmitText {
                text: "hi".to_string(),
            },
        );
        let stream_id = match output.effects.as_slice() {
            [ShellEffect::SubmitMessage { stream_id, .. }] => stream_id.clone(),
            _ => panic!("expected submit effect"),
        };

        let output = reduce(&mut state, ShellAction::Ui(Action::CloseOverlay));

        assert_eq!(state.status, "Canceling response...");
        assert!(!state.is_streaming);
        assert!(state.current_stream_id.is_none());
        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::CancelStream { stream_id: canceled }] if canceled == &stream_id
        ));
    }

    #[test]
    fn submit_text_without_session_creates_session_first() {
        let mut state = AppState::empty();
        state.set_default_agent(Some("agent-1".to_string()), Some("Agent".to_string()));

        let output = reduce(
            &mut state,
            ShellAction::SubmitText {
                text: "hi".to_string(),
            },
        );

        let active_turn = state.active_turn.as_ref().expect("active turn");
        assert_eq!(active_turn.cells[0].body, "hi");
        assert!(active_turn.cells.last().is_some_and(|cell| cell.is_active));
        assert_eq!(state.status, "Creating session...");
        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::CreateSessionForSubmit { message }] if message == "hi"
        ));
    }

    #[test]
    fn session_created_for_submit_sends_pending_message() {
        let mut state = AppState::empty();
        let session = ChatSession::new("agent-1".to_string(), "model".to_string());
        let session_id = session.id.clone();

        let output = reduce(
            &mut state,
            ShellAction::SessionCreatedForSubmit {
                session: Box::new(session),
                runs: Vec::new(),
                child_runs: Vec::new(),
                message: "hi".to_string(),
            },
        );

        assert_eq!(state.current_session_id(), Some(session_id.as_str()));
        let active_turn = state.active_turn.as_ref().expect("active turn");
        assert_eq!(active_turn.cells[0].body, "hi");
        assert!(active_turn.cells.last().is_some_and(|cell| cell.is_active));
        assert_eq!(state.status, "Sending message...");
        assert!(state.is_streaming);
        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::SubmitMessage { message, stream_id }] if message == "hi" && !stream_id.is_empty()
        ));
        match output.effects.as_slice() {
            [ShellEffect::SubmitMessage { stream_id, .. }] => {
                assert_eq!(state.current_stream_id.as_deref(), Some(stream_id.as_str()));
            }
            _ => unreachable!("asserted submit effect above"),
        }
    }

    #[test]
    fn model_selection_without_session_updates_pending_session() {
        let mut state = AppState::empty();
        state.set_default_agent(Some("agent-1".to_string()), Some("Agent".to_string()));
        state.open_model_picker("codex");

        let output = reduce(
            &mut state,
            ShellAction::PendingSessionModelSelected {
                provider: "codex".to_string(),
                model: "gpt-5.4".to_string(),
                model_name: "GPT-5.4".to_string(),
                status: "Model selected for new chat.".to_string(),
            },
        );

        assert!(output.effects.is_empty());
        let pending = state.pending_session.as_ref().expect("pending session");
        assert_eq!(pending.agent_id, "agent-1");
        assert_eq!(pending.provider, "codex");
        assert_eq!(pending.model, "gpt-5.4");
        assert_eq!(pending.model_name, "GPT-5.4");
        assert!(state.overlay.is_none());
        assert_eq!(state.status, "Model selected for new chat.");
    }

    #[test]
    fn model_selection_without_default_agent_does_not_claim_success() {
        let mut state = AppState::empty();
        state.open_model_picker("codex");

        let output = reduce(
            &mut state,
            ShellAction::PendingSessionModelSelected {
                provider: "codex".to_string(),
                model: "gpt-5.4".to_string(),
                model_name: "GPT-5.4".to_string(),
                status: "Model selected for new chat.".to_string(),
            },
        );

        assert!(output.effects.is_empty());
        assert!(state.pending_session.is_none());
        assert!(state.overlay.is_some());
        assert_eq!(
            state.status,
            "No default agent is available. Start the daemon or send a message first."
        );
    }

    #[test]
    fn session_created_for_submit_clears_pending_session() {
        let mut state = AppState::empty();
        state.set_pending_session(Some(PendingSessionState::new(
            "agent-1".to_string(),
            "Agent".to_string(),
            "gpt-5.4".to_string(),
        )));
        let session = ChatSession::new("agent-1".to_string(), "gpt-5.4".to_string());

        let _ = reduce(
            &mut state,
            ShellAction::SessionCreatedForSubmit {
                session: Box::new(session),
                runs: Vec::new(),
                child_runs: Vec::new(),
                message: "hi".to_string(),
            },
        );

        assert!(state.pending_session.is_none());
    }

    #[test]
    fn new_chat_started_clears_view_and_sets_pending_session() {
        let mut state = AppState::empty();
        state.set_default_agent(Some("agent-1".to_string()), Some("Agent".to_string()));
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5.4".to_string());
        session.add_message(types::ChatMessage::user("old"));
        state.set_current_session(session);
        state.push_local_user_message("pending".to_string());
        state.push_info("notice");
        state.open_command_picker();

        let output = reduce(
            &mut state,
            ShellAction::NewChatStarted {
                status: "Started new chat".to_string(),
            },
        );

        assert!(output.effects.is_empty());
        assert!(state.current_session_id().is_none());
        assert!(state.conversation_cells.is_empty());
        assert!(state.runtime_cells.is_empty());
        assert!(state.active_turn.is_none());
        assert!(state.overlay.is_none());
        assert!(state.composer.draft().is_empty());
        let pending = state.pending_session.as_ref().expect("pending session");
        assert_eq!(pending.agent_id, "agent-1");
        assert_eq!(pending.model, "gpt-5.4");
        assert_eq!(state.status, "Started new chat");
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
    fn esc_with_empty_composer_cancels_active_stream() {
        let mut state = AppState::empty();
        state.is_streaming = true;
        state.current_stream_id = Some("stream-1".to_string());
        state.apply_stream_frame(StreamFrame::Ack {
            content: "Partial".to_string(),
        });

        let output = reduce(&mut state, ShellAction::Ui(Action::CloseOverlay));

        assert!(!output.should_quit);
        assert!(!state.is_streaming);
        assert!(state.current_stream_id.is_none());
        assert!(state.active_turn.is_none());
        assert!(
            state
                .runtime_cells
                .iter()
                .any(|entry| entry.cell.body.contains("Partial"))
        );
        assert!(state.runtime_cells.iter().any(|entry| {
            entry.cell.title == "Info" && entry.cell.body.contains("Canceled current response")
        }));
        assert_eq!(state.status, "Canceling response...");
        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::CancelStream { stream_id }] if stream_id == "stream-1"
        ));
    }

    #[test]
    fn esc_clears_active_turn_even_without_stream_id() {
        let mut state = AppState::empty();
        state.push_local_user_message("run tool".to_string());
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-1".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({"cmd": "sleep 10"}),
        });

        let output = reduce(&mut state, ShellAction::Ui(Action::CloseOverlay));

        assert!(output.effects.is_empty());
        assert!(state.active_turn.is_none());
        assert!(state.runtime_cells.iter().any(|entry| {
            entry.cell.kind == crate::transcript::TranscriptCellKind::Tool
                && entry.cell.tool_call_id() == Some("call-1")
        }));
        assert_eq!(state.status, "Canceled current response.");
    }

    #[test]
    fn esc_with_draft_cancels_active_stream_before_clearing_composer() {
        let mut state = AppState::empty();
        state.is_streaming = true;
        state.current_stream_id = Some("stream-1".to_string());
        state.composer.replace("draft");
        state.apply_stream_frame(StreamFrame::Ack {
            content: "Partial".to_string(),
        });

        let output = reduce(&mut state, ShellAction::Ui(Action::CloseOverlay));

        assert_eq!(state.composer.draft(), "draft");
        assert_eq!(state.status, "Canceling response...");
        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::CancelStream { stream_id }] if stream_id == "stream-1"
        ));
    }

    #[test]
    fn esc_with_overlay_cancels_active_stream_before_closing_overlay() {
        let mut state = AppState::empty();
        state.is_streaming = true;
        state.current_stream_id = Some("stream-1".to_string());
        state.open_command_picker();
        state.apply_stream_frame(StreamFrame::Ack {
            content: "Partial".to_string(),
        });

        let output = reduce(&mut state, ShellAction::Ui(Action::CloseOverlay));

        assert!(state.overlay.is_some());
        assert_eq!(state.status, "Canceling response...");
        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::CancelStream { stream_id }] if stream_id == "stream-1"
        ));
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
        assert_eq!(state.status, "Connecting to daemon...");
    }

    #[test]
    fn message_added_event_does_not_reload_idle_current_session() {
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
    fn message_added_event_reloads_current_session_when_active_turn_is_visible() {
        let mut state = AppState::empty();
        let session = ChatSession::new("agent-1".to_string(), "model".to_string());
        let session_id = session.id.clone();
        state.set_current_session(session);
        state.push_local_user_message("run a team".to_string());
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-1".to_string(),
            name: "spawn_subagent_batch".to_string(),
            arguments: serde_json::json!({"specs":[{"task":"reply"}]}),
        });

        let output = reduce(
            &mut state,
            ShellAction::SessionEvent(ChatSessionEvent::MessageAdded {
                session_id,
                source: "ipc".to_string(),
            }),
        );

        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::ReloadCurrentSession]
        ));
    }

    #[test]
    fn message_added_event_reloads_when_visible_tool_turn_lost_session_anchor() {
        let mut state = AppState::empty();
        state.push_local_user_message("run a team".to_string());
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-1".to_string(),
            name: "spawn_subagent_batch".to_string(),
            arguments: serde_json::json!({"specs":[{"task":"reply"}]}),
        });

        let output = reduce(
            &mut state,
            ShellAction::SessionEvent(ChatSessionEvent::MessageAdded {
                session_id: "session-1".to_string(),
                source: "ipc".to_string(),
            }),
        );

        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::ReloadCurrentSession]
        ));
    }

    #[test]
    fn stream_tool_result_reloads_when_visible_tool_turn_lost_session_anchor() {
        let mut state = AppState::empty();
        state.push_local_user_message("run a team".to_string());
        reduce(
            &mut state,
            ShellAction::StreamFrame(StreamFrame::ToolCall {
                id: "call-1".to_string(),
                name: "spawn_subagent_batch".to_string(),
                arguments: serde_json::json!({"specs":[{"task":"reply"}]}),
            }),
        );

        let output = reduce(
            &mut state,
            ShellAction::StreamFrame(StreamFrame::ToolResult {
                id: "call-1".to_string(),
                result: "completed".to_string(),
                success: true,
            }),
        );

        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::ReloadCurrentSession]
        ));
    }

    #[test]
    fn stream_done_reloads_current_session_to_reconcile_pending_user() {
        let mut state = AppState::empty();
        let session = ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session);
        state.push_local_user_message("hi".to_string());
        state.start_assistant_typing();

        let output = reduce(
            &mut state,
            ShellAction::StreamFrame(StreamFrame::Done { total_tokens: None }),
        );

        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::ReloadCurrentSession]
        ));
    }

    #[test]
    fn refresh_tick_reloads_current_session_while_active_turn_waits_for_persistence() {
        let mut state = AppState::empty();
        let session = ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session);
        state.push_local_user_message("hi".to_string());
        state.apply_stream_frame(StreamFrame::Ack {
            content: "done".to_string(),
        });
        state.apply_stream_frame(StreamFrame::Done { total_tokens: None });

        assert!(!state.is_streaming);
        assert!(state.active_turn.is_some());

        let output = reduce(&mut state, ShellAction::RefreshTick);

        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::ReloadCurrentSession]
        ));
    }

    #[test]
    fn refresh_tick_uses_active_turn_session_when_thread_session_is_missing() {
        let mut state = AppState::empty();
        let session = ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session);
        state.push_local_user_message("hi".to_string());
        state.apply_stream_frame(StreamFrame::Ack {
            content: "done".to_string(),
        });
        state.apply_stream_frame(StreamFrame::Done { total_tokens: None });
        state.thread.clear_session();

        let output = reduce(&mut state, ShellAction::RefreshTick);

        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::ReloadCurrentSession]
        ));
    }

    #[test]
    fn refresh_tick_reloads_visible_tool_turn_without_session_anchor() {
        let mut state = AppState::empty();
        state.push_local_user_message("run a team".to_string());
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-1".to_string(),
            name: "spawn_subagent_batch".to_string(),
            arguments: serde_json::json!({"specs":[{"task":"reply"}]}),
        });

        let output = reduce(&mut state, ShellAction::RefreshTick);

        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::ReloadCurrentSession]
        ));
    }

    #[test]
    fn active_reload_miss_does_not_clear_visible_turn() {
        let mut state = AppState::empty();
        let session = ChatSession::new("agent-1".to_string(), "model".to_string());
        let session_id = session.id.clone();
        state.set_current_session(session);
        state.push_local_user_message("edit a file".to_string());
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-1".to_string(),
            name: "edit".to_string(),
            arguments: serde_json::json!({"file_path":"check.txt"}),
        });

        let output = reduce(
            &mut state,
            ShellAction::CurrentSessionReloaded {
                session: None,
                runs: Vec::new(),
                child_runs: Vec::new(),
            },
        );

        assert!(output.effects.is_empty());
        assert_eq!(state.current_session_id(), Some(session_id.as_str()));
        assert!(state.active_turn.is_some());
        assert!(!state.conversation_cells.iter().any(|cell| {
            cell.body
                .contains("The active session is no longer available")
        }));
    }

    #[test]
    fn stream_error_reloads_current_session_to_reconcile_pending_user() {
        let mut state = AppState::empty();
        let session = ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session);
        state.push_local_user_message("hi".to_string());
        state.start_assistant_typing();

        let output = reduce(
            &mut state,
            ShellAction::StreamFrame(StreamFrame::error(500, "failed")),
        );

        assert!(matches!(
            output.effects.as_slice(),
            [ShellEffect::ReloadCurrentSession]
        ));
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
    fn error_clears_active_typing_cell() {
        let mut state = AppState::empty();
        state.start_assistant_typing();
        assert!(state.active_turn.is_some());

        let output = reduce(
            &mut state,
            ShellAction::Error("Failed to connect to daemon. Is it running?".to_string()),
        );

        assert!(output.effects.is_empty());
        assert!(state.active_turn.is_none());
        assert!(!state.is_streaming);
        assert_eq!(state.status, "Failed to connect to daemon. Is it running?");
        assert!(
            state
                .runtime_cells
                .iter()
                .any(|entry| entry.cell.body.contains("Failed to connect to daemon"))
        );
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
