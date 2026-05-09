use std::collections::{HashMap, HashSet};

use chrono::Utc;
use runtime::models::{
    ChatRole, ChatSession, ChatSessionSummary, ChatTurnEventKind, ChatTurnStatus, ExecutionThread,
    ModelId, ModelMetadataDTO, Provider, RunKind, RunSummary, Skill, SkillSource,
};
use runtime::storage::agent::StoredAgent;
use types::{ChatSessionEvent, StreamEventKind, StreamFrame, TaskStreamEvent};

use super::activity::{ActivityState, BackgroundWorkStatus};
use super::composer::ComposerState;
use super::transcript::{
    MessageGroup, ShellMessage, TranscriptCell, TranscriptCellKind, cell_from_message,
    message_from_session_event, message_from_stream_frame, message_from_task_event,
    messages_from_session, transcript_cells,
};

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum WorkPickerItem {
    BackgroundTask {
        task_id: String,
        title: String,
        status: String,
        next_run_at: Option<i64>,
        latest_run_id: Option<String>,
    },
    Run {
        run_id: String,
        root_run_id: Option<String>,
        parent_run_id: Option<String>,
        kind: RunKind,
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
    pub latest_run_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillPickerItem {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub source: SkillSource,
    pub read_only: bool,
}

impl From<Skill> for SkillPickerItem {
    fn from(skill: Skill) -> Self {
        Self {
            id: skill.id,
            name: skill.name,
            description: skill.description,
            source: skill.source,
            read_only: skill.read_only,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillManagerSelection {
    Skill(SkillPickerItem),
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
            .resolved_model_ref()
            .map(|model_ref| model_ref.model.as_serialized_str().to_string())
            .unwrap_or_else(|| ModelId::Gpt5_4.as_serialized_str().to_string());
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
        let model = display_model_for_provider(&self.provider, &self.model);
        if self.provider.trim().is_empty() {
            model
        } else {
            format!("{} · {}", self.provider, model)
        }
    }
}

fn display_model_for_provider(provider: &str, model: &str) -> String {
    Provider::from_canonical_str(provider.trim())
        .and_then(|provider| ModelId::for_provider_and_model(provider, model))
        .or_else(|| ModelId::from_serialized_str(model))
        .map(|model_id| model_id.as_str().to_string())
        .unwrap_or_else(|| model.to_string())
}

fn model_display_name(model: &str) -> String {
    ModelId::from_serialized_str(model)
        .map(|model_id| model_id.metadata().name.to_string())
        .unwrap_or_else(|| model.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnchoredRuntimeCell {
    pub base_cell_index: usize,
    pub cell: TranscriptCell,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ActiveTurn {
    pub cells: Vec<TranscriptCell>,
    pub queued_updates: Vec<String>,
    active_assistant_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ThreadFocus {
    #[default]
    Session,
    Run {
        run_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    #[default]
    Chat,
    Plan,
    Task,
}

impl InputMode {
    pub fn next(self) -> Self {
        match self {
            Self::Chat => Self::Plan,
            Self::Plan => Self::Task,
            Self::Task => Self::Chat,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Chat => "Chat",
            Self::Plan => "Plan",
            Self::Task => "Task",
        }
    }
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
#[derive(Debug, Clone)]
pub enum OverlayState {
    CommandPicker { selected: usize },
    DaemonPicker { selected: usize },
    SessionPicker { selected: usize },
    SkillManager { selected: usize },
    SkillMentionPicker { selected: usize },
    SkillDetail,
    TaskPicker { selected: usize },
    TaskActionPicker { task_id: String, selected: usize },
    ProviderPicker { selected: usize },
    ModelPicker { provider: String, selected: usize },
    RunPicker { selected: usize },
    RunDetail,
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
    pub sessions: Vec<ChatSessionSummary>,
    pub skills: Vec<SkillPickerItem>,
    pub skills_loaded: bool,
    pub selected_skill: Option<Skill>,
    pub tasks: Vec<TaskPickerItem>,
    pub provider_items: Vec<ProviderPickerItem>,
    pub model_items: Vec<ModelPickerItem>,
    pub available_models: Vec<ModelMetadataDTO>,
    pub pending_session: Option<PendingSessionState>,
    // Conversation cells are rebuilt from persisted session messages and should stay stable.
    pub conversation_cells: Vec<TranscriptCell>,
    // Runtime cells are ephemeral UI feedback for the current turn only.
    pub runtime_cells: Vec<AnchoredRuntimeCell>,
    // Activity is a transient live panel for the current foreground turn.
    pub activity: ActivityState,
    // Background work status is rendered outside the transcript message panel.
    pub background_work: BackgroundWorkStatus,
    // Active turn is the latest live viewport. Stable history comes from session projection only.
    pub active_turn: Option<ActiveTurn>,
    active_turn_session_id: Option<String>,
    active_progress_started_at_ms: Option<i64>,
    active_tool_progress_started_at_ms: HashMap<String, i64>,
    active_assistant_stream_body: String,
    active_tool_call_ids: HashSet<String>,
    active_tool_result_ids: HashSet<String>,
    canceled_stream_ids: HashSet<String>,
    ignore_stream_frames: bool,
    last_error_notice: Option<String>,
    pub overlay: Option<OverlayState>,
    pub composer: ComposerState,
    pub input_mode: InputMode,
    pub message_scroll_from_bottom: usize,
    pub status: String,
    pub is_streaming: bool,
    pub current_stream_id: Option<String>,
    pub startup: Option<StartupState>,
    pending_session_delete_id: Option<String>,
    pending_initial_message: Option<String>,
}

impl AppState {
    pub fn empty() -> Self {
        Self {
            default_agent_name: None,
            default_agent_id: None,
            thread: SessionThreadState::default(),
            sessions: Vec::new(),
            skills: Vec::new(),
            skills_loaded: false,
            selected_skill: None,
            tasks: Vec::new(),
            provider_items: Vec::new(),
            model_items: Vec::new(),
            available_models: Vec::new(),
            pending_session: None,
            conversation_cells: Vec::new(),
            runtime_cells: Vec::new(),
            activity: ActivityState::default(),
            background_work: BackgroundWorkStatus::default(),
            active_turn: None,
            active_turn_session_id: None,
            active_progress_started_at_ms: None,
            active_tool_progress_started_at_ms: HashMap::new(),
            active_assistant_stream_body: String::new(),
            active_tool_call_ids: HashSet::new(),
            active_tool_result_ids: HashSet::new(),
            canceled_stream_ids: HashSet::new(),
            ignore_stream_frames: false,
            last_error_notice: None,
            overlay: None,
            composer: ComposerState::default(),
            input_mode: InputMode::default(),
            message_scroll_from_bottom: 0,
            status: "Connecting to daemon...".to_string(),
            is_streaming: false,
            current_stream_id: None,
            startup: None,
            pending_session_delete_id: None,
            pending_initial_message: None,
        }
    }

    pub fn current_session(&self) -> Option<&ChatSession> {
        self.thread.session.as_ref()
    }

    pub fn current_session_id(&self) -> Option<&str> {
        self.thread.session_id()
    }

    pub fn active_refresh_session_id(&self) -> Option<&str> {
        self.active_turn_session_id
            .as_deref()
            .or_else(|| self.current_session_id())
    }

    pub fn active_turn_has_tool_call(&self) -> bool {
        self.active_turn
            .as_ref()
            .is_some_and(|turn| turn.cells.iter().any(|cell| cell.tool_call_id().is_some()))
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

    pub fn cycle_input_mode(&mut self) {
        self.input_mode = self.input_mode.next();
        self.status = format!("{} mode", self.input_mode.label());
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
        self.thread.set_session(session.clone());
        self.pending_session = None;
        self.runtime_cells.clear();
        self.activity.clear();
        self.background_work.clear();
        self.clear_active_response();
        self.reset_message_scroll();
        self.conversation_cells =
            transcript_cells(&messages_from_session(&session), self.assistant_name());
    }

    pub fn refresh_current_session(&mut self, session: ChatSession) {
        let current_turn_finished = self.current_stream_finished_in_session(&session)
            || self.active_turn_finished_in_session(&session)
            || self.active_turn_answered_in_session_messages(&session)
            || (!self.is_streaming
                && (self.active_turn_projected_in_session(&session)
                    || self.pending_user_turn_persisted_in_session(&session)));
        if current_turn_finished {
            self.is_streaming = false;
            if let Some(stream_id) = self.current_stream_id.take() {
                self.canceled_stream_ids.insert(stream_id);
            }
            self.ignore_stream_frames = true;
            self.clear_active_response();
        }
        let old_cells = std::mem::take(&mut self.conversation_cells);
        self.thread.session = Some(session.clone());
        self.replace_session_projection(messages_from_session(&session));
        self.reanchor_runtime_cells(&old_cells);
        self.reconcile_runtime_conversation_cells();
    }

    fn current_stream_finished_in_session(&self, session: &ChatSession) -> bool {
        let Some(stream_id) = self.current_stream_id.as_deref() else {
            return false;
        };
        session.turns.iter().any(|turn| {
            turn.id == stream_id
                && matches!(
                    turn.status,
                    ChatTurnStatus::Completed | ChatTurnStatus::Canceled | ChatTurnStatus::Failed
                )
        })
    }

    fn active_turn_finished_in_session(&self, session: &ChatSession) -> bool {
        let Some(active_turn) = self.active_turn.as_ref() else {
            return false;
        };
        let active_tool_call_ids: Vec<&str> = active_turn
            .cells
            .iter()
            .filter_map(|cell| cell.tool_call_id())
            .collect();
        let Some(active_user) = active_turn
            .cells
            .iter()
            .find(|cell| cell.kind == TranscriptCellKind::User)
            .map(|cell| cell.body.trim_end())
        else {
            return false;
        };
        session.turns.iter().rev().any(|turn| {
            if !matches!(
                turn.status,
                ChatTurnStatus::Completed | ChatTurnStatus::Canceled | ChatTurnStatus::Failed
            ) {
                return false;
            }
            if active_tool_call_ids.is_empty() {
                return turn.events.iter().any(|event| {
                    matches!(
                        &event.kind,
                        ChatTurnEventKind::UserMessage { content } if content.trim_end() == active_user
                    )
                });
            }
            turn.events.iter().any(|event| {
                matches!(
                    &event.kind,
                    ChatTurnEventKind::ToolCall { call_id, .. }
                        if active_tool_call_ids.contains(&call_id.as_str())
                )
            })
        })
    }

    fn active_turn_projected_in_session(&self, session: &ChatSession) -> bool {
        let Some(active_turn) = self.active_turn.as_ref() else {
            return false;
        };
        if active_turn.cells.len() <= 1 {
            return false;
        }
        let persisted_cells =
            transcript_cells(&messages_from_session(session), self.assistant_name());
        active_turn.cells.iter().all(|active| {
            persisted_cells
                .iter()
                .any(|persisted| active_cell_projected_by(active, persisted))
        })
    }

    fn active_turn_answered_in_session_messages(&self, session: &ChatSession) -> bool {
        let Some(active_turn) = self.active_turn.as_ref() else {
            return false;
        };
        if active_turn
            .cells
            .iter()
            .any(|cell| cell.tool_call_id().is_some())
        {
            return false;
        }
        let Some(active_user) = active_turn
            .cells
            .iter()
            .find(|cell| cell.kind == TranscriptCellKind::User)
            .map(|cell| cell.body.trim_end())
        else {
            return false;
        };
        let Some(last_user_index) = session
            .messages
            .iter()
            .rposition(|message| message.role == ChatRole::User)
        else {
            return false;
        };
        session.messages[last_user_index].content.trim_end() == active_user
            && session.messages[last_user_index + 1..]
                .iter()
                .any(|message| message.role == ChatRole::Assistant)
    }

    fn pending_user_turn_persisted_in_session(&self, session: &ChatSession) -> bool {
        let Some(active_turn) = self.active_turn.as_ref() else {
            return false;
        };
        if active_turn.cells.len() != 1 {
            return false;
        }
        let Some(active_user) = active_turn
            .cells
            .iter()
            .find(|cell| cell.kind == TranscriptCellKind::User)
            .map(|cell| cell.body.trim_end())
        else {
            return false;
        };
        messages_from_session(session).iter().any(|message| {
            matches!(
                message,
                ShellMessage::UserMessage { content } if content.trim_end() == active_user
            )
        })
    }

    pub fn clear_current_session(&mut self, notice: impl Into<String>) {
        self.thread.clear_session();
        self.replace_session_projection(Vec::new());
        self.runtime_cells.clear();
        self.activity.clear();
        self.background_work.clear();
        self.clear_active_response();
        self.reset_message_scroll();
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
                        ModelId::Gpt5_4.as_serialized_str().to_string(),
                    )
                })
            });

        self.thread.clear_session();
        self.conversation_cells.clear();
        self.runtime_cells.clear();
        self.activity.clear();
        self.background_work.clear();
        self.clear_active_response();
        self.reset_message_scroll();
        self.pending_session = pending_session;
        self.clear_overlay();
        self.composer.clear();
        self.is_streaming = false;
    }

    pub fn set_run_focus(
        &mut self,
        run_id: String,
        thread: ExecutionThread,
        child_runs: Vec<RunSummary>,
    ) {
        self.activity.clear();
        self.thread.set_run_focus(run_id, thread, child_runs);
        let active_run_id = self.current_stream_id.clone();
        self.activity
            .sync_child_runs(&self.thread.child_runs, active_run_id.as_deref());
    }

    pub fn clear_overlay(&mut self) {
        self.overlay = None;
        self.pending_session_delete_id = None;
        self.selected_skill = None;
    }

    #[allow(dead_code)]
    pub fn open_session_picker(&mut self) {
        self.overlay = Some(OverlayState::SessionPicker { selected: 0 });
    }

    pub fn open_skill_manager(&mut self) {
        self.selected_skill = None;
        self.overlay = Some(OverlayState::SkillManager { selected: 0 });
    }

    pub fn open_skill_mention_picker(&mut self) {
        self.selected_skill = None;
        self.overlay = Some(OverlayState::SkillMentionPicker { selected: 0 });
    }

    pub fn open_skill_detail(&mut self, skill: Skill) {
        self.selected_skill = Some(skill);
        self.overlay = Some(OverlayState::SkillDetail);
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
            | Some(OverlayState::SkillManager { selected })
            | Some(OverlayState::SkillMentionPicker { selected })
            | Some(OverlayState::TaskPicker { selected })
            | Some(OverlayState::TaskActionPicker { selected, .. })
            | Some(OverlayState::ProviderPicker { selected })
            | Some(OverlayState::ModelPicker { selected, .. })
            | Some(OverlayState::RunPicker { selected }) => {
                let next = (*selected as isize + delta).clamp(0, len.saturating_sub(1) as isize);
                *selected = next as usize;
            }
            Some(OverlayState::SkillDetail)
            | Some(OverlayState::RunDetail)
            | Some(OverlayState::Help)
            | None => {}
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

    pub fn overlay_item_len(&self) -> Option<usize> {
        match self.overlay.as_ref()? {
            OverlayState::CommandPicker { .. } => {
                Some(super::slash_command::SLASH_COMMAND_SPECS.len())
            }
            OverlayState::DaemonPicker { .. } => Some(2),
            OverlayState::SessionPicker { .. } => Some(self.sessions.len()),
            OverlayState::SkillManager { .. } => Some(self.skills.len()),
            OverlayState::SkillMentionPicker { .. } => Some(self.skill_mention_matches().len()),
            OverlayState::SkillDetail => None,
            OverlayState::TaskPicker { .. } => Some(self.tasks.len()),
            OverlayState::TaskActionPicker { .. } => Some(3),
            OverlayState::ProviderPicker { .. } => Some(self.provider_items.len()),
            OverlayState::ModelPicker { .. } => Some(self.model_items.len()),
            OverlayState::RunPicker { .. } => Some(self.run_picker_items().len()),
            OverlayState::RunDetail => None,
            OverlayState::Help => None,
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

    pub fn selected_skill_manager_item(&self) -> Option<SkillManagerSelection> {
        match self.overlay.as_ref() {
            Some(OverlayState::SkillManager { selected }) => self
                .skills
                .get(*selected)
                .cloned()
                .map(SkillManagerSelection::Skill),
            _ => None,
        }
    }

    pub fn selected_skill_mention_item(&self) -> Option<SkillPickerItem> {
        let Some(OverlayState::SkillMentionPicker { selected }) = self.overlay.as_ref() else {
            return None;
        };
        self.skill_mention_matches().get(*selected).cloned()
    }

    pub fn sync_skill_mention_picker_to_draft(&mut self) {
        if !matches!(self.overlay, Some(OverlayState::SkillMentionPicker { .. })) {
            return;
        }
        if self.composer.current_skill_mention_query().is_none() {
            self.clear_overlay();
            return;
        }
        let len = self.skill_mention_matches().len();
        if len == 0 {
            return;
        }
        if let Some(OverlayState::SkillMentionPicker { selected }) = self.overlay.as_mut() {
            *selected = (*selected).min(len.saturating_sub(1));
        }
    }

    pub fn skill_mention_matches(&self) -> Vec<SkillPickerItem> {
        let Some(query) = self.composer.current_skill_mention_query() else {
            return Vec::new();
        };
        let query = query.to_lowercase();
        self.skills
            .iter()
            .filter(|skill| {
                query.is_empty()
                    || skill.id.to_lowercase().contains(&query)
                    || skill.name.to_lowercase().contains(&query)
                    || skill
                        .description
                        .as_deref()
                        .is_some_and(|description| description.to_lowercase().contains(&query))
            })
            .cloned()
            .collect()
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

    pub fn selected_run_picker_item(&self) -> Option<WorkPickerItem> {
        match self.overlay.as_ref() {
            Some(OverlayState::RunPicker { selected }) => {
                self.work_picker_items().get(*selected).cloned()
            }
            _ => None,
        }
    }

    pub fn work_picker_items(&self) -> Vec<WorkPickerItem> {
        build_work_picker_items(&self.tasks, &self.thread.runs, &self.thread.child_runs)
    }

    pub fn run_picker_items(&self) -> Vec<WorkPickerItem> {
        self.work_picker_items()
    }

    pub fn set_session_runs_and_child_runs(
        &mut self,
        runs: Vec<RunSummary>,
        child_runs: Vec<RunSummary>,
    ) {
        self.thread.set_session_runs(runs);
        self.thread.child_runs = child_runs;
        let active_run_id = self.current_stream_id.clone();
        self.activity
            .sync_child_runs(&self.thread.child_runs, active_run_id.as_deref());
    }

    pub fn clear_thread_runs(&mut self) {
        self.thread.runs.clear();
        self.thread.child_runs.clear();
        self.thread.execution_thread = None;
        self.activity.clear();
        self.background_work.clear();
    }

    #[allow(dead_code)]
    pub fn work_summary_notice(&self) -> Option<TranscriptCell> {
        self.active_turn.as_ref()?;
        let items = self.work_picker_items();
        if items.is_empty() {
            return None;
        }
        let active_task_ids = self.active_turn_task_ids();
        let active_turn_run_id = self.current_stream_id.as_deref();
        let active_items = items
            .into_iter()
            .filter(|item| is_active_turn_work_item(item, &active_task_ids, active_turn_run_id))
            .take(5)
            .collect::<Vec<_>>();
        if active_items.is_empty() {
            return None;
        }
        Some(TranscriptCell {
            kind: TranscriptCellKind::Notice,
            title: "Activity".to_string(),
            subtitle: Some("running".to_string()),
            body: active_work_notice_text(&active_items),
            group: MessageGroup::RuntimeNotice,
            is_active: true,
        })
    }

    fn active_turn_task_ids(&self) -> HashSet<String> {
        let mut ids = HashSet::new();
        let Some(active_turn) = self.active_turn.as_ref() else {
            return ids;
        };
        for cell in active_turn.cells.iter().filter(|cell| {
            cell.kind == TranscriptCellKind::Tool
                && cell
                    .title
                    .strip_prefix("Tool · ")
                    .is_some_and(|name| types::store::is_task_management_tool_name(name.trim()))
        }) {
            let input = extract_tool_json_payload(&cell.body, "Input:");
            let output = extract_tool_json_payload(&cell.body, "Output:");
            let operation = input
                .as_ref()
                .and_then(|value| value.get("operation"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if !task_operation_targets_background_work(operation) {
                continue;
            }
            if let Some(input) = input.as_ref() {
                collect_task_ids(input, &mut ids);
            }
            if let Some(output) = output.as_ref() {
                collect_task_ids(output, &mut ids);
            }
        }
        ids
    }
}

pub fn build_work_picker_items(
    tasks: &[TaskPickerItem],
    runs: &[RunSummary],
    child_runs: &[RunSummary],
) -> Vec<WorkPickerItem> {
    let mut items = Vec::new();
    items.extend(tasks.iter().map(|task| WorkPickerItem::BackgroundTask {
        task_id: task.task_id.clone(),
        title: task.name.clone(),
        status: task.status.clone(),
        next_run_at: task.next_run_at,
        latest_run_id: task.latest_run_id.clone(),
    }));
    items.extend(runs.iter().filter_map(|run| {
        run.run_id.as_ref().map(|run_id| WorkPickerItem::Run {
            run_id: run_id.clone(),
            root_run_id: run.root_run_id.clone(),
            parent_run_id: run.parent_run_id.clone(),
            kind: run.kind,
            title: run.title.clone(),
            status: run.status.clone(),
        })
    }));
    items.extend(child_runs.iter().filter_map(|run| {
        run.run_id.as_ref().map(|run_id| WorkPickerItem::Run {
            run_id: run_id.clone(),
            root_run_id: run.root_run_id.clone(),
            parent_run_id: run.parent_run_id.clone(),
            kind: run.kind,
            title: run.title.clone(),
            status: run.status.clone(),
        })
    }));
    items
}

#[allow(dead_code)]
fn active_work_notice_text(items: &[WorkPickerItem]) -> String {
    let mut lines = vec!["Current turn activity".to_string()];
    for item in items {
        match item {
            WorkPickerItem::BackgroundTask {
                task_id,
                title,
                status,
                latest_run_id,
                ..
            } => lines.push(format!(
                "- background task · {title} · {status} · {task_id}{}",
                latest_run_id
                    .as_ref()
                    .map(|run_id| format!(" · run {run_id}"))
                    .unwrap_or_default()
            )),
            WorkPickerItem::Run {
                run_id,
                root_run_id: _,
                parent_run_id: _,
                kind,
                title,
                status,
            } => {
                let label = match kind {
                    RunKind::WorkspaceRun => "workspace run",
                    RunKind::TaskRun => "background run",
                    RunKind::SubagentRun => "team",
                };
                lines.push(format!("- {label} · {title} · {status} · {run_id}"));
            }
        }
    }
    lines.join("\n")
}

#[allow(dead_code)]
fn is_active_turn_work_item(
    item: &WorkPickerItem,
    active_task_ids: &HashSet<String>,
    active_turn_run_id: Option<&str>,
) -> bool {
    match item {
        WorkPickerItem::BackgroundTask {
            task_id,
            status,
            latest_run_id,
            ..
        } => {
            matches_normalized_status(status, &["active", "running"])
                && (active_task_ids.contains(task_id)
                    || latest_run_id
                        .as_deref()
                        .is_some_and(|run_id| Some(run_id) == active_turn_run_id))
        }
        WorkPickerItem::Run {
            run_id,
            root_run_id,
            parent_run_id,
            kind,
            status,
            ..
        } => match kind {
            RunKind::WorkspaceRun => false,
            RunKind::TaskRun | RunKind::SubagentRun => {
                matches_normalized_status(status, &["running"])
                    && active_turn_run_id.is_some_and(|active_turn_run_id| {
                        run_id == active_turn_run_id
                            || root_run_id.as_deref() == Some(active_turn_run_id)
                            || parent_run_id.as_deref() == Some(active_turn_run_id)
                    })
            }
        },
    }
}

fn extract_tool_json_payload(body: &str, label: &str) -> Option<serde_json::Value> {
    let start = body.strip_prefix(label).map(|_| label.len()).or_else(|| {
        body.find(&format!("\n{label}"))
            .map(|index| index + 1 + label.len())
    })?;
    let tail = &body[start..];
    let end = ["\nInput:", "\nOutput:", "\nError:"]
        .iter()
        .filter_map(|marker| tail.find(marker))
        .min()
        .unwrap_or(tail.len());
    let payload = tail[..end].trim();
    serde_json::from_str(payload).ok()
}

fn task_operation_targets_background_work(operation: &str) -> bool {
    matches!(
        operation,
        "create"
            | "convert_session"
            | "promote_to_background"
            | "update"
            | "delete"
            | "run_batch"
            | "control"
            | "send_message"
            | "pause"
            | "start"
            | "resume"
            | "stop"
            | "run"
    )
}

fn collect_task_ids(value: &serde_json::Value, ids: &mut HashSet<String>) {
    match value {
        serde_json::Value::Object(object) => {
            for key in ["id", "task_id"] {
                if let Some(id) = object.get(key).and_then(serde_json::Value::as_str)
                    && !id.trim().is_empty()
                {
                    ids.insert(id.trim().to_string());
                }
            }
            for key in ["result", "task", "tasks"] {
                if let Some(value) = object.get(key) {
                    collect_task_ids(value, ids);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_task_ids(item, ids);
            }
        }
        _ => {}
    }
}

#[allow(dead_code)]
fn matches_normalized_status(status: &str, values: &[&str]) -> bool {
    let normalized = status.trim().to_ascii_lowercase();
    values.iter().any(|value| normalized == *value)
}

pub fn work_run_kind_label(kind: RunKind) -> &'static str {
    match kind {
        RunKind::WorkspaceRun => "workspace run",
        RunKind::TaskRun => "task run",
        RunKind::SubagentRun => "subagent run",
    }
}

impl AppState {
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

    fn clear_active_response(&mut self) {
        self.active_turn = None;
        self.active_turn_session_id = None;
        self.active_progress_started_at_ms = None;
        self.active_tool_progress_started_at_ms.clear();
        self.activity.clear();
        self.active_assistant_stream_body.clear();
        self.active_tool_call_ids.clear();
        self.active_tool_result_ids.clear();
    }

    fn finish_active_assistant_segment(&mut self) {
        let Some(active_turn) = self.active_turn.as_mut() else {
            return;
        };
        let Some(index) = active_turn.active_assistant_index.take() else {
            return;
        };
        let Some(active_cell) = active_turn.cells.get_mut(index) else {
            return;
        };
        active_cell.body = active_cell.body.trim_end().to_string();
        if active_cell.body.trim().is_empty() {
            active_turn.cells.remove(index);
        } else {
            let _ = active_cell.finalize();
        }
        if active_turn.cells.is_empty() {
            self.active_turn = None;
        }
        self.active_progress_started_at_ms = None;
        self.active_assistant_stream_body.clear();
    }

    pub fn start_assistant_typing(&mut self) {
        self.ensure_active_assistant_cell();
        if self.active_progress_started_at_ms.is_none() {
            self.active_progress_started_at_ms = Some(Utc::now().timestamp_millis());
        }
        let _ = self.update_active_progress_indicator();
    }

    pub fn begin_stream(&mut self, stream_id: String) {
        self.canceled_stream_ids.remove(&stream_id);
        self.ignore_stream_frames = false;
        self.is_streaming = true;
        self.current_stream_id = Some(stream_id);
    }

    pub fn cancel_active_response(&mut self) {
        self.flush_active_turn_to_runtime();
        self.is_streaming = false;
        if let Some(stream_id) = self.current_stream_id.take() {
            self.canceled_stream_ids.insert(stream_id);
        }
        self.ignore_stream_frames = true;
    }

    pub fn update_active_typing_indicator(&mut self) -> bool {
        self.update_active_progress_indicator()
    }

    pub fn update_active_progress_indicator(&mut self) -> bool {
        self.update_active_progress_indicator_at(Utc::now().timestamp_millis())
    }

    fn update_active_progress_indicator_at(&mut self, now_ms: i64) -> bool {
        let assistant_started_at = &mut self.active_progress_started_at_ms;
        let tool_started_at = &mut self.active_tool_progress_started_at_ms;
        let Some(active_turn) = self.active_turn.as_mut() else {
            return false;
        };
        let mut changed = false;
        for active_cell in active_turn.cells.iter_mut().filter(|cell| cell.is_active) {
            let label = match active_cell.kind {
                TranscriptCellKind::Tool | TranscriptCellKind::Subagent => "running",
                _ => "typing",
            };
            let started_at = if matches!(
                active_cell.kind,
                TranscriptCellKind::Tool | TranscriptCellKind::Subagent
            ) {
                active_cell
                    .tool_call_id()
                    .map(|call_id| *tool_started_at.entry(call_id.to_string()).or_insert(now_ms))
                    .unwrap_or_else(|| *assistant_started_at.get_or_insert(now_ms))
            } else {
                *assistant_started_at.get_or_insert(now_ms)
            };
            let elapsed_ms = now_ms.saturating_sub(started_at);
            let elapsed_secs = elapsed_ms / 1000;
            let frame = match (elapsed_ms / 250) % 4 {
                0 => label.to_string(),
                1 => format!("{label}."),
                2 => format!("{label}.."),
                _ => format!("{label}..."),
            };
            let progress = format!("{frame:<10} {elapsed_secs}s");
            let next = if matches!(
                active_cell.kind,
                TranscriptCellKind::Tool | TranscriptCellKind::Subagent
            ) {
                active_cell
                    .tool_call_id()
                    .map(|call_id| format!("#{call_id} · {progress}"))
                    .unwrap_or(progress)
            } else {
                progress
            };
            if active_cell.subtitle.as_deref() == Some(next.as_str()) {
                continue;
            }
            active_cell.subtitle = Some(next);
            changed = true;
        }
        changed
    }

    fn push_tool_message(&mut self, message: ShellMessage) {
        if self.active_turn.is_some() || self.is_streaming {
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

    fn ensure_active_turn(&mut self) -> &mut ActiveTurn {
        if self.active_turn_session_id.is_none() {
            self.active_turn_session_id = self.current_session_id().map(ToOwned::to_owned);
        }
        self.active_turn.get_or_insert_with(ActiveTurn::default)
    }

    fn active_assistant_cell_mut(&mut self) -> Option<&mut TranscriptCell> {
        let active_turn = self.active_turn.as_mut()?;
        let index = active_turn.active_assistant_index?;
        active_turn.cells.get_mut(index)
    }

    fn ensure_active_assistant_cell(&mut self) {
        let needs_cell = self
            .active_turn
            .as_ref()
            .and_then(|turn| turn.active_assistant_index)
            .is_none();
        if needs_cell {
            let assistant_name = self.assistant_name().to_string();
            let cell = cell_from_message(
                &ShellMessage::AssistantStream {
                    content: String::new(),
                },
                &assistant_name,
            );
            let active_turn = self.ensure_active_turn();
            let index = active_turn.cells.len();
            active_turn.cells.push(cell);
            active_turn.active_assistant_index = Some(index);
        }
        if self.active_progress_started_at_ms.is_none() {
            self.active_progress_started_at_ms = Some(Utc::now().timestamp_millis());
        }
        let _ = self.update_active_progress_indicator();
    }

    fn append_tool_call_to_active(&mut self, call_id: &str, name: &str, arguments: &str) {
        if !self.active_tool_call_ids.insert(call_id.to_string()) {
            return;
        }
        self.finish_active_assistant_segment();
        let assistant_name = self.assistant_name().to_string();
        let mut cell = cell_from_message(
            &ShellMessage::ToolCall {
                call_id: call_id.to_string(),
                name: name.to_string(),
                arguments: arguments.to_string(),
            },
            &assistant_name,
        );
        cell.is_active = true;
        self.activity.record_tool_call(call_id, name, &cell.body);
        self.active_tool_progress_started_at_ms
            .entry(call_id.to_string())
            .or_insert_with(|| Utc::now().timestamp_millis());
        self.ensure_active_turn().cells.push(cell);
        let _ = self.update_active_progress_indicator();
    }

    fn append_tool_result_to_active(&mut self, call_id: &str, success: bool, result: &str) {
        if !self.active_tool_result_ids.insert(call_id.to_string()) {
            return;
        }
        if let Some(active_turn) = self.active_turn.as_mut()
            && let Some(cell) = active_turn
                .cells
                .iter_mut()
                .find(|cell| cell.tool_call_id() == Some(call_id))
        {
            let _ = cell.merge_tool_result(success, result);
            self.activity
                .record_tool_result(call_id, success, &cell.body);
            self.active_tool_progress_started_at_ms.remove(call_id);
            if !active_turn.cells.iter().any(|cell| cell.is_active) {
                self.active_progress_started_at_ms = None;
            }
            return;
        }
        let assistant_name = self.assistant_name().to_string();
        let cell = cell_from_message(
            &ShellMessage::ToolResult {
                call_id: call_id.to_string(),
                success,
                result: result.to_string(),
            },
            &assistant_name,
        );
        self.activity
            .record_tool_result(call_id, success, &cell.body);
        self.ensure_active_turn().cells.push(cell);
    }

    pub fn push_local_user_message(&mut self, content: String) {
        self.reset_message_scroll();
        self.last_error_notice = None;
        self.background_work.clear_terminal_entries();
        self.flush_active_turn_to_runtime();
        let cell = cell_from_message(
            &ShellMessage::UserMessage { content },
            self.assistant_name(),
        );
        self.active_turn_session_id = self.current_session_id().map(ToOwned::to_owned);
        self.active_turn = Some(ActiveTurn {
            cells: vec![cell],
            queued_updates: Vec::new(),
            active_assistant_index: None,
        });
    }

    pub fn queue_active_turn_update(&mut self, instruction: String) {
        self.ensure_active_turn().queued_updates.push(instruction);
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
        let content = content.into();
        let normalized = normalized_error_notice(&content).to_string();
        if self.last_error_notice.as_deref() == Some(normalized.as_str()) {
            return;
        }
        if self.last_runtime_error_matches(&content) {
            self.last_error_notice = Some(normalized);
            return;
        }
        self.last_error_notice = Some(normalized);
        self.push_message(ShellMessage::ErrorNotice { content });
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

    fn append_assistant_stream_chunk(&mut self, chunk: &str) {
        if chunk.is_empty() {
            return;
        }

        if self.active_progress_started_at_ms.is_none() {
            self.active_progress_started_at_ms = Some(Utc::now().timestamp_millis());
        }

        let Some(delta) = assistant_stream_delta(&mut self.active_assistant_stream_body, chunk)
        else {
            return;
        };

        if delta.trim().is_empty() {
            return;
        }

        self.ensure_active_assistant_cell();
        if let Some(active_cell) = self.active_assistant_cell_mut() {
            append_active_text(&mut active_cell.body, &delta);
        }
        let _ = self.update_active_progress_indicator();
    }

    pub fn apply_stream_frame(&mut self, frame: StreamFrame) -> bool {
        match frame {
            StreamFrame::Start { stream_id } => {
                if self.canceled_stream_ids.remove(&stream_id) {
                    self.ignore_stream_frames = true;
                    return false;
                }
                self.ignore_stream_frames = false;
                self.is_streaming = true;
                self.current_stream_id = Some(stream_id.clone());
                self.status = format!("Streaming response ({stream_id})");
            }
            StreamFrame::Ack { content } => {
                if self.ignore_stream_frames {
                    return false;
                }
                self.is_streaming = true;
                self.append_assistant_stream_chunk(&content);
            }
            StreamFrame::Data { content } => {
                if self.ignore_stream_frames {
                    return false;
                }
                self.is_streaming = true;
                self.append_assistant_stream_chunk(&content);
            }
            StreamFrame::Done { total_tokens } => {
                if self.ignore_stream_frames {
                    return false;
                }
                let should_flush_completed_turn = self.thread.session.is_none();
                self.is_streaming = false;
                self.current_stream_id = None;
                self.finish_active_assistant_segment();
                if should_flush_completed_turn {
                    self.flush_active_turn_to_runtime();
                }
                self.status = match total_tokens {
                    Some(total_tokens) => format!("Stream finished ({total_tokens} tokens)"),
                    None => "Stream finished".to_string(),
                };
            }
            other => {
                if self.ignore_stream_frames {
                    return false;
                }
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
        true
    }

    pub fn apply_session_event(&mut self, event: ChatSessionEvent) {
        if let Some(message) = message_from_session_event(&event) {
            self.push_message(message);
        }
    }

    pub fn apply_task_event(&mut self, event: TaskStreamEvent) {
        let message = message_from_task_event(&event);
        let is_terminal = matches!(
            event.kind,
            StreamEventKind::Completed { .. }
                | StreamEventKind::Failed { .. }
                | StreamEventKind::Interrupted { .. }
        );
        if is_terminal {
            self.background_work.record_task_event(
                &event,
                match &message {
                    ShellMessage::TaskNotice { content } => content.clone(),
                    _ => String::new(),
                },
            );
            if let ShellMessage::TaskNotice { content } = message {
                self.push_message(ShellMessage::TaskNotice {
                    content: content.clone(),
                });
                self.status = content;
            }
            return;
        }
        if self.active_turn.is_none() && !self.is_streaming {
            if let ShellMessage::TaskNotice { content } = message {
                self.background_work
                    .record_task_event(&event, content.clone());
                self.status = content;
            }
            return;
        }
        if let ShellMessage::TaskNotice { content } = message {
            self.background_work.record_task_event(&event, content);
        }
    }

    #[allow(dead_code)]
    pub fn transcript_cells_for_render(&self) -> Vec<TranscriptCell> {
        let mut cells = Vec::with_capacity(
            self.conversation_cells.len()
                + self.runtime_cells.len()
                + self
                    .active_turn
                    .as_ref()
                    .map(|turn| turn.cells.len())
                    .unwrap_or_default(),
        );

        let mut runtime = self.runtime_cells.iter().peekable();
        for (index, cell) in self.conversation_cells.iter().enumerate() {
            if runtime.peek().is_some_and(|entry| {
                entry.base_cell_index == index && entry.cell.kind != TranscriptCellKind::User
            }) && cell.kind == TranscriptCellKind::User
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

        for entry in runtime {
            cells.push(entry.cell.clone());
        }
        if let Some(active_turn) = self.active_turn.as_ref() {
            cells.extend(active_turn.cells.iter().cloned());
        }
        cells
    }

    fn flush_active_turn_to_runtime(&mut self) {
        let Some(mut active_turn) = self.active_turn.take() else {
            self.clear_active_response();
            return;
        };
        let base_cell_index = self.conversation_cells.len();
        for mut cell in active_turn.cells.drain(..) {
            if cell.is_active {
                let _ = cell.finalize();
            }
            if matches!(
                cell.kind,
                TranscriptCellKind::Assistant | TranscriptCellKind::User
            ) && cell.body.trim().is_empty()
            {
                continue;
            }
            if self
                .conversation_cells
                .iter()
                .any(|persisted| is_persisted_duplicate_cell(&cell, persisted))
            {
                continue;
            }
            self.runtime_cells.push(AnchoredRuntimeCell {
                base_cell_index,
                cell,
            });
        }
        self.clear_active_response();
    }

    fn reconcile_runtime_conversation_cells(&mut self) {
        self.runtime_cells.retain(|entry| {
            let persisted_duplicate = self
                .conversation_cells
                .iter()
                .any(|cell| is_persisted_duplicate_cell(&entry.cell, cell));
            let equivalent_error = entry.cell.title == "Error"
                && self.conversation_cells.iter().any(|cell| {
                    cell.title == "Error"
                        && normalized_error_notice(&cell.body)
                            == normalized_error_notice(&entry.cell.body)
                });
            !persisted_duplicate && !equivalent_error
        });
    }

    fn reanchor_runtime_cells(&mut self, old_cells: &[TranscriptCell]) {
        for entry in self.runtime_cells.iter_mut() {
            // base_cell_index == old_cells.len() means the runtime cell was anchored
            // past the end (after the last conversation cell). Keep it at the new
            // end once there was a real old anchor; if the old projection was empty,
            // local failed-turn cells must stay before the first persisted messages.
            if entry.base_cell_index == old_cells.len() && !old_cells.is_empty() {
                entry.base_cell_index = self.conversation_cells.len();
            } else if let Some(old_cell) = old_cells.get(entry.base_cell_index)
                && let Some(new_index) = self.conversation_cells.iter().position(|c| c == old_cell)
            {
                entry.base_cell_index = new_index;
            }
        }
    }

    fn assistant_name(&self) -> &str {
        self.default_agent_name.as_deref().unwrap_or("Agent")
    }

    fn last_runtime_error_matches(&self, content: &str) -> bool {
        let normalized = normalized_error_notice(content);
        self.runtime_cells.iter().rev().any(|entry| {
            entry.cell.title == "Error" && normalized_error_notice(&entry.cell.body) == normalized
        })
    }
}

fn is_persisted_duplicate_cell(runtime: &TranscriptCell, persisted: &TranscriptCell) -> bool {
    if runtime.kind != persisted.kind {
        return false;
    }
    if runtime.body == persisted.body {
        return true;
    }
    if matches!(
        runtime.kind,
        TranscriptCellKind::Tool | TranscriptCellKind::Subagent
    ) && let Some(runtime_call_id) = runtime.tool_call_id()
    {
        return persisted.tool_call_id() == Some(runtime_call_id);
    }
    matches!(
        runtime.kind,
        TranscriptCellKind::User | TranscriptCellKind::Assistant
    ) && compact_duplicate_text(&runtime.body) == compact_duplicate_text(&persisted.body)
}

fn active_cell_projected_by(active: &TranscriptCell, persisted: &TranscriptCell) -> bool {
    if is_persisted_duplicate_cell(active, persisted) {
        return true;
    }
    if active.kind != TranscriptCellKind::Assistant
        || persisted.kind != TranscriptCellKind::Assistant
    {
        return false;
    }
    let active_text = compact_duplicate_text(&active.body);
    if active_text.is_empty() {
        return false;
    }
    compact_duplicate_text(&persisted.body).starts_with(&active_text)
}

fn compact_duplicate_text(content: &str) -> String {
    content.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn assistant_stream_delta(current: &mut String, chunk: &str) -> Option<String> {
    let normalized = chunk.trim_start_matches(['\r', '\n']);
    if chunk == current
        || normalized == current
        || (!normalized.trim().is_empty() && normalized.trim() == current.trim())
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

fn normalized_error_notice(content: &str) -> &str {
    let trimmed = content.trim();
    let Some(rest) = trimmed.strip_prefix("Stream error ") else {
        return trimmed;
    };
    let Some((_, message)) = rest.split_once(": ") else {
        return trimmed;
    };
    message.trim()
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
    use super::{AppState, OverlayState, TaskPickerItem};
    use crate::transcript::TranscriptCellKind;
    use types::{ChatSessionEvent, StreamFrame};

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
        assert!(state.active_turn.is_some());
        state.apply_stream_frame(StreamFrame::Done { total_tokens: None });

        assert!(state.conversation_cells.is_empty());
        assert!(state.active_turn.is_none());
        assert_eq!(state.runtime_cells.len(), 1);
        let active = &state.runtime_cells[0].cell;
        assert_eq!(active.kind, TranscriptCellKind::Assistant);
        assert_eq!(active.body, "hello");
    }

    #[test]
    fn stream_frames_preserve_text_split_across_repeated_prefix_boundary() {
        let mut state = AppState::empty();
        state.push_local_user_message("hello".to_string());
        state.apply_stream_frame(StreamFrame::Data {
            content: "STREAM_CANCEL_TEST_1\nST".to_string(),
        });
        state.apply_stream_frame(StreamFrame::Data {
            content: "REAM_CANCEL_TEST_2\nST".to_string(),
        });
        state.apply_stream_frame(StreamFrame::Data {
            content: "REAM_CANCEL_TEST_3".to_string(),
        });

        let cells = state.transcript_cells_for_render();
        assert_eq!(cells[1].kind, TranscriptCellKind::Assistant);
        assert_eq!(
            cells[1].body,
            "STREAM_CANCEL_TEST_1\nSTREAM_CANCEL_TEST_2\nSTREAM_CANCEL_TEST_3"
        );
    }

    #[test]
    fn stream_frames_preserve_newline_prefix_delta_matching_existing_prefix() {
        let mut state = AppState::empty();
        state.push_local_user_message("hello".to_string());
        state.apply_stream_frame(StreamFrame::Data {
            content: "EXACT_CANCEL_LINE_001".to_string(),
        });
        state.apply_stream_frame(StreamFrame::Data {
            content: "\nEX".to_string(),
        });
        state.apply_stream_frame(StreamFrame::Data {
            content: "ACT_CANCEL_LINE_002".to_string(),
        });

        let cells = state.transcript_cells_for_render();
        assert_eq!(cells[1].kind, TranscriptCellKind::Assistant);
        assert_eq!(
            cells[1].body,
            "EXACT_CANCEL_LINE_001\nEXACT_CANCEL_LINE_002"
        );
    }

    #[test]
    fn canceled_stream_ignores_late_frames() {
        let mut state = AppState::empty();
        state.push_local_user_message("hi".to_string());
        state.begin_stream("stream-1".to_string());
        state.apply_stream_frame(StreamFrame::Ack {
            content: "hel".to_string(),
        });

        state.cancel_active_response();

        assert!(!state.apply_stream_frame(StreamFrame::Start {
            stream_id: "stream-1".to_string(),
        }));
        assert!(!state.apply_stream_frame(StreamFrame::Data {
            content: "lo".to_string(),
        }));
        assert!(!state.apply_stream_frame(StreamFrame::Done { total_tokens: None }));

        let cells = state.transcript_cells_for_render();
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].kind, TranscriptCellKind::User);
        assert_eq!(cells[1].kind, TranscriptCellKind::Assistant);
        assert_eq!(cells[1].body, "hel");
        assert!(!state.is_streaming);
        assert!(state.current_stream_id.is_none());
    }

    #[test]
    fn active_progress_indicator_animates_with_elapsed_time() {
        let mut state = AppState::empty();
        state.start_assistant_typing();
        let started_at = state.active_progress_started_at_ms.expect("typing start");

        state.update_active_progress_indicator_at(started_at);
        let initial = state
            .active_turn
            .as_ref()
            .and_then(|turn| turn.cells.last())
            .and_then(|cell| cell.subtitle.as_deref())
            .expect("subtitle")
            .to_string();
        assert_eq!(initial, "typing     0s");

        state.update_active_progress_indicator_at(started_at + 500);
        let animated = state
            .active_turn
            .as_ref()
            .and_then(|turn| turn.cells.last())
            .and_then(|cell| cell.subtitle.as_deref())
            .expect("subtitle")
            .to_string();
        assert_eq!(animated, "typing..   0s");

        state.update_active_progress_indicator_at(started_at + 1_250);
        let elapsed = state
            .active_turn
            .as_ref()
            .and_then(|turn| turn.cells.last())
            .and_then(|cell| cell.subtitle.as_deref())
            .expect("subtitle");
        assert_eq!(elapsed, "typing.    1s");
    }

    #[test]
    fn active_progress_indicator_updates_all_running_tool_cells() {
        let mut state = AppState::empty();
        state.apply_stream_frame(StreamFrame::Start {
            stream_id: "stream-1".to_string(),
        });
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-1".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({"command": "pwd"}),
        });
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-2".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({"command": "ls"}),
        });
        let started_at = *state
            .active_tool_progress_started_at_ms
            .get("call-1")
            .expect("tool start");

        state.update_active_progress_indicator_at(started_at + 500);

        let active_turn = state.active_turn.as_ref().expect("active turn");
        let tool_subtitles = active_turn
            .cells
            .iter()
            .filter(|cell| cell.kind == TranscriptCellKind::Tool)
            .map(|cell| cell.subtitle.as_deref().unwrap_or_default().to_string())
            .collect::<Vec<_>>();
        assert_eq!(tool_subtitles.len(), 2);
        assert!(tool_subtitles[0].contains("#call-1 · running.."));
        assert!(tool_subtitles[1].contains("#call-2 · running.."));
    }

    #[test]
    fn completing_one_tool_does_not_reset_remaining_tool_elapsed_time() {
        let mut state = AppState::empty();
        state.apply_stream_frame(StreamFrame::Start {
            stream_id: "stream-1".to_string(),
        });
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-1".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({"command": "pwd"}),
        });
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-2".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({"command": "ls"}),
        });
        let started_at = *state
            .active_tool_progress_started_at_ms
            .get("call-2")
            .expect("tool start");

        state.update_active_progress_indicator_at(started_at + 2_000);
        state.apply_stream_frame(StreamFrame::ToolResult {
            id: "call-1".to_string(),
            result: "{\"ok\":true}".to_string(),
            success: true,
        });
        state.update_active_progress_indicator_at(started_at + 2_500);

        let remaining_subtitle = state
            .active_turn
            .as_ref()
            .and_then(|turn| {
                turn.cells
                    .iter()
                    .find(|cell| cell.tool_call_id() == Some("call-2"))
            })
            .and_then(|cell| cell.subtitle.as_deref())
            .expect("remaining tool subtitle");
        assert!(remaining_subtitle.contains("#call-2 · running..  2s"));
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
        let active_turn = state.active_turn.as_ref().expect("active turn");
        let tool = active_turn.cells.last().expect("active tool");
        assert!(tool.is_active);
        assert!(
            tool.subtitle
                .as_deref()
                .is_some_and(|subtitle| subtitle.contains("running"))
        );
        state.apply_stream_frame(StreamFrame::ToolResult {
            id: "call-1".to_string(),
            result: "{\"cwd\":\"/tmp\"}".to_string(),
            success: true,
        });
        state.apply_stream_frame(StreamFrame::Data {
            content: "Done.".to_string(),
        });

        assert!(state.active_turn.is_some());
        assert!(state.conversation_cells.is_empty());
        assert!(state.runtime_cells.is_empty());
        let active_turn = state.active_turn.as_ref().expect("active turn");
        assert_eq!(active_turn.cells.len(), 3);
        let active = active_turn.cells.last().expect("active assistant");
        assert_eq!(active.kind, TranscriptCellKind::Assistant);
        assert!(active_turn.cells[0].body.contains("Checking..."));
        assert!(active.body.contains("Done."));
        assert_eq!(active_turn.cells[1].title, "Tool · bash");
        assert!(active_turn.cells[1].body.contains("Input:"));
        assert!(active_turn.cells[1].body.contains("Output:"));

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

        assert!(state.conversation_cells.is_empty());
        assert!(state.active_turn.is_none());
        assert_eq!(state.runtime_cells.len(), 3);
        assert!(
            !state.runtime_cells[0]
                .cell
                .body
                .contains("Tool · bash #call-1")
        );
    }

    #[test]
    fn stream_error_persists_partial_live_turn_before_error_notice() {
        let mut state = AppState::empty();
        state.apply_stream_frame(StreamFrame::Ack {
            content: "Partial answer".to_string(),
        });
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-1".to_string(),
            name: "web_search".to_string(),
            arguments: serde_json::json!({"query": "test"}),
        });
        state.apply_stream_frame(StreamFrame::ToolResult {
            id: "call-1".to_string(),
            result: "{\"ok\":true}".to_string(),
            success: true,
        });

        state.apply_stream_frame(StreamFrame::error(500, "stream failed"));

        assert!(state.active_turn.is_none());
        assert!(!state.is_streaming);
        assert!(state.conversation_cells.is_empty());
        assert_eq!(state.runtime_cells.len(), 3);
        assert!(state.runtime_cells[0].cell.body.contains("Partial answer"));
        assert_eq!(state.runtime_cells[1].cell.kind, TranscriptCellKind::Tool);
        assert_eq!(state.runtime_cells[2].cell.title, "Error");
        assert!(state.runtime_cells[2].cell.body.contains("stream failed"));
    }

    #[test]
    fn cancel_preserves_unfinished_tool_call_without_result() {
        let mut state = AppState::empty();
        state.push_local_user_message("run tool".to_string());
        state.apply_stream_frame(StreamFrame::Ack {
            content: "Checking".to_string(),
        });
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-1".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({"cmd": "sleep 10"}),
        });

        state.cancel_active_response();

        let rendered = state.transcript_cells_for_render();
        assert_eq!(
            rendered.iter().map(|cell| cell.kind).collect::<Vec<_>>(),
            vec![
                TranscriptCellKind::User,
                TranscriptCellKind::Assistant,
                TranscriptCellKind::Tool,
            ]
        );
        assert_eq!(rendered[2].title, "Tool · bash");
        assert_eq!(rendered[2].tool_call_id(), Some("call-1"));
        assert!(rendered[2].body.contains("sleep 10"));
        assert!(!rendered[2].is_active);
    }

    #[test]
    fn canceled_session_refresh_deduplicates_persisted_tool_call() {
        let mut state = AppState::empty();
        let mut session =
            runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session.clone());
        state.begin_stream("turn-1".to_string());
        state.push_local_user_message("run tool".to_string());
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-1".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({"cmd": "sleep 10"}),
        });
        state.cancel_active_response();
        state.push_info("Canceled current response.");

        session.record_turn_user_message("turn-1", "run tool");
        session.record_turn_event(
            "turn-1",
            runtime::models::ChatTurnEventKind::ToolCall {
                call_id: "call-1".to_string(),
                name: "bash".to_string(),
                arguments: "{\"cmd\":\"sleep 10\"}".to_string(),
            },
        );
        session.cancel_turn("turn-1");
        state.refresh_current_session(session);

        let tool_cells = state
            .transcript_cells_for_render()
            .into_iter()
            .filter(|cell| cell.tool_call_id() == Some("call-1"))
            .collect::<Vec<_>>();
        assert_eq!(tool_cells.len(), 1);
        assert!(!tool_cells[0].is_active);
    }

    #[test]
    fn cancel_flush_deduplicates_tool_call_already_projected_from_session() {
        let mut state = AppState::empty();
        let mut session =
            runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session.clone());
        state.begin_stream("turn-1".to_string());
        state.push_local_user_message("run tool".to_string());
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-1".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({"cmd": "sleep 10"}),
        });

        session.record_turn_user_message("turn-1", "run tool");
        session.record_turn_event(
            "turn-1",
            runtime::models::ChatTurnEventKind::ToolCall {
                call_id: "call-1".to_string(),
                name: "bash".to_string(),
                arguments: "{\"cmd\":\"sleep 10\"}".to_string(),
            },
        );
        state.refresh_current_session(session);

        state.cancel_active_response();

        let tool_cells = state
            .transcript_cells_for_render()
            .into_iter()
            .filter(|cell| cell.tool_call_id() == Some("call-1"))
            .collect::<Vec<_>>();
        assert_eq!(tool_cells.len(), 1);
        assert!(!tool_cells[0].is_active);
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
        assert!(
            state
                .active_turn
                .as_ref()
                .map(|turn| turn.cells.is_empty())
                .unwrap_or(true)
        );
    }

    #[test]
    fn work_picker_includes_tasks_runs_and_child_runs() {
        let mut state = AppState::empty();
        state.tasks.push(TaskPickerItem {
            task_id: "task-1".to_string(),
            name: "Daily digest".to_string(),
            status: "Active".to_string(),
            next_run_at: None,
            latest_run_id: Some("run-task-1".to_string()),
        });
        state.thread.runs.push(runtime::models::RunSummary {
            id: "run-local".to_string(),
            kind: runtime::models::RunKind::WorkspaceRun,
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
        state.thread.child_runs.push(runtime::models::RunSummary {
            id: "child-1".to_string(),
            kind: runtime::models::RunKind::SubagentRun,
            container_id: "session-1".to_string(),
            root_run_id: Some("run-local".to_string()),
            title: "Subagent run".to_string(),
            subtitle: None,
            status: "running".to_string(),
            updated_at: 2,
            started_at: Some(2),
            ended_at: None,
            session_id: Some("session-1".to_string()),
            run_id: Some("child-1".to_string()),
            task_id: None,
            parent_run_id: Some("run-local".to_string()),
            agent_id: Some("agent-2".to_string()),
            source_channel: None,
            source_conversation_id: None,
            effective_model: None,
            provider: None,
            event_count: 0,
        });

        let items = state.work_picker_items();
        assert_eq!(items.len(), 3);
        assert!(matches!(
            items[0],
            super::WorkPickerItem::BackgroundTask { .. }
        ));
        assert!(matches!(
            items[1],
            super::WorkPickerItem::Run {
                kind: runtime::models::RunKind::WorkspaceRun,
                ..
            }
        ));
        assert!(matches!(
            items[2],
            super::WorkPickerItem::Run {
                kind: runtime::models::RunKind::SubagentRun,
                ..
            }
        ));
    }

    #[test]
    fn active_turn_task_ids_extracts_manage_task_result_ids() {
        let mut state = AppState::empty();
        state.push_local_user_message("create a task".to_string());
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-task".to_string(),
            name: "manage_tasks".to_string(),
            arguments: serde_json::json!({"operation":"create","name":"Created task"}),
        });
        state.apply_stream_frame(StreamFrame::ToolResult {
            id: "call-task".to_string(),
            success: true,
            result: serde_json::json!({
                "status": "executed",
                "result": {
                    "id": "task-1"
                }
            })
            .to_string(),
        });

        let tool_body = &state.active_turn.as_ref().unwrap().cells[1].body;
        assert_eq!(
            super::extract_tool_json_payload(tool_body, "Input:")
                .and_then(|value| value.get("operation").cloned()),
            Some(serde_json::json!("create"))
        );
        assert_eq!(
            super::extract_tool_json_payload(tool_body, "Output:")
                .and_then(|value| value.get("result").cloned())
                .and_then(|value| value.get("id").cloned()),
            Some(serde_json::json!("task-1"))
        );
        assert!(state.active_turn_task_ids().contains("task-1"));
    }

    #[test]
    fn refresh_current_session_preserves_notice_messages() {
        let mut state = AppState::empty();
        let mut session =
            runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        session
            .messages
            .push(runtime::models::ChatMessage::user("hello"));
        state.set_current_session(session.clone());
        state.push_info("notice");

        let mut updated = session.clone();
        updated
            .messages
            .push(runtime::models::ChatMessage::assistant("hi"));
        state.refresh_current_session(updated);

        assert_eq!(state.conversation_cells.len(), 2);
        assert_eq!(state.runtime_cells.len(), 1);
        assert_eq!(state.runtime_cells[0].cell.title, "Info");
    }

    #[test]
    fn refresh_current_session_keeps_active_turn_until_stream_finishes() {
        let mut state = AppState::empty();
        let session = runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session.clone());
        state.push_local_user_message("hello".to_string());

        state.is_streaming = true;
        state.refresh_current_session(session.clone());
        assert_eq!(state.active_turn.as_ref().unwrap().cells.len(), 1);

        let mut updated = session;
        updated
            .messages
            .push(runtime::models::ChatMessage::user("hello"));
        state.is_streaming = false;
        state.refresh_current_session(updated);
        assert!(state.active_turn.is_none());
        assert_eq!(state.conversation_cells.len(), 1);
        assert_eq!(state.conversation_cells[0].body, "hello");
    }

    #[test]
    fn refresh_current_session_keeps_streaming_turn_when_user_is_only_persisted_event() {
        let mut state = AppState::empty();
        let mut session =
            runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session.clone());
        state.push_local_user_message("hello".to_string());
        state.is_streaming = true;

        session.record_turn_user_message("turn-1", "hello");
        state.refresh_current_session(session);

        assert!(state.is_streaming);
        assert!(state.active_turn.is_some());
        assert!(!state.ignore_stream_frames);
    }

    #[test]
    fn refresh_current_session_keeps_streaming_turn_when_tool_call_is_only_projected_event() {
        let mut state = AppState::empty();
        let mut session =
            runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session.clone());
        state.begin_stream("turn-1".to_string());
        state.push_local_user_message("coordinate team".to_string());
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-team".to_string(),
            name: "spawn_subagent_batch".to_string(),
            arguments: serde_json::json!({}),
        });
        assert!(state.is_streaming);

        session.record_turn_user_message("turn-1", "coordinate team");
        session.record_turn_event(
            "turn-1",
            runtime::models::ChatTurnEventKind::ToolCall {
                call_id: "call-team".to_string(),
                name: "spawn_subagent_batch".to_string(),
                arguments: "{}".to_string(),
            },
        );
        state.refresh_current_session(session);

        assert!(state.is_streaming);
        assert!(state.active_turn.is_some());
        assert!(!state.ignore_stream_frames);
    }

    #[test]
    fn refresh_current_session_keeps_completed_live_turn_until_session_persists_answer() {
        let mut state = AppState::empty();
        let mut session =
            runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session.clone());
        state.push_local_user_message("hello".to_string());
        state.apply_stream_frame(StreamFrame::Data {
            content: "done".to_string(),
        });
        state.apply_stream_frame(StreamFrame::Done { total_tokens: None });

        session
            .messages
            .push(runtime::models::ChatMessage::user("hello"));
        state.refresh_current_session(session);

        assert!(state.active_turn.is_some());
        assert!(state.runtime_cells.is_empty());
        let rendered = state.transcript_cells_for_render();
        assert_eq!(rendered.len(), 3);
        assert_eq!(rendered[0].body, "hello");
        assert_eq!(rendered[1].body, "hello");
        assert_eq!(rendered[2].body, "done");
    }

    #[test]
    fn refresh_current_session_clears_active_turn_when_legacy_messages_project_answer() {
        let mut state = AppState::empty();
        let mut session =
            runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session.clone());
        state.push_local_user_message("hello".to_string());
        state.apply_stream_frame(StreamFrame::Data {
            content: "done".to_string(),
        });
        state.apply_stream_frame(StreamFrame::Done { total_tokens: None });

        session
            .messages
            .push(runtime::models::ChatMessage::user("hello"));
        session
            .messages
            .push(runtime::models::ChatMessage::assistant("done"));
        state.refresh_current_session(session);

        assert!(state.active_turn.is_none());
        assert!(state.runtime_cells.is_empty());
        let rendered = state.transcript_cells_for_render();
        assert_eq!(rendered.len(), 2);
        assert_eq!(rendered[0].body, "hello");
        assert_eq!(rendered[1].body, "done");
    }

    #[test]
    fn refresh_current_session_projects_queued_update_as_user_message_when_turn_finishes() {
        let mut state = AppState::empty();
        let turn_id = "turn-1".to_string();
        let mut session =
            runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session.clone());
        state.begin_stream(turn_id.clone());
        state.push_local_user_message("hello".to_string());
        state.queue_active_turn_update("please be shorter".to_string());

        session.record_turn_user_message(&turn_id, "hello");
        session.record_turn_user_message(&turn_id, "please be shorter");
        session.complete_turn_with_assistant_message(&turn_id, "done");
        state.refresh_current_session(session);

        assert!(state.active_turn.is_none());
        let rendered = state.transcript_cells_for_render();
        assert_eq!(
            rendered
                .iter()
                .map(|cell| (cell.kind, cell.body.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (TranscriptCellKind::User, "hello"),
                (TranscriptCellKind::User, "please be shorter"),
                (TranscriptCellKind::Assistant, "done"),
            ]
        );
    }

    #[test]
    fn refresh_current_session_clears_active_partial_when_session_projects_full_answer() {
        let mut state = AppState::empty();
        let mut session =
            runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session.clone());
        state.begin_stream("stream-1".to_string());
        state.push_local_user_message("hello".to_string());
        state.apply_stream_frame(StreamFrame::Data {
            content: "do".to_string(),
        });

        session
            .messages
            .push(runtime::models::ChatMessage::user("hello"));
        session
            .messages
            .push(runtime::models::ChatMessage::assistant("done"));
        state.refresh_current_session(session);

        assert!(state.active_turn.is_none());
        assert!(!state.is_streaming);
        assert!(!state.apply_stream_frame(StreamFrame::Start {
            stream_id: "stream-1".to_string(),
        }));
        assert!(!state.apply_stream_frame(StreamFrame::Data {
            content: "don".to_string(),
        }));
        let rendered = state.transcript_cells_for_render();
        assert_eq!(rendered.len(), 2);
        assert_eq!(rendered[0].body, "hello");
        assert_eq!(rendered[1].body, "done");
    }

    #[test]
    fn refresh_current_session_clears_active_turn_when_current_stream_is_completed() {
        let mut state = AppState::empty();
        let mut session =
            runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session.clone());

        let stream_id = "stream-1".to_string();
        state.begin_stream(stream_id.clone());
        state.push_local_user_message("hello".to_string());
        state.apply_stream_frame(StreamFrame::Ack {
            content: "working".to_string(),
        });
        assert!(state.active_turn.is_some());

        session.record_turn_user_message(&stream_id, "hello");
        session.complete_turn_with_assistant_message(&stream_id, "done");
        state.refresh_current_session(session);

        assert!(!state.is_streaming);
        assert!(state.current_stream_id.is_none());
        assert!(state.active_turn.is_none());
        assert_eq!(state.conversation_cells.len(), 2);
        assert_eq!(state.conversation_cells[0].body, "hello");
        assert_eq!(state.conversation_cells[1].body, "done");
    }

    #[test]
    fn active_refresh_session_id_survives_until_active_turn_reconciles() {
        let mut state = AppState::empty();
        let mut session =
            runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        let session_id = session.id.clone();
        state.set_current_session(session.clone());
        state.push_local_user_message("hello".to_string());
        state.apply_stream_frame(StreamFrame::Ack {
            content: "done".to_string(),
        });
        state.apply_stream_frame(StreamFrame::Done { total_tokens: None });
        state.thread.clear_session();

        assert_eq!(state.active_refresh_session_id(), Some(session_id.as_str()));

        session.record_turn_user_message("turn-1", "hello");
        session.complete_turn_with_assistant_message("turn-1", "done");
        state.refresh_current_session(session);

        assert!(state.active_turn.is_none());
        assert!(state.active_turn_session_id.is_none());
        assert_eq!(state.active_refresh_session_id(), Some(session_id.as_str()));
    }

    #[test]
    fn refresh_current_session_ignores_late_stream_frames_after_persisted_completion() {
        let mut state = AppState::empty();
        let mut session =
            runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session.clone());

        let stream_id = "stream-1".to_string();
        state.begin_stream(stream_id.clone());
        state.push_local_user_message("hello".to_string());
        state.apply_stream_frame(StreamFrame::Data {
            content: "done".to_string(),
        });

        session.record_turn_user_message(&stream_id, "hello");
        session.complete_turn_with_assistant_message(&stream_id, "done");
        state.refresh_current_session(session);

        assert!(state.active_turn.is_none());
        assert!(!state.is_streaming);
        assert!(state.current_stream_id.is_none());

        assert!(!state.apply_stream_frame(StreamFrame::Data {
            content: "done again".to_string(),
        }));
        assert!(!state.apply_stream_frame(StreamFrame::Done { total_tokens: None }));
        assert!(state.active_turn.is_none());
        assert_eq!(state.conversation_cells.len(), 2);
        assert_eq!(state.conversation_cells[1].body, "done");
    }

    #[test]
    fn refresh_current_session_clears_active_turn_when_persisted_turn_matches_user_message() {
        let mut state = AppState::empty();
        let mut session =
            runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session.clone());

        state.begin_stream("local-stream-id".to_string());
        state.push_local_user_message("create two tasks\n".to_string());
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-1".to_string(),
            name: "manage_tasks".to_string(),
            arguments: serde_json::json!({}),
        });
        assert!(state.active_turn.is_some());

        session.record_turn_user_message("persisted-turn-id", "create two tasks\n");
        session.record_turn_event(
            "persisted-turn-id",
            runtime::models::ChatTurnEventKind::ToolCall {
                call_id: "call-1".to_string(),
                name: "manage_tasks".to_string(),
                arguments: "{}".to_string(),
            },
        );
        session.record_turn_event(
            "persisted-turn-id",
            runtime::models::ChatTurnEventKind::ToolResult {
                call_id: "call-1".to_string(),
                success: true,
                result: "ok".to_string(),
            },
        );
        session.complete_turn_with_assistant_message("persisted-turn-id", "done");

        state.refresh_current_session(session);

        assert!(!state.is_streaming);
        assert!(state.current_stream_id.is_none());
        assert!(state.active_turn.is_none());
        assert!(
            state
                .conversation_cells
                .iter()
                .any(|cell| cell.kind == TranscriptCellKind::Tool && cell.body.contains("ok"))
        );
        assert!(
            state
                .conversation_cells
                .iter()
                .any(|cell| cell.kind == TranscriptCellKind::Assistant && cell.body == "done")
        );
    }

    #[test]
    fn refresh_current_session_clears_active_turn_when_matching_completed_turn_is_not_last() {
        let mut state = AppState::empty();
        let mut session =
            runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session.clone());

        state.begin_stream("local-stream-id".to_string());
        state.push_local_user_message("coordinate team\n".to_string());
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-1".to_string(),
            name: "spawn_subagent_batch".to_string(),
            arguments: serde_json::json!({}),
        });

        session.record_turn_user_message("persisted-turn-id", "coordinate team\n");
        session.record_turn_event(
            "persisted-turn-id",
            runtime::models::ChatTurnEventKind::ToolCall {
                call_id: "call-1".to_string(),
                name: "spawn_subagent_batch".to_string(),
                arguments: "{}".to_string(),
            },
        );
        session.record_turn_event(
            "persisted-turn-id",
            runtime::models::ChatTurnEventKind::ToolResult {
                call_id: "call-1".to_string(),
                success: true,
                result: "child ok".to_string(),
            },
        );
        session.complete_turn_with_assistant_message("persisted-turn-id", "parent ok");
        session.record_turn_user_message("later-running-turn-id", "later message");

        state.refresh_current_session(session);

        assert!(!state.is_streaming);
        assert!(state.current_stream_id.is_none());
        assert!(state.active_turn.is_none());
        assert!(
            state
                .conversation_cells
                .iter()
                .all(|cell| cell.kind != TranscriptCellKind::Tool)
        );
        assert!(
            state
                .conversation_cells
                .iter()
                .any(|cell| cell.kind == TranscriptCellKind::Assistant && cell.body == "parent ok")
        );
    }

    #[test]
    fn refresh_current_session_clears_active_turn_when_persisted_tool_call_matches() {
        let mut state = AppState::empty();
        let mut session =
            runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session.clone());

        state.begin_stream("local-stream-id".to_string());
        state.push_local_user_message("local draft text".to_string());
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-team".to_string(),
            name: "spawn_subagent_batch".to_string(),
            arguments: serde_json::json!({}),
        });

        session.record_turn_user_message("persisted-turn-id", "persisted user text");
        session.record_turn_event(
            "persisted-turn-id",
            runtime::models::ChatTurnEventKind::ToolCall {
                call_id: "call-team".to_string(),
                name: "spawn_subagent_batch".to_string(),
                arguments: "{}".to_string(),
            },
        );
        session.record_turn_event(
            "persisted-turn-id",
            runtime::models::ChatTurnEventKind::ToolResult {
                call_id: "call-team".to_string(),
                success: true,
                result: "child ok".to_string(),
            },
        );
        session.complete_turn_with_assistant_message("persisted-turn-id", "parent ok");

        state.refresh_current_session(session);

        assert!(!state.is_streaming);
        assert!(state.current_stream_id.is_none());
        assert!(state.active_turn.is_none());
    }

    #[test]
    fn refresh_current_session_keeps_active_tool_turn_for_repeated_user_text() {
        let mut state = AppState::empty();
        let mut session =
            runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        session.record_turn_user_message("old-turn", "repeat");
        session.complete_turn_with_assistant_message("old-turn", "old answer");
        state.set_current_session(session.clone());

        state.begin_stream("current-turn".to_string());
        state.push_local_user_message("repeat".to_string());
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-current".to_string(),
            name: "spawn_subagent_batch".to_string(),
            arguments: serde_json::json!({"specs":[{"task":"reply"}]}),
        });

        state.refresh_current_session(session);

        assert!(state.is_streaming);
        assert_eq!(state.current_stream_id.as_deref(), Some("current-turn"));
        assert!(state.active_turn.is_some());
    }

    #[test]
    fn refresh_current_session_clears_active_turn_when_session_messages_have_answer() {
        let mut state = AppState::empty();
        let mut session =
            runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session.clone());

        state.begin_stream("local-stream-id".to_string());
        state.push_local_user_message("coordinate team".to_string());
        state.apply_stream_frame(StreamFrame::Ack {
            content: "parent ok".to_string(),
        });

        session.add_message(runtime::models::ChatMessage::user("coordinate team"));
        session.add_message(runtime::models::ChatMessage::assistant("parent ok"));

        state.refresh_current_session(session);

        assert!(!state.is_streaming);
        assert!(state.current_stream_id.is_none());
        assert!(state.active_turn.is_none());
    }

    #[test]
    fn pending_user_message_stays_before_local_assistant_finalize() {
        let mut state = AppState::empty();
        let session = runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
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
    fn daemon_backed_stream_done_waits_for_session_refresh_before_stable_runtime() {
        let mut state = AppState::empty();
        let session = runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session);
        state.push_local_user_message("hello".to_string());
        state.apply_stream_frame(StreamFrame::Ack {
            content: "done".to_string(),
        });

        state.apply_stream_frame(StreamFrame::Done { total_tokens: None });

        assert!(state.runtime_cells.is_empty());
        let rendered = state.transcript_cells_for_render();
        assert_eq!(rendered.len(), 2);
        assert_eq!(rendered[0].kind, TranscriptCellKind::User);
        assert_eq!(rendered[1].kind, TranscriptCellKind::Assistant);
    }

    #[test]
    fn refresh_after_interrupted_stream_preserves_partial_turn_after_user() {
        let mut state = AppState::empty();
        let session = runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session.clone());
        state.push_local_user_message("first".to_string());
        state.apply_stream_frame(StreamFrame::Ack {
            content: "partial answer".to_string(),
        });
        state.cancel_active_response();

        let mut updated = session;
        updated
            .messages
            .push(runtime::models::ChatMessage::user("first"));
        state.refresh_current_session(updated);

        let rendered = state.transcript_cells_for_render();
        assert_eq!(rendered.len(), 2);
        assert_eq!(rendered[0].kind, TranscriptCellKind::User);
        assert_eq!(rendered[0].body, "first");
        assert_eq!(rendered[1].kind, TranscriptCellKind::Assistant);
        assert_eq!(rendered[1].body, "partial answer");
        assert!(state.active_turn.is_none());
    }

    #[test]
    fn next_submit_preserves_interrupted_turn_before_new_user() {
        let mut state = AppState::empty();
        let session = runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session);
        state.push_local_user_message("first".to_string());
        state.apply_stream_frame(StreamFrame::Ack {
            content: "partial answer".to_string(),
        });
        state.cancel_active_response();
        state.push_local_user_message("second".to_string());

        let rendered = state.transcript_cells_for_render();
        assert_eq!(rendered.len(), 3);
        assert_eq!(rendered[0].kind, TranscriptCellKind::User);
        assert_eq!(rendered[0].body, "first");
        assert_eq!(rendered[1].kind, TranscriptCellKind::Assistant);
        assert_eq!(rendered[1].body, "partial answer");
        assert_eq!(rendered[2].kind, TranscriptCellKind::User);
        assert_eq!(rendered[2].body, "second");
    }

    #[test]
    fn failed_first_turn_stays_before_later_persisted_success() {
        let mut state = AppState::empty();
        let session = runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session.clone());

        state.push_local_user_message("first".to_string());
        state.apply_stream_frame(StreamFrame::error(500, "preflight failed"));

        state.push_local_user_message("second".to_string());
        state.apply_stream_frame(StreamFrame::Ack {
            content: "OK".to_string(),
        });
        state.apply_stream_frame(StreamFrame::Done { total_tokens: None });

        let mut updated = session;
        updated
            .messages
            .push(runtime::models::ChatMessage::user("second"));
        updated
            .messages
            .push(runtime::models::ChatMessage::assistant("OK"));
        state.refresh_current_session(updated);

        let rendered = state.transcript_cells_for_render();
        assert_eq!(rendered[0].kind, TranscriptCellKind::User);
        assert_eq!(rendered[0].body, "first");
        assert_eq!(rendered[1].kind, TranscriptCellKind::Notice);
        assert_eq!(rendered[1].title, "Error");
        assert!(rendered[1].body.contains("preflight failed"));
        assert_eq!(rendered[2].kind, TranscriptCellKind::User);
        assert_eq!(rendered[2].body, "second");
        assert_eq!(rendered[3].kind, TranscriptCellKind::Assistant);
        assert_eq!(rendered[3].body, "OK");
    }

    #[test]
    fn duplicate_stream_and_plain_errors_are_collapsed() {
        let mut state = AppState::empty();
        state.push_error("Stream error 500: Preflight check failed:\n- missing secret");
        state.push_error("Preflight check failed:\n- missing secret");

        assert_eq!(state.runtime_cells.len(), 1);
        assert_eq!(state.runtime_cells[0].cell.title, "Error");
        assert!(
            state.runtime_cells[0]
                .cell
                .body
                .contains("Preflight check failed")
        );
    }

    #[test]
    fn refresh_removes_runtime_error_when_session_persists_equivalent_error() {
        let mut state = AppState::empty();
        let mut session =
            runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session.clone());
        state.push_error("Stream error 500: Preflight check failed:\n- missing secret");

        session.record_turn_user_message("turn-1", "hello");
        session.record_turn_event(
            "turn-1",
            runtime::models::ChatTurnEventKind::Error {
                message: "Preflight check failed:\n- missing secret".to_string(),
            },
        );
        state.refresh_current_session(session);

        let rendered = state.transcript_cells_for_render();
        let errors = rendered
            .iter()
            .filter(|cell| cell.title == "Error")
            .collect::<Vec<_>>();
        assert_eq!(errors.len(), 1);
        assert_eq!(
            super::normalized_error_notice(&errors[0].body),
            "Preflight check failed:\n- missing secret"
        );
    }

    #[test]
    fn refresh_removes_runtime_tool_cells_when_session_persists_turn_events() {
        let mut state = AppState::empty();
        let session = runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session);
        state.push_local_user_message("hello".to_string());
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-1".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({"command":"pwd"}),
        });
        state.apply_stream_frame(StreamFrame::ToolResult {
            id: "call-1".to_string(),
            success: true,
            result: "/tmp".to_string(),
        });
        state.apply_stream_frame(StreamFrame::Data {
            content: "done".to_string(),
        });
        state.apply_stream_frame(StreamFrame::Done { total_tokens: None });

        let mut persisted =
            runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        persisted.record_turn_user_message("turn-1", "hello");
        persisted.record_turn_event(
            "turn-1",
            runtime::models::ChatTurnEventKind::ToolCall {
                call_id: "call-1".to_string(),
                name: "bash".to_string(),
                arguments: "{\"command\":\"pwd\"}".to_string(),
            },
        );
        persisted.record_turn_event(
            "turn-1",
            runtime::models::ChatTurnEventKind::ToolResult {
                call_id: "call-1".to_string(),
                success: true,
                result: "/tmp".to_string(),
            },
        );
        persisted.complete_turn_with_assistant_message("turn-1", "done");

        state.refresh_current_session(persisted);

        let rendered = state.transcript_cells_for_render();
        assert_eq!(
            rendered
                .iter()
                .filter(|cell| cell.kind == TranscriptCellKind::Tool)
                .count(),
            1
        );
        assert!(state.runtime_cells.is_empty());
        assert!(state.active_turn.is_none());
    }

    #[test]
    fn refresh_removes_runtime_cells_when_persisted_turn_has_equivalent_live_content() {
        let mut state = AppState::empty();
        let session = runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session);
        state.push_local_user_message("hello".to_string());
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "call-1".to_string(),
            name: "spawn_subagent_batch".to_string(),
            arguments: serde_json::json!({"specs":[{"task":"reply"}]}),
        });
        state.apply_stream_frame(StreamFrame::ToolResult {
            id: "call-1".to_string(),
            success: true,
            result: "{\"status\":\"completed\"}".to_string(),
        });
        state.apply_stream_frame(StreamFrame::Data {
            content: "- **耗时**:1326ms".to_string(),
        });
        state.apply_stream_frame(StreamFrame::Done { total_tokens: None });

        let mut persisted =
            runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        persisted.record_turn_user_message("turn-1", "hello");
        persisted.record_turn_event(
            "turn-1",
            runtime::models::ChatTurnEventKind::ToolCall {
                call_id: "call-1".to_string(),
                name: "spawn_subagent_batch".to_string(),
                arguments: "{\"specs\":[{\"task\":\"reply\"}]}".to_string(),
            },
        );
        persisted.record_turn_event(
            "turn-1",
            runtime::models::ChatTurnEventKind::ToolResult {
                call_id: "call-1".to_string(),
                success: true,
                result: "{\"operation\":\"spawn\",\"status\":\"completed\"}".to_string(),
            },
        );
        persisted.complete_turn_with_assistant_message("turn-1", "- **耗时**: 1326ms");

        state.refresh_current_session(persisted);

        let rendered = state.transcript_cells_for_render();
        assert_eq!(
            rendered
                .iter()
                .filter(|cell| cell.kind == TranscriptCellKind::Tool)
                .count(),
            0
        );
        assert_eq!(
            rendered
                .iter()
                .filter(|cell| cell.kind == TranscriptCellKind::Assistant)
                .count(),
            1
        );
        assert!(state.runtime_cells.is_empty());
        assert!(state.active_turn.is_none());
    }

    #[test]
    fn refresh_keeps_persisted_team_activity_when_final_answer_matches() {
        let mut state = AppState::empty();
        let session = runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        state.set_current_session(session);
        state.push_local_user_message("run one subagent".to_string());
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "spawn-call".to_string(),
            name: "spawn_subagent_batch".to_string(),
            arguments: serde_json::json!({
                "specs": [{"task": "reply TEAM_MESSAGE_PANEL_CHILD_OK"}]
            }),
        });
        state.apply_stream_frame(StreamFrame::ToolResult {
            id: "spawn-call".to_string(),
            success: true,
            result: serde_json::json!({
                "operation": "spawn",
                "status": "spawned",
                "task_ids": ["child-1"]
            })
            .to_string(),
        });
        state.apply_stream_frame(StreamFrame::ToolCall {
            id: "wait-call".to_string(),
            name: "wait_subagents".to_string(),
            arguments: serde_json::json!({
                "task_ids": ["child-1"],
                "timeout_secs": 60
            }),
        });
        state.apply_stream_frame(StreamFrame::ToolResult {
            id: "wait-call".to_string(),
            success: true,
            result: serde_json::json!({
                "results": [{
                    "duration_ms": 5027,
                    "output": "TEAM_MESSAGE_PANEL_CHILD_OK",
                    "status": "completed",
                    "task_id": "child-1"
                }]
            })
            .to_string(),
        });

        let mut persisted =
            runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        persisted.record_turn_user_message("turn-1", "run one subagent");
        persisted.record_turn_event(
            "turn-1",
            runtime::models::ChatTurnEventKind::ToolCall {
                call_id: "spawn-call".to_string(),
                name: "spawn_subagent_batch".to_string(),
                arguments: serde_json::json!({
                    "specs": [{"task": "reply TEAM_MESSAGE_PANEL_CHILD_OK"}]
                })
                .to_string(),
            },
        );
        persisted.record_turn_event(
            "turn-1",
            runtime::models::ChatTurnEventKind::ToolResult {
                call_id: "spawn-call".to_string(),
                success: true,
                result: serde_json::json!({
                    "operation": "spawn",
                    "status": "spawned",
                    "task_ids": ["child-1"]
                })
                .to_string(),
            },
        );
        persisted.record_turn_event(
            "turn-1",
            runtime::models::ChatTurnEventKind::ToolCall {
                call_id: "wait-call".to_string(),
                name: "wait_subagents".to_string(),
                arguments: serde_json::json!({
                    "task_ids": ["child-1"],
                    "timeout_secs": 60
                })
                .to_string(),
            },
        );
        persisted.record_turn_event(
            "turn-1",
            runtime::models::ChatTurnEventKind::ToolResult {
                call_id: "wait-call".to_string(),
                success: true,
                result: serde_json::json!({
                    "results": [{
                        "duration_ms": 5027,
                        "output": "TEAM_MESSAGE_PANEL_CHILD_OK",
                        "status": "completed",
                        "task_id": "child-1"
                    }]
                })
                .to_string(),
            },
        );
        persisted.complete_turn_with_assistant_message(
            "turn-1",
            "TEAM_MESSAGE_PANEL_PARENT_OK\n\nTEAM_MESSAGE_PANEL_CHILD_OK",
        );

        state.refresh_current_session(persisted);

        let rendered = state.transcript_cells_for_render();
        assert!(state.active_turn.is_none());
        assert!(state.runtime_cells.is_empty());
        assert_eq!(
            rendered
                .iter()
                .filter(|cell| cell.kind == TranscriptCellKind::Subagent)
                .count(),
            2
        );
        let assistant = rendered
            .iter()
            .find(|cell| cell.kind == TranscriptCellKind::Assistant)
            .expect("assistant cell");
        assert!(assistant.body.contains("TEAM_MESSAGE_PANEL_PARENT_OK"));
        assert!(assistant.body.contains("TEAM_MESSAGE_PANEL_CHILD_OK"));
    }

    #[test]
    fn clear_current_session_keeps_notices() {
        let mut state = AppState::empty();
        let mut session =
            runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        session
            .messages
            .push(runtime::models::ChatMessage::user("hello"));
        state.set_current_session(session);
        state.push_info("notice");

        state.clear_current_session("session missing");

        assert_eq!(state.conversation_cells.len(), 0);
        assert_eq!(state.runtime_cells.len(), 1);
        assert_eq!(state.runtime_cells[0].cell.title, "Info");
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
        assert!(state.active_turn.is_none());
    }

    #[test]
    fn set_current_session_resets_runtime_cells_for_new_session() {
        let mut state = AppState::empty();
        state.push_info("notice");
        let mut session =
            runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        session
            .messages
            .push(runtime::models::ChatMessage::user("hello"));

        state.set_current_session(session);

        assert_eq!(state.conversation_cells.len(), 1);
        assert_eq!(state.conversation_cells[0].kind, TranscriptCellKind::User);
        assert!(state.runtime_cells.is_empty());
    }

    #[test]
    fn refresh_current_session_preserves_runtime_cells_and_streaming_active_turn() {
        let mut state = AppState::empty();
        let mut session =
            runtime::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        session
            .messages
            .push(runtime::models::ChatMessage::user("hello"));
        state.set_current_session(session.clone());
        state.push_info("notice");
        state.is_streaming = true;
        state.apply_stream_frame(StreamFrame::Ack {
            content: "chunk".to_string(),
        });

        let mut updated = session.clone();
        updated
            .messages
            .push(runtime::models::ChatMessage::assistant("reply"));
        state.refresh_current_session(updated);

        assert_eq!(state.conversation_cells.len(), 2);
        assert_eq!(state.runtime_cells.len(), 1);
        assert_eq!(state.runtime_cells[0].cell.title, "Info");
        assert!(state.active_turn.is_some());
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
