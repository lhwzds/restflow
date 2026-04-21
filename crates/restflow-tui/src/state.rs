use std::collections::HashSet;

use chrono::Utc;
use restflow_core::daemon::{ChatSessionEvent, StreamFrame};
use restflow_core::models::{
    ChatSession, ChatSessionSummary, ExecutionThread, ModelId, ModelMetadataDTO, RunSummary,
};
use restflow_core::runtime::TaskStreamEvent;
use restflow_core::storage::agent::StoredAgent;
use restflow_traits::{
    PendingTeamApproval, TeamAssignment, TeamMessage, TeamMessageKind, TeamState,
};

use super::composer::ComposerState;
use super::transcript::{
    ShellMessage, TranscriptCell, TranscriptCellKind, cell_from_message,
    message_from_session_event, message_from_stream_frame, message_from_task_event,
    message_from_team_message, messages_from_session, transcript_cells,
};

#[derive(Debug, Clone)]
pub enum RunPickerItem {
    Run {
        run_id: String,
        title: String,
        status: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskPickerItem {
    pub task_id: String,
    pub name: String,
    pub status: String,
    pub next_run_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeamPickerItem {
    Current {
        team_run_id: String,
        status: String,
        members: usize,
    },
    Saved {
        name: String,
        member_groups: usize,
        total_instances: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelPickerCategory {
    Recent,
    Frequent,
    Available,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPickerItem {
    pub provider: String,
    pub label: String,
    pub category: ModelPickerCategory,
    pub usage_count: usize,
    pub last_used_at: Option<i64>,
    pub is_current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelPickerItem {
    pub provider: String,
    pub model: String,
    pub name: String,
    pub category: ModelPickerCategory,
    pub usage_count: usize,
    pub last_used_at: Option<i64>,
    pub is_current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSessionState {
    pub agent_id: String,
    pub agent_name: String,
    pub provider: String,
    pub model: String,
    pub model_name: String,
}

impl PendingSessionState {
    pub fn from_agent(agent: &StoredAgent) -> Self {
        let model = agent
            .agent
            .model
            .map(|model| model.as_serialized_str().to_string())
            .unwrap_or_else(|| ModelId::Gpt5.as_serialized_str().to_string());
        Self::new(agent.id.clone(), agent.name.clone(), model)
    }

    pub fn new(agent_id: String, agent_name: String, model: String) -> Self {
        let (provider, model) = ChatSession::resolve_model_identity(&model);
        let model_name = model_display_name(&model);
        Self {
            agent_id,
            agent_name,
            provider,
            model,
            model_name,
        }
    }

    pub fn update_model(&mut self, provider: String, model: String, model_name: String) {
        self.provider = provider;
        self.model = model;
        self.model_name = model_name;
    }

    pub fn model_label(&self) -> String {
        if self.provider.trim().is_empty() {
            self.model.clone()
        } else {
            format!("{} · {}", self.provider, self.model)
        }
    }
}

fn model_display_name(model: &str) -> String {
    ModelId::from_serialized_str(model)
        .map(|model_id| model_id.metadata().name.to_string())
        .unwrap_or_else(|| model.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingUserCell {
    pub base_cell_index: usize,
    pub cell: TranscriptCell,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnchoredRuntimeCell {
    pub base_cell_index: usize,
    pub cell: TranscriptCell,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ThreadFocus {
    #[default]
    Session,
    Run {
        run_id: String,
    },
}

#[derive(Debug, Clone, Default)]
pub struct SessionThreadState {
    pub session: Option<ChatSession>,
    pub focus: ThreadFocus,
    pub runs: Vec<RunSummary>,
    pub child_runs: Vec<RunSummary>,
    pub execution_thread: Option<ExecutionThread>,
}

impl SessionThreadState {
    pub fn session_id(&self) -> Option<&str> {
        self.session.as_ref().map(|session| session.id.as_str())
    }

    pub fn set_session(&mut self, session: ChatSession) {
        self.session = Some(session);
        self.focus = ThreadFocus::Session;
        self.runs.clear();
        self.child_runs.clear();
        self.execution_thread = None;
    }

    pub fn clear_session(&mut self) {
        self.session = None;
        self.focus = ThreadFocus::Session;
        self.runs.clear();
        self.child_runs.clear();
        self.execution_thread = None;
    }

    pub fn set_session_runs(&mut self, runs: Vec<RunSummary>) {
        self.runs = runs;
    }

    pub fn set_run_focus(
        &mut self,
        run_id: String,
        thread: ExecutionThread,
        child_runs: Vec<RunSummary>,
    ) {
        self.focus = ThreadFocus::Run { run_id };
        self.execution_thread = Some(thread);
        self.child_runs = child_runs;
    }

    pub fn task_stream_id(&self) -> Option<&str> {
        self.execution_thread
            .as_ref()
            .and_then(|thread| thread.focus.task_id.as_deref())
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamOverlayTab {
    Members,
    Messages,
    Assignments,
    Approvals,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum OverlayState {
    CommandPicker { selected: usize },
    DaemonPicker { selected: usize },
    SessionPicker { selected: usize },
    TaskPicker { selected: usize },
    TaskActionPicker { task_id: String, selected: usize },
    TeamPicker { selected: usize },
    ProviderPicker { selected: usize },
    ModelPicker { provider: String, selected: usize },
    RunPicker { selected: usize },
    ApprovalPicker { selected: usize },
    TeamView { tab: TeamOverlayTab, scroll: u16 },
    Help,
}

#[derive(Debug, Clone, Default)]
pub struct StartupState {
    pub starting_daemon: bool,
    pub error: Option<String>,
    pub agent_override: Option<String>,
    pub session_override: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub default_agent_name: Option<String>,
    pub default_agent_id: Option<String>,
    pub thread: SessionThreadState,
    pub current_team_state: Option<TeamState>,
    pub current_team_messages: Vec<TeamMessage>,
    pub current_team_assignments: Vec<TeamAssignment>,
    pub current_team_approvals: Vec<PendingTeamApproval>,
    pub sessions: Vec<ChatSessionSummary>,
    pub tasks: Vec<TaskPickerItem>,
    pub team_items: Vec<TeamPickerItem>,
    pub provider_items: Vec<ProviderPickerItem>,
    pub model_items: Vec<ModelPickerItem>,
    pub available_models: Vec<ModelMetadataDTO>,
    pub pending_session: Option<PendingSessionState>,
    // Conversation cells are rebuilt from persisted session messages and should stay stable.
    pub conversation_cells: Vec<TranscriptCell>,
    // Runtime cells are ephemeral UI feedback for the current turn only.
    pub runtime_cells: Vec<AnchoredRuntimeCell>,
    // Active cell is the single in-flight assistant response while streaming.
    pub active_cell: Option<TranscriptCell>,
    pub active_turn_cells: Vec<TranscriptCell>,
    active_typing_started_at_ms: Option<i64>,
    active_assistant_stream_body: String,
    active_tool_call_ids: HashSet<String>,
    active_tool_result_ids: HashSet<String>,
    pub pending_user_cells: Vec<PendingUserCell>,
    pub overlay: Option<OverlayState>,
    pub composer: ComposerState,
    pub message_scroll_from_bottom: usize,
    pub status: String,
    pub is_streaming: bool,
    pub startup: Option<StartupState>,
    pending_session_delete_id: Option<String>,
    pending_initial_message: Option<String>,
    seen_team_message_ids: HashSet<String>,
}

impl AppState {
    pub fn empty() -> Self {
        Self {
            default_agent_name: None,
            default_agent_id: None,
            thread: SessionThreadState::default(),
            current_team_state: None,
            current_team_messages: Vec::new(),
            current_team_assignments: Vec::new(),
            current_team_approvals: Vec::new(),
            sessions: Vec::new(),
            tasks: Vec::new(),
            team_items: Vec::new(),
            provider_items: Vec::new(),
            model_items: Vec::new(),
            available_models: Vec::new(),
            pending_session: None,
            conversation_cells: Vec::new(),
            runtime_cells: Vec::new(),
            active_cell: None,
            active_turn_cells: Vec::new(),
            active_typing_started_at_ms: None,
            active_assistant_stream_body: String::new(),
            active_tool_call_ids: HashSet::new(),
            active_tool_result_ids: HashSet::new(),
            pending_user_cells: Vec::new(),
            overlay: None,
            composer: ComposerState::default(),
            message_scroll_from_bottom: 0,
            status: "Connecting to daemon...".to_string(),
            is_streaming: false,
            startup: None,
            pending_session_delete_id: None,
            pending_initial_message: None,
            seen_team_message_ids: HashSet::new(),
        }
    }

    pub fn current_session(&self) -> Option<&ChatSession> {
        self.thread.session.as_ref()
    }

    pub fn current_session_id(&self) -> Option<&str> {
        self.thread.session_id()
    }

    pub fn focused_task_stream_id(&self) -> Option<&str> {
        self.thread.task_stream_id()
    }

    pub fn is_startup_mode(&self) -> bool {
        self.startup.is_some()
    }

    pub fn startup_state(&self) -> Option<&StartupState> {
        self.startup.as_ref()
    }

    pub fn set_default_agent(&mut self, id: Option<String>, name: Option<String>) {
        self.default_agent_id = id;
        self.default_agent_name = name;
    }

    pub fn set_pending_initial_message(&mut self, message: Option<String>) {
        self.pending_initial_message = message;
    }

    pub fn set_pending_session(&mut self, pending_session: Option<PendingSessionState>) {
        self.pending_session = pending_session;
    }

    pub fn set_pending_session_from_agent(&mut self, agent: &StoredAgent) {
        self.pending_session = Some(PendingSessionState::from_agent(agent));
    }

    pub fn update_pending_session_model(
        &mut self,
        provider: String,
        model: String,
        model_name: String,
    ) -> bool {
        if self.pending_session.is_none()
            && let Some(agent_id) = self.default_agent_id.clone()
        {
            let agent_name = self
                .default_agent_name
                .clone()
                .unwrap_or_else(|| "Agent".to_string());
            self.pending_session = Some(PendingSessionState::new(
                agent_id,
                agent_name,
                model.clone(),
            ));
        }
        let Some(pending_session) = self.pending_session.as_mut() else {
            return false;
        };
        pending_session.update_model(provider, model, model_name);
        true
    }

    pub fn current_model_identity(&self) -> Option<(&str, &str)> {
        if let Some(session) = self.current_session() {
            return Some((session.provider.as_str(), session.model.as_str()));
        }
        self.pending_session
            .as_ref()
            .map(|session| (session.provider.as_str(), session.model.as_str()))
    }

    pub fn take_pending_initial_message(&mut self) -> Option<String> {
        self.pending_initial_message.take()
    }

    pub fn enter_startup(
        &mut self,
        agent_override: Option<String>,
        session_override: Option<String>,
    ) {
        self.pending_session = None;
        self.startup = Some(StartupState {
            starting_daemon: false,
            error: None,
            agent_override,
            session_override,
        });
        self.status = "RestFlow daemon is not running".to_string();
    }

    pub fn set_startup_error(&mut self, message: String) {
        if let Some(startup) = self.startup.as_mut() {
            startup.starting_daemon = false;
            startup.error = Some(message.clone());
        }
        self.status = message;
    }

    pub fn exit_startup(&mut self) {
        self.startup = None;
    }

    pub fn set_current_session(&mut self, session: ChatSession) {
        let session_changed = self.current_session_id() != Some(session.id.as_str());
        self.thread.set_session(session.clone());
        self.pending_session = None;
        if session_changed {
            self.clear_team_context();
        }
        self.seen_team_message_ids.clear();
        self.runtime_cells.clear();
        self.clear_active_response();
        self.reset_message_scroll();
        self.pending_user_cells.clear();
        self.conversation_cells =
            transcript_cells(&messages_from_session(&session), self.assistant_name());
    }

    pub fn refresh_current_session(&mut self, session: ChatSession) {
        self.thread.session = Some(session.clone());
        self.replace_session_projection(messages_from_session(&session));
        self.reconcile_pending_user_cells();
    }

    pub fn clear_current_session(&mut self, notice: impl Into<String>) {
        self.thread.clear_session();
        self.clear_team_context();
        self.replace_session_projection(Vec::new());
        self.runtime_cells.clear();
        self.clear_active_response();
        self.reset_message_scroll();
        self.pending_user_cells.clear();
        self.push_info(notice);
    }

    pub fn start_new_chat(&mut self) {
        let pending_session = self
            .pending_session
            .clone()
            .or_else(|| {
                self.thread.session.as_ref().map(|session| {
                    PendingSessionState::new(
                        session.agent_id.clone(),
                        self.default_agent_name
                            .clone()
                            .unwrap_or_else(|| "Agent".to_string()),
                        session.model.clone(),
                    )
                })
            })
            .or_else(|| {
                self.default_agent_id.clone().map(|agent_id| {
                    PendingSessionState::new(
                        agent_id,
                        self.default_agent_name
                            .clone()
                            .unwrap_or_else(|| "Agent".to_string()),
                        ModelId::Gpt5.as_serialized_str().to_string(),
                    )
                })
            });

        self.thread.clear_session();
        self.clear_team_context();
        self.conversation_cells.clear();
        self.runtime_cells.clear();
        self.clear_active_response();
        self.reset_message_scroll();
        self.pending_user_cells.clear();
        self.pending_session = pending_session;
        self.clear_overlay();
        self.composer.clear();
        self.is_streaming = false;
    }

    pub fn set_session_runs(&mut self, runs: Vec<RunSummary>) {
        self.thread.set_session_runs(runs);
    }

    pub fn set_run_focus(
        &mut self,
        run_id: String,
        thread: ExecutionThread,
        child_runs: Vec<RunSummary>,
    ) {
        self.thread.set_run_focus(run_id, thread, child_runs);
    }

    pub fn clear_overlay(&mut self) {
        self.overlay = None;
        self.pending_session_delete_id = None;
    }

    fn clear_team_context(&mut self) {
        self.current_team_state = None;
        self.current_team_messages.clear();
        self.current_team_assignments.clear();
        self.current_team_approvals.clear();
        self.seen_team_message_ids.clear();
        if matches!(
            self.overlay,
            Some(OverlayState::TeamView { .. }) | Some(OverlayState::ApprovalPicker { .. })
        ) {
            self.overlay = None;
        }
    }

    #[allow(dead_code)]
    pub fn open_session_picker(&mut self) {
        self.overlay = Some(OverlayState::SessionPicker { selected: 0 });
    }

    pub fn open_command_picker(&mut self) {
        self.overlay = Some(OverlayState::CommandPicker { selected: 0 });
    }

    pub fn open_daemon_picker(&mut self) {
        self.overlay = Some(OverlayState::DaemonPicker { selected: 0 });
    }

    pub fn open_task_picker(&mut self) {
        self.overlay = Some(OverlayState::TaskPicker { selected: 0 });
    }

    pub fn open_task_action_picker(&mut self, task_id: impl Into<String>) {
        self.overlay = Some(OverlayState::TaskActionPicker {
            task_id: task_id.into(),
            selected: 0,
        });
    }

    pub fn open_team_picker(&mut self) {
        self.overlay = Some(OverlayState::TeamPicker { selected: 0 });
    }

    pub fn open_provider_picker(&mut self) {
        self.overlay = Some(OverlayState::ProviderPicker { selected: 0 });
    }

    pub fn open_model_picker(&mut self, provider: impl Into<String>) {
        self.overlay = Some(OverlayState::ModelPicker {
            provider: provider.into(),
            selected: 0,
        });
    }

    #[allow(dead_code)]
    pub fn open_run_picker(&mut self) {
        self.overlay = Some(OverlayState::RunPicker { selected: 0 });
    }

    #[allow(dead_code)]
    pub fn open_approval_picker(&mut self) {
        self.overlay = Some(OverlayState::ApprovalPicker { selected: 0 });
    }

    #[allow(dead_code)]
    pub fn open_team_overlay(&mut self) {
        self.overlay = Some(OverlayState::TeamView {
            tab: TeamOverlayTab::Members,
            scroll: 0,
        });
    }

    #[allow(dead_code)]
    pub fn open_help_overlay(&mut self) {
        self.overlay = Some(OverlayState::Help);
    }

    pub fn move_overlay_selection(&mut self, delta: isize) {
        self.pending_session_delete_id = None;
        let len = match self.overlay_item_len() {
            Some(len) if len > 0 => len,
            _ => return,
        };
        match self.overlay.as_mut() {
            Some(OverlayState::CommandPicker { selected })
            | Some(OverlayState::DaemonPicker { selected })
            | Some(OverlayState::SessionPicker { selected })
            | Some(OverlayState::TaskPicker { selected })
            | Some(OverlayState::TaskActionPicker { selected, .. })
            | Some(OverlayState::TeamPicker { selected })
            | Some(OverlayState::ProviderPicker { selected })
            | Some(OverlayState::ModelPicker { selected, .. })
            | Some(OverlayState::RunPicker { selected })
            | Some(OverlayState::ApprovalPicker { selected }) => {
                let next = (*selected as isize + delta).clamp(0, len.saturating_sub(1) as isize);
                *selected = next as usize;
            }
            Some(OverlayState::TeamView { scroll, .. }) => {
                let next = (*scroll as i16 + delta as i16).max(0) as u16;
                *scroll = next;
            }
            Some(OverlayState::Help) | None => {}
        }
    }

    pub fn sync_command_picker_to_draft(
        &mut self,
        commands: &[super::slash_command::SlashCommandSpec],
    ) {
        let Some(OverlayState::CommandPicker { selected }) = self.overlay.as_mut() else {
            return;
        };
        let draft = self.composer.draft().trim();
        if !draft.starts_with('/') {
            self.overlay = None;
            return;
        }
        if let Some(index) = commands
            .iter()
            .position(|spec| spec.command.starts_with(draft))
        {
            *selected = index;
        }
    }

    #[allow(dead_code)]
    pub fn cycle_team_tab(&mut self, forward: bool) {
        if let Some(OverlayState::TeamView { tab, .. }) = self.overlay.as_mut() {
            *tab = match (*tab, forward) {
                (TeamOverlayTab::Members, true) => TeamOverlayTab::Messages,
                (TeamOverlayTab::Messages, true) => TeamOverlayTab::Assignments,
                (TeamOverlayTab::Assignments, true) => TeamOverlayTab::Approvals,
                (TeamOverlayTab::Approvals, true) => TeamOverlayTab::Members,
                (TeamOverlayTab::Members, false) => TeamOverlayTab::Approvals,
                (TeamOverlayTab::Messages, false) => TeamOverlayTab::Members,
                (TeamOverlayTab::Assignments, false) => TeamOverlayTab::Messages,
                (TeamOverlayTab::Approvals, false) => TeamOverlayTab::Assignments,
            };
        }
    }

    pub fn overlay_item_len(&self) -> Option<usize> {
        match self.overlay.as_ref()? {
            OverlayState::CommandPicker { .. } => {
                Some(super::slash_command::SLASH_COMMAND_SPECS.len())
            }
            OverlayState::DaemonPicker { .. } => Some(2),
            OverlayState::SessionPicker { .. } => Some(self.sessions.len()),
            OverlayState::TaskPicker { .. } => Some(self.tasks.len()),
            OverlayState::TaskActionPicker { .. } => Some(3),
            OverlayState::TeamPicker { .. } => Some(self.team_items.len()),
            OverlayState::ProviderPicker { .. } => Some(self.provider_items.len()),
            OverlayState::ModelPicker { .. } => Some(self.model_items.len()),
            OverlayState::RunPicker { .. } => Some(self.run_picker_items().len()),
            OverlayState::ApprovalPicker { .. } => Some(self.current_team_approvals.len()),
            OverlayState::TeamView { .. } | OverlayState::Help => None,
        }
    }

    pub fn selected_session_id(&self) -> Option<&str> {
        match self.overlay.as_ref() {
            Some(OverlayState::SessionPicker { selected }) => self
                .sessions
                .get(*selected)
                .map(|session| session.id.as_str()),
            _ => None,
        }
    }

    pub fn selected_session_summary(&self) -> Option<&ChatSessionSummary> {
        match self.overlay.as_ref() {
            Some(OverlayState::SessionPicker { selected }) => self.sessions.get(*selected),
            _ => None,
        }
    }

    pub fn mark_session_delete_pending(&mut self, session_id: impl Into<String>) {
        self.pending_session_delete_id = Some(session_id.into());
    }

    pub fn is_session_delete_pending(&self, session_id: &str) -> bool {
        self.pending_session_delete_id.as_deref() == Some(session_id)
    }

    pub fn apply_session_delete_result(
        &mut self,
        session_id: &str,
        sessions: Vec<ChatSessionSummary>,
    ) {
        self.sessions = sessions;
        self.pending_session_delete_id = None;
        if self.current_session_id() == Some(session_id) {
            self.clear_current_session("Deleted current session.");
        }
        if self.sessions.is_empty()
            && matches!(self.overlay, Some(OverlayState::SessionPicker { .. }))
        {
            self.overlay = None;
            return;
        }
        if let Some(OverlayState::SessionPicker { selected }) = self.overlay.as_mut() {
            *selected = (*selected).min(self.sessions.len().saturating_sub(1));
        }
    }

    pub fn selected_command_index(&self) -> Option<usize> {
        match self.overlay.as_ref() {
            Some(OverlayState::CommandPicker { selected }) => Some(*selected),
            _ => None,
        }
    }

    pub fn selected_daemon_action(&self) -> Option<&'static str> {
        match self.overlay.as_ref() {
            Some(OverlayState::DaemonPicker { selected: 0 }) => Some("start"),
            Some(OverlayState::DaemonPicker { selected: 1 }) => Some("stop"),
            _ => None,
        }
    }

    pub fn selected_task_id(&self) -> Option<&str> {
        match self.overlay.as_ref() {
            Some(OverlayState::TaskPicker { selected }) => {
                self.tasks.get(*selected).map(|task| task.task_id.as_str())
            }
            _ => None,
        }
    }

    pub fn selected_task_action(&self) -> Option<(String, &'static str)> {
        match self.overlay.as_ref() {
            Some(OverlayState::TaskActionPicker { task_id, selected }) => {
                let action = match *selected {
                    0 => "pause",
                    1 => "resume",
                    2 => "stop",
                    _ => return None,
                };
                Some((task_id.clone(), action))
            }
            _ => None,
        }
    }

    pub fn selected_team_item(&self) -> Option<TeamPickerItem> {
        match self.overlay.as_ref() {
            Some(OverlayState::TeamPicker { selected }) => self.team_items.get(*selected).cloned(),
            _ => None,
        }
    }

    pub fn selected_provider_item(&self) -> Option<ProviderPickerItem> {
        match self.overlay.as_ref() {
            Some(OverlayState::ProviderPicker { selected }) => {
                self.provider_items.get(*selected).cloned()
            }
            _ => None,
        }
    }

    pub fn selected_model_item(&self) -> Option<ModelPickerItem> {
        match self.overlay.as_ref() {
            Some(OverlayState::ModelPicker { selected, .. }) => {
                self.model_items.get(*selected).cloned()
            }
            _ => None,
        }
    }

    pub fn selected_run_picker_item(&self) -> Option<RunPickerItem> {
        match self.overlay.as_ref() {
            Some(OverlayState::RunPicker { selected }) => {
                self.run_picker_items().get(*selected).cloned()
            }
            _ => None,
        }
    }

    pub fn run_picker_items(&self) -> Vec<RunPickerItem> {
        let mut items = Vec::new();
        items.extend(self.thread.runs.iter().filter_map(|run| {
            run.run_id.as_ref().map(|run_id| RunPickerItem::Run {
                run_id: run_id.clone(),
                title: run.title.clone(),
                status: run.status.clone(),
            })
        }));
        items.extend(self.thread.child_runs.iter().filter_map(|run| {
            run.run_id.as_ref().map(|run_id| RunPickerItem::Run {
                run_id: run_id.clone(),
                title: format!("-> {}", run.title),
                status: run.status.clone(),
            })
        }));
        items
    }

    pub fn push_message(&mut self, message: ShellMessage) {
        if matches!(
            message,
            ShellMessage::ToolCall { .. } | ShellMessage::ToolResult { .. }
        ) {
            self.push_tool_message(message);
            return;
        }

        let cell = cell_from_message(&message, self.assistant_name());
        if cell.is_conversation_cell() {
            self.conversation_cells.push(cell);
        } else {
            self.runtime_cells.push(AnchoredRuntimeCell {
                base_cell_index: self.conversation_cells.len(),
                cell,
            });
        }
    }

    fn finalize_active_cell(&mut self) {
        self.finish_active_assistant_segment();
        let live_cells = std::mem::take(&mut self.active_turn_cells);
        self.active_assistant_stream_body.clear();
        self.active_tool_call_ids.clear();
        self.active_tool_result_ids.clear();
        let mut base_cell_index = self.conversation_cells.len();
        for mut cell in live_cells {
            match cell.kind {
                TranscriptCellKind::Assistant if !cell.body.trim().is_empty() => {
                    let _ = cell.finalize();
                    self.conversation_cells.push(cell);
                    base_cell_index = self.conversation_cells.len();
                }
                TranscriptCellKind::Tool => {
                    self.runtime_cells.push(AnchoredRuntimeCell {
                        base_cell_index,
                        cell,
                    });
                }
                _ => {}
            }
        }
    }

    fn clear_active_response(&mut self) {
        self.active_cell = None;
        self.active_turn_cells.clear();
        self.active_typing_started_at_ms = None;
        self.active_assistant_stream_body.clear();
        self.active_tool_call_ids.clear();
        self.active_tool_result_ids.clear();
    }

    fn finish_active_assistant_segment(&mut self) {
        let Some(mut active_cell) = self.active_cell.take() else {
            return;
        };
        self.active_typing_started_at_ms = None;
        self.active_assistant_stream_body.clear();
        active_cell.body = active_cell.body.trim_end().to_string();
        if !active_cell.body.trim().is_empty() {
            let _ = active_cell.finalize();
            self.active_turn_cells.push(active_cell);
        }
    }

    pub fn start_assistant_typing(&mut self) {
        if self.active_cell.is_none() {
            self.active_cell = Some(cell_from_message(
                &ShellMessage::AssistantStream {
                    content: String::new(),
                },
                self.assistant_name(),
            ));
        }
        if self.active_typing_started_at_ms.is_none() {
            self.active_typing_started_at_ms = Some(Utc::now().timestamp_millis());
        }
        let _ = self.update_active_typing_indicator();
    }

    pub fn cancel_active_response(&mut self) {
        self.clear_active_response();
        self.is_streaming = false;
    }

    pub fn update_active_typing_indicator(&mut self) -> bool {
        self.update_active_typing_indicator_at(Utc::now().timestamp_millis())
    }

    fn update_active_typing_indicator_at(&mut self, now_ms: i64) -> bool {
        let Some(active_cell) = self.active_cell.as_mut() else {
            return false;
        };
        if !active_cell.is_active {
            return false;
        }
        let started_at = *self.active_typing_started_at_ms.get_or_insert(now_ms);
        let elapsed_ms = now_ms.saturating_sub(started_at);
        let elapsed_secs = elapsed_ms / 1000;
        let frame = match (elapsed_ms / 250) % 4 {
            0 => "typing",
            1 => "typing.",
            2 => "typing..",
            _ => "typing...",
        };
        let next = format!("{frame:<9} {elapsed_secs}s");
        if active_cell.subtitle.as_deref() == Some(next.as_str()) {
            return false;
        }
        active_cell.subtitle = Some(next);
        true
    }

    fn push_tool_message(&mut self, message: ShellMessage) {
        if self.active_cell.is_some() || self.is_streaming || !self.pending_user_cells.is_empty() {
            match &message {
                ShellMessage::ToolCall {
                    call_id,
                    name,
                    arguments,
                } => {
                    self.append_tool_call_to_active(call_id, name, arguments);
                    return;
                }
                ShellMessage::ToolResult {
                    call_id,
                    success,
                    result,
                } => {
                    self.append_tool_result_to_active(call_id, *success, result);
                    return;
                }
                _ => {}
            }
        }

        match &message {
            ShellMessage::ToolCall { call_id, .. }
                if self
                    .runtime_cells
                    .iter()
                    .any(|entry| entry.cell.tool_call_id() == Some(call_id.as_str())) =>
            {
                return;
            }
            ShellMessage::ToolCall { .. } => {}
            ShellMessage::ToolResult {
                call_id,
                success,
                result,
            } => {
                if let Some(entry) = self
                    .runtime_cells
                    .iter_mut()
                    .find(|entry| entry.cell.tool_call_id() == Some(call_id.as_str()))
                {
                    let _ = entry.cell.merge_tool_result(*success, result);
                    return;
                }
            }
            _ => {}
        }

        let cell = cell_from_message(&message, self.assistant_name());
        self.runtime_cells.push(AnchoredRuntimeCell {
            base_cell_index: self.conversation_cells.len(),
            cell,
        });
    }

    fn ensure_active_assistant_cell(&mut self) {
        if self.active_cell.is_none() {
            self.active_cell = Some(cell_from_message(
                &ShellMessage::AssistantStream {
                    content: String::new(),
                },
                self.assistant_name(),
            ));
        }
        if self.active_typing_started_at_ms.is_none() {
            self.active_typing_started_at_ms = Some(Utc::now().timestamp_millis());
        }
        let _ = self.update_active_typing_indicator();
    }

    fn append_tool_call_to_active(&mut self, call_id: &str, name: &str, arguments: &str) {
        if !self.active_tool_call_ids.insert(call_id.to_string()) {
            return;
        }
        self.finish_active_assistant_segment();
        self.active_turn_cells.push(cell_from_message(
            &ShellMessage::ToolCall {
                call_id: call_id.to_string(),
                name: name.to_string(),
                arguments: arguments.to_string(),
            },
            self.assistant_name(),
        ));
    }

    fn append_tool_result_to_active(&mut self, call_id: &str, success: bool, result: &str) {
        if !self.active_tool_result_ids.insert(call_id.to_string()) {
            return;
        }
        if let Some(cell) = self
            .active_turn_cells
            .iter_mut()
            .find(|cell| cell.tool_call_id() == Some(call_id))
        {
            let _ = cell.merge_tool_result(success, result);
            return;
        }
        self.active_turn_cells.push(cell_from_message(
            &ShellMessage::ToolResult {
                call_id: call_id.to_string(),
                success,
                result: result.to_string(),
            },
            self.assistant_name(),
        ));
    }

    pub fn push_local_user_message(&mut self, content: String) {
        self.reset_message_scroll();
        let base_cell_index = self.conversation_cells.len();
        let cell = cell_from_message(
            &ShellMessage::UserMessage { content },
            self.assistant_name(),
        );
        self.pending_user_cells.push(PendingUserCell {
            base_cell_index,
            cell,
        });
    }

    pub fn replace_session_projection(&mut self, messages: Vec<ShellMessage>) {
        self.conversation_cells = transcript_cells(&messages, self.assistant_name());
    }

    pub fn push_info(&mut self, content: impl Into<String>) {
        self.push_message(ShellMessage::InfoNotice {
            content: content.into(),
        });
    }

    pub fn push_error(&mut self, content: impl Into<String>) {
        self.push_message(ShellMessage::ErrorNotice {
            content: content.into(),
        });
    }

    pub fn scroll_message_up(&mut self, rows: usize) {
        self.message_scroll_from_bottom = self.message_scroll_from_bottom.saturating_add(rows);
    }

    pub fn scroll_message_down(&mut self, rows: usize) {
        self.message_scroll_from_bottom = self.message_scroll_from_bottom.saturating_sub(rows);
    }

    pub fn reset_message_scroll(&mut self) {
        self.message_scroll_from_bottom = 0;
    }

    pub fn record_team_message(&mut self, message: &TeamMessage) {
        if self
            .seen_team_message_ids
            .insert(message.message_id.clone())
        {
            self.push_message(message_from_team_message(message));
        }
    }

    pub fn apply_team_snapshot(
        &mut self,
        team_state: Option<TeamState>,
        messages: Vec<TeamMessage>,
        assignments: Vec<TeamAssignment>,
        status: impl Into<String>,
        open_overlay: bool,
    ) {
        self.current_team_state = team_state;
        self.current_team_messages = messages;
        self.current_team_assignments = assignments;
        let team_messages = self.current_team_messages.clone();
        for message in &team_messages {
            self.record_team_message(message);
        }
        self.rebuild_pending_approvals();
        self.status = status.into();
        if open_overlay {
            self.open_team_overlay();
        }
    }

    fn append_assistant_stream_chunk(&mut self, chunk: &str) {
        if chunk.is_empty() {
            return;
        }

        if self.active_typing_started_at_ms.is_none() {
            self.active_typing_started_at_ms = Some(Utc::now().timestamp_millis());
        }

        let Some(delta) = assistant_stream_delta(&mut self.active_assistant_stream_body, chunk)
        else {
            return;
        };

        if delta.trim().is_empty() {
            return;
        }

        self.ensure_active_assistant_cell();
        if let Some(active_cell) = self.active_cell.as_mut() {
            append_active_text(&mut active_cell.body, &delta);
        }
        let _ = self.update_active_typing_indicator();
    }

    pub fn apply_stream_frame(&mut self, frame: StreamFrame) {
        match frame {
            StreamFrame::Start { stream_id } => {
                self.is_streaming = true;
                self.status = format!("Streaming response ({stream_id})");
            }
            StreamFrame::Ack { content } => {
                self.is_streaming = true;
                self.append_assistant_stream_chunk(&content);
            }
            StreamFrame::Data { content } => {
                self.is_streaming = true;
                self.append_assistant_stream_chunk(&content);
            }
            StreamFrame::Done { total_tokens } => {
                self.is_streaming = false;
                self.finalize_active_cell();
                self.status = match total_tokens {
                    Some(total_tokens) => format!("Stream finished ({total_tokens} tokens)"),
                    None => "Stream finished".to_string(),
                };
            }
            other => {
                if let Some(message) = message_from_stream_frame(&other) {
                    if matches!(message, ShellMessage::ErrorNotice { .. }) {
                        self.is_streaming = false;
                        self.status = "Stream failed".to_string();
                        self.cancel_active_response();
                    }
                    self.push_message(message);
                }
            }
        }
    }

    pub fn apply_session_event(&mut self, event: ChatSessionEvent) {
        if let Some(message) = message_from_session_event(&event) {
            self.push_message(message);
        }
    }

    pub fn apply_task_event(&mut self, event: TaskStreamEvent) {
        self.push_message(message_from_task_event(&event));
    }

    pub fn rebuild_pending_approvals(&mut self) {
        self.current_team_approvals = self
            .current_team_messages
            .iter()
            .filter(|message| message.kind == TeamMessageKind::ApprovalRequest)
            .map(|message| PendingTeamApproval {
                team_run_id: message.team_run_id.clone(),
                approval_id: message
                    .content
                    .split_whitespace()
                    .last()
                    .unwrap_or_default()
                    .trim_matches(|ch| ch == '(' || ch == ')')
                    .to_string(),
                member_id: message.from_member_id.clone(),
                tool_name: "unknown".to_string(),
                content: message.content.clone(),
                status: restflow_traits::TeamApprovalStatus::Pending,
                requested_at: message.created_at,
                resolved_at: None,
                resolution_reason: None,
            })
            .collect();
    }

    #[allow(dead_code)]
    pub fn transcript_cells_for_render(&self) -> Vec<TranscriptCell> {
        let mut cells = Vec::with_capacity(
            self.conversation_cells.len()
                + self.pending_user_cells.len()
                + self.runtime_cells.len()
                + self.active_turn_cells.len()
                + usize::from(self.active_cell.is_some()),
        );

        let mut pending = self.pending_user_cells.iter().peekable();
        let mut runtime = self.runtime_cells.iter().peekable();
        for (index, cell) in self.conversation_cells.iter().enumerate() {
            while let Some(entry) = pending.peek() {
                if entry.base_cell_index <= index {
                    cells.push(entry.cell.clone());
                    pending.next();
                } else {
                    break;
                }
            }

            if runtime
                .peek()
                .is_some_and(|entry| entry.base_cell_index == index)
                && cell.kind == TranscriptCellKind::User
            {
                cells.push(cell.clone());
                while let Some(entry) = runtime.peek() {
                    if entry.base_cell_index == index {
                        cells.push(entry.cell.clone());
                        runtime.next();
                    } else {
                        break;
                    }
                }
                continue;
            }

            while let Some(entry) = runtime.peek() {
                if entry.base_cell_index == index {
                    cells.push(entry.cell.clone());
                    runtime.next();
                } else {
                    break;
                }
            }

            cells.push(cell.clone());
        }

        for entry in pending {
            cells.push(entry.cell.clone());
        }

        for entry in runtime {
            cells.push(entry.cell.clone());
        }
        cells.extend(self.active_turn_cells.iter().cloned());
        if let Some(active_cell) = self.active_cell.clone() {
            cells.push(active_cell);
        }
        cells
    }

    fn reconcile_pending_user_cells(&mut self) {
        self.pending_user_cells.retain(|entry| {
            !self
                .conversation_cells
                .iter()
                .skip(entry.base_cell_index)
                .any(|cell| cell.kind == TranscriptCellKind::User && cell.body == entry.cell.body)
        });
    }

    fn assistant_name(&self) -> &str {
        self.default_agent_name.as_deref().unwrap_or("Agent")
    }
}

fn assistant_stream_delta(current: &mut String, chunk: &str) -> Option<String> {
    let normalized = chunk.trim_start_matches(['\r', '\n']);
    if chunk == current
        || normalized == current
        || (!normalized.trim().is_empty() && normalized.trim() == current.trim())
        || current.starts_with(chunk)
        || current.starts_with(normalized)
    {
        return None;
    }

    if let Some(delta) = chunk.strip_prefix(current.as_str()) {
        *current = chunk.to_string();
        return Some(delta.to_string());
    }

    if let Some(delta) = normalized.strip_prefix(current.as_str()) {
        *current = normalized.to_string();
        return Some(delta.to_string());
    }

    let delta = if current.is_empty() {
        normalized
    } else {
        chunk
    };
    current.push_str(delta);
    Some(delta.to_string())
}

fn append_active_text(body: &mut String, text: &str) {
    let text = if body.is_empty() {
        text.trim_start_matches(['\r', '\n'])
    } else {
        text
    };
    body.push_str(text);
}

#[cfg(test)]
mod tests {
    use super::{AppState, OverlayState};
    use crate::transcript::{TranscriptCellKind, transcript_cells};
    use restflow_core::daemon::{ChatSessionEvent, StreamFrame};
    use restflow_traits::{TeamMessage, TeamMessageKind};

    #[test]
    fn app_state_session_picker_uses_overlay() {
        let mut state = AppState::empty();
        state.open_session_picker();
        assert!(matches!(
            state.overlay,
            Some(OverlayState::SessionPicker { .. })
        ));
    }

    #[test]
    fn stream_frames_merge_into_one_assistant_message() {
        let mut state = AppState::empty();
        state.apply_stream_frame(StreamFrame::Ack {
            content: "hel".to_string(),
        });
        state.apply_stream_frame(StreamFrame::Data {
            content: "lo".to_string(),
        });
        assert!(state.active_cell.is_some());
        state.apply_stream_frame(StreamFrame::Done { total_tokens: None });

        assert_eq!(state.conversation_cells.len(), 1);
        assert!(state.runtime_cells.is_empty());
        assert!(state.active_cell.is_none());
        assert_eq!(
            state.conversation_cells[0].kind,
            TranscriptCellKind::Assistant
        );
        assert_eq!(state.conversation_cells[0].body, "hello");
    }

    #[test]
    fn assistant_typing_indicator_animates_with_elapsed_time() {
        let mut state = AppState::empty();
        state.start_assistant_typing();
        let started_at = state.active_typing_started_at_ms.expect("typing start");

        state.update_active_typing_indicator_at(started_at);
        let initial = state
            .active_cell
            .as_ref()
            .and_then(|cell| cell.subtitle.as_deref())
            .expect("subtitle")
            .to_string();
        assert_eq!(initial, "typing    0s");

        state.update_active_typing_indicator_at(started_at + 500);
        let animated = state
            .active_cell
            .as_ref()
            .and_then(|cell| cell.subtitle.as_deref())
            .expect("subtitle")
            .to_string();
        assert_eq!(animated, "typing..  0s");

        state.update_active_typing_indicator_at(started_at + 1_250);
        let elapsed = state
            .active_cell
            .as_ref()
            .and_then(|cell| cell.subtitle.as_deref())
            .expect("subtitle");
        assert_eq!(elapsed, "typing.   1s");
    }

    #[test]
    fn tool_frames_render_as_live_runtime_cells_in_the_active_turn() {
        let mut state = AppState::empty();
        state.apply_stream_frame(StreamFrame::Ack {
            content: "Checking...".to_string(),
        });
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-1".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({"cmd": "pwd"}),
        });
        state.apply_stream_frame(StreamFrame::ToolResult {
            id: "call-1".to_string(),
            result: "{\"cwd\":\"/tmp\"}".to_string(),
            success: true,
        });
        state.apply_stream_frame(StreamFrame::Data {
            content: "Done.".to_string(),
        });

        assert!(state.active_cell.is_some());
        assert!(state.conversation_cells.is_empty());
        assert!(state.runtime_cells.is_empty());
        assert_eq!(state.active_turn_cells.len(), 2);
        let active = state.active_cell.as_ref().expect("active assistant");
        assert_eq!(active.kind, TranscriptCellKind::Assistant);
        assert!(state.active_turn_cells[0].body.contains("Checking..."));
        assert!(active.body.contains("Done."));
        assert_eq!(state.active_turn_cells[1].title, "Tool · bash");
        assert!(state.active_turn_cells[1].body.contains("Input:"));
        assert!(state.active_turn_cells[1].body.contains("Output:"));

        let cells = state.transcript_cells_for_render();
        let kinds = cells.iter().map(|cell| cell.kind).collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                TranscriptCellKind::Assistant,
                TranscriptCellKind::Tool,
                TranscriptCellKind::Assistant,
            ]
        );

        state.apply_stream_frame(StreamFrame::Done { total_tokens: None });

        assert_eq!(state.conversation_cells.len(), 2);
        assert!(state.active_cell.is_none());
        assert!(state.active_turn_cells.is_empty());
        assert_eq!(state.runtime_cells.len(), 1);
        assert!(
            !state.conversation_cells[0]
                .body
                .contains("Tool · bash #call-1")
        );
    }

    #[test]
    fn blank_stream_chunks_do_not_create_empty_assistant_cells() {
        let mut state = AppState::empty();
        state.apply_stream_frame(StreamFrame::Ack {
            content: "   ".to_string(),
        });
        state.apply_stream_frame(StreamFrame::Data {
            content: String::new(),
        });
        state.apply_stream_frame(StreamFrame::Done { total_tokens: None });

        assert!(state.conversation_cells.is_empty());
        assert!(state.active_cell.is_none());
    }

    #[test]
    fn team_messages_are_deduped_in_transcript() {
        let mut state = AppState::empty();
        let message = TeamMessage {
            team_run_id: "team-1".to_string(),
            message_id: "message-1".to_string(),
            from_member_id: "leader".to_string(),
            to_member_id: None,
            kind: TeamMessageKind::Note,
            content: "hello".to_string(),
            created_at: 1,
        };

        state.record_team_message(&message);
        state.record_team_message(&message);

        assert_eq!(state.runtime_cells.len(), 1);
    }

    #[test]
    fn run_picker_uses_only_thread_runs() {
        let mut state = AppState::empty();
        state.thread.runs.push(restflow_core::models::RunSummary {
            id: "run-local".to_string(),
            kind: restflow_core::models::RunKind::WorkspaceRun,
            container_id: "session-1".to_string(),
            root_run_id: Some("run-local".to_string()),
            title: "Run One".to_string(),
            subtitle: None,
            status: "running".to_string(),
            updated_at: 1,
            started_at: Some(1),
            ended_at: None,
            session_id: Some("session-1".to_string()),
            run_id: Some("run-local".to_string()),
            task_id: None,
            parent_run_id: None,
            agent_id: Some("agent-1".to_string()),
            source_channel: None,
            source_conversation_id: None,
            effective_model: None,
            provider: None,
            event_count: 0,
        });

        let items = state.run_picker_items();
        assert_eq!(items.len(), 1);
        assert!(matches!(items[0], super::RunPickerItem::Run { .. }));
    }

    #[test]
    fn refresh_current_session_preserves_notice_messages() {
        let mut state = AppState::empty();
        let mut session =
            restflow_core::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        session
            .messages
            .push(restflow_core::models::ChatMessage::user("hello"));
        state.set_current_session(session.clone());
        state.push_info("notice");

        let mut updated = session.clone();
        updated
            .messages
            .push(restflow_core::models::ChatMessage::assistant("hi"));
        state.refresh_current_session(updated);

        assert_eq!(state.conversation_cells.len(), 2);
        assert_eq!(state.runtime_cells.len(), 1);
        assert_eq!(state.runtime_cells[0].cell.title, "Info");
    }

    #[test]
    fn refresh_current_session_keeps_pending_user_message_until_persisted() {
        let mut state = AppState::empty();
        let session =
            restflow_core::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session.clone());
        state.push_local_user_message("hello".to_string());

        state.refresh_current_session(session.clone());
        assert_eq!(state.pending_user_cells.len(), 1);

        let mut updated = session;
        updated
            .messages
            .push(restflow_core::models::ChatMessage::user("hello"));
        state.refresh_current_session(updated);
        assert!(state.pending_user_cells.is_empty());
        assert_eq!(state.conversation_cells.len(), 1);
        assert_eq!(state.conversation_cells[0].body, "hello");
    }

    #[test]
    fn pending_user_message_stays_before_local_assistant_finalize() {
        let mut state = AppState::empty();
        let session =
            restflow_core::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session);
        state.push_local_user_message("123".to_string());
        state.apply_stream_frame(StreamFrame::Ack {
            content: "hello".to_string(),
        });
        state.apply_stream_frame(StreamFrame::Done { total_tokens: None });

        let rendered = state.transcript_cells_for_render();
        assert_eq!(rendered.len(), 2);
        assert_eq!(rendered[0].kind, TranscriptCellKind::User);
        assert_eq!(rendered[0].body, "123");
        assert_eq!(rendered[1].kind, TranscriptCellKind::Assistant);
        assert_eq!(rendered[1].body, "hello");
    }

    #[test]
    fn clear_current_session_keeps_notices() {
        let mut state = AppState::empty();
        let mut session =
            restflow_core::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        session
            .messages
            .push(restflow_core::models::ChatMessage::user("hello"));
        state.set_current_session(session);
        state.push_info("notice");

        state.clear_current_session("session missing");

        assert_eq!(state.conversation_cells.len(), 0);
        assert_eq!(state.runtime_cells.len(), 1);
        assert_eq!(state.runtime_cells[0].cell.title, "Info");
    }

    #[test]
    fn switching_session_clears_team_context() {
        let mut state = AppState::empty();
        let first =
            restflow_core::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        let second =
            restflow_core::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(first);
        state.current_team_state = Some(restflow_traits::TeamState {
            team_run_id: "team-1".to_string(),
            leader_member_id: "leader".to_string(),
            members: Vec::new(),
            status: restflow_traits::TeamStatus::Running,
            pending_message_count: 1,
            pending_assignment_count: 0,
            updated_at: 1,
        });
        state.open_team_overlay();

        state.set_current_session(second);

        assert!(state.current_team_state.is_none());
        assert!(state.current_team_messages.is_empty());
        assert!(state.current_team_assignments.is_empty());
        assert!(state.current_team_approvals.is_empty());
        assert!(state.overlay.is_none());
    }

    #[test]
    fn session_events_do_not_append_debug_notices() {
        let mut state = AppState::empty();
        state.apply_session_event(ChatSessionEvent::MessageAdded {
            session_id: "session-1".to_string(),
            source: "ipc".to_string(),
        });
        assert!(state.conversation_cells.is_empty());
        assert!(state.runtime_cells.is_empty());
        assert!(state.active_cell.is_none());
    }

    #[test]
    fn set_current_session_resets_runtime_cells_for_new_session() {
        let mut state = AppState::empty();
        state.push_info("notice");
        let mut session =
            restflow_core::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        session
            .messages
            .push(restflow_core::models::ChatMessage::user("hello"));

        state.set_current_session(session);

        assert_eq!(state.conversation_cells.len(), 1);
        assert_eq!(state.conversation_cells[0].kind, TranscriptCellKind::User);
        assert!(state.runtime_cells.is_empty());
    }

    #[test]
    fn refresh_current_session_preserves_runtime_cells_and_active_cell() {
        let mut state = AppState::empty();
        let mut session =
            restflow_core::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        session
            .messages
            .push(restflow_core::models::ChatMessage::user("hello"));
        state.set_current_session(session.clone());
        state.push_info("notice");
        state.active_cell = transcript_cells(
            &[crate::transcript::ShellMessage::AssistantStream {
                content: "chunk".to_string(),
            }],
            "Agent",
        )
        .into_iter()
        .next();

        let mut updated = session.clone();
        updated
            .messages
            .push(restflow_core::models::ChatMessage::assistant("reply"));
        state.refresh_current_session(updated);

        assert_eq!(state.conversation_cells.len(), 2);
        assert_eq!(state.runtime_cells.len(), 1);
        assert_eq!(state.runtime_cells[0].cell.title, "Info");
        assert!(state.active_cell.is_some());
    }

    #[test]
    fn startup_state_tracks_daemon_bootstrap_flow() {
        let mut state = AppState::empty();
        state.enter_startup(Some("agent-1".to_string()), Some("session-1".to_string()));

        assert!(state.is_startup_mode());
        assert_eq!(
            state
                .startup_state()
                .and_then(|startup| startup.agent_override.as_deref()),
            Some("agent-1")
        );

        state.set_startup_error("boom".to_string());
        assert_eq!(
            state
                .startup_state()
                .and_then(|startup| startup.error.as_deref()),
            Some("boom")
        );

        state.exit_startup();
        assert!(!state.is_startup_mode());
    }
}
