use std::collections::HashSet;

use restflow_core::daemon::{ChatSessionEvent, StreamFrame};
use restflow_core::models::{ChatSession, ChatSessionSummary, ExecutionThread, RunSummary};
use restflow_core::runtime::TaskStreamEvent;
use restflow_traits::{PendingTeamApproval, TeamAssignment, TeamMessage, TeamMessageKind, TeamState};

use super::composer::ComposerState;
use super::transcript::{
    ShellMessage, TranscriptCell, cell_from_message, message_from_session_event,
    message_from_stream_frame, message_from_task_event, message_from_team_message,
    messages_from_session, transcript_cells,
};

#[derive(Debug, Clone)]
pub enum RunPickerItem {
    Run { run_id: String, title: String, status: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ThreadFocus {
    #[default]
    Session,
    Run { run_id: String },
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

    pub fn focus_label(&self) -> String {
        match &self.focus {
            ThreadFocus::Session => "session".to_string(),
            ThreadFocus::Run { run_id } => format!("run:{run_id}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamOverlayTab {
    Members,
    Messages,
    Assignments,
    Approvals,
}

#[derive(Debug, Clone)]
pub enum OverlayState {
    SessionPicker { selected: usize },
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
    // Conversation cells are rebuilt from persisted session messages and should stay stable.
    pub conversation_cells: Vec<TranscriptCell>,
    // Runtime cells are ephemeral UI feedback for the current turn only.
    pub runtime_cells: Vec<TranscriptCell>,
    // Active cell is the single in-flight assistant response while streaming.
    pub active_cell: Option<TranscriptCell>,
    pub overlay: Option<OverlayState>,
    pub transcript_scroll: u16,
    pub composer: ComposerState,
    pub status: String,
    pub is_streaming: bool,
    pub startup: Option<StartupState>,
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
            conversation_cells: Vec::new(),
            runtime_cells: Vec::new(),
            active_cell: None,
            overlay: None,
            transcript_scroll: 0,
            composer: ComposerState::default(),
            status: "Connecting to daemon...".to_string(),
            is_streaming: false,
            startup: None,
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

    pub fn focus_label(&self) -> String {
        self.thread.focus_label()
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

    pub fn take_pending_initial_message(&mut self) -> Option<String> {
        self.pending_initial_message.take()
    }

    pub fn enter_startup(
        &mut self,
        agent_override: Option<String>,
        session_override: Option<String>,
    ) {
        self.startup = Some(StartupState {
            starting_daemon: false,
            error: None,
            agent_override,
            session_override,
        });
        self.status = "RestFlow daemon is not running".to_string();
    }

    pub fn mark_starting_daemon(&mut self) {
        if let Some(startup) = self.startup.as_mut() {
            startup.starting_daemon = true;
            startup.error = None;
            self.status = "Starting daemon...".to_string();
        }
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
        if session_changed {
            self.clear_team_context();
        }
        self.seen_team_message_ids.clear();
        self.runtime_cells.clear();
        self.active_cell = None;
        self.conversation_cells =
            transcript_cells(&messages_from_session(&session), self.assistant_name());
        self.transcript_scroll_to_bottom();
    }

    pub fn refresh_current_session(&mut self, session: ChatSession) {
        self.thread.session = Some(session.clone());
        self.replace_session_projection(messages_from_session(&session));
    }

    pub fn clear_current_session(&mut self, notice: impl Into<String>) {
        self.thread.clear_session();
        self.clear_team_context();
        self.replace_session_projection(Vec::new());
        self.runtime_cells.clear();
        self.active_cell = None;
        self.push_info(notice);
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

    pub fn open_session_picker(&mut self) {
        self.overlay = Some(OverlayState::SessionPicker { selected: 0 });
    }

    pub fn open_run_picker(&mut self) {
        self.overlay = Some(OverlayState::RunPicker { selected: 0 });
    }

    pub fn open_approval_picker(&mut self) {
        self.overlay = Some(OverlayState::ApprovalPicker { selected: 0 });
    }

    pub fn open_team_overlay(&mut self) {
        self.overlay = Some(OverlayState::TeamView {
            tab: TeamOverlayTab::Members,
            scroll: 0,
        });
    }

    pub fn open_help_overlay(&mut self) {
        self.overlay = Some(OverlayState::Help);
    }

    pub fn move_overlay_selection(&mut self, delta: isize) {
        let len = match self.overlay_item_len() {
            Some(len) if len > 0 => len,
            _ => return,
        };
        match self.overlay.as_mut() {
            Some(OverlayState::SessionPicker { selected })
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
            OverlayState::SessionPicker { .. } => Some(self.sessions.len()),
            OverlayState::RunPicker { .. } => Some(self.run_picker_items().len()),
            OverlayState::ApprovalPicker { .. } => Some(self.current_team_approvals.len()),
            OverlayState::TeamView { .. } | OverlayState::Help => None,
        }
    }

    pub fn selected_session_id(&self) -> Option<&str> {
        match self.overlay.as_ref() {
            Some(OverlayState::SessionPicker { selected }) => {
                self.sessions.get(*selected).map(|session| session.id.as_str())
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

    pub fn selected_approval(&self) -> Option<&PendingTeamApproval> {
        match self.overlay.as_ref() {
            Some(OverlayState::ApprovalPicker { selected }) => {
                self.current_team_approvals.get(*selected)
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
        let cell = cell_from_message(&message, self.assistant_name());
        if cell.is_conversation_cell() {
            self.conversation_cells.push(cell);
        } else {
            self.runtime_cells.push(cell);
        }
        self.transcript_scroll_to_bottom();
    }

    pub fn replace_session_projection(&mut self, messages: Vec<ShellMessage>) {
        self.conversation_cells = transcript_cells(&messages, self.assistant_name());
        self.transcript_scroll_to_bottom();
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

    pub fn record_team_message(&mut self, message: &TeamMessage) {
        if self.seen_team_message_ids.insert(message.message_id.clone()) {
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

    pub fn transcript_scroll_to_bottom(&mut self) {
        self.transcript_scroll = 0;
    }

    pub fn scroll_transcript(&mut self, delta: i16) {
        let next = (self.transcript_scroll as i16 - delta).max(0) as u16;
        self.transcript_scroll = next;
    }

    fn append_assistant_stream_chunk(&mut self, chunk: &str) {
        if chunk.is_empty() {
            return;
        }

        if self
            .active_cell
            .as_mut()
            .is_some_and(|cell| cell.append_chunk(chunk))
        {
            self.transcript_scroll_to_bottom();
            return;
        }

        if chunk.trim().is_empty() {
            return;
        }

        self.runtime_cells.clear();
        self.active_cell = Some(cell_from_message(
            &ShellMessage::AssistantStream {
                content: chunk.to_string(),
            },
            self.assistant_name(),
        ));
        self.transcript_scroll_to_bottom();
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
                if let Some(mut active_cell) = self.active_cell.take()
                    && !active_cell.body.trim().is_empty()
                {
                    let _ = active_cell.finalize();
                    self.conversation_cells.push(active_cell);
                    self.transcript_scroll_to_bottom();
                }
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
                        self.active_cell = None;
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

    pub fn transcript_cells_for_render(&self) -> Vec<TranscriptCell> {
        let mut cells = self.conversation_cells.clone();
        cells.extend(self.runtime_cells.clone());
        if let Some(active_cell) = self.active_cell.clone() {
            cells.push(active_cell);
        }
        cells
    }

    fn assistant_name(&self) -> &str {
        self.default_agent_name.as_deref().unwrap_or("Agent")
    }
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
        assert!(matches!(state.overlay, Some(OverlayState::SessionPicker { .. })));
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
        let mut session = restflow_core::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        session.messages.push(restflow_core::models::ChatMessage::user("hello"));
        state.set_current_session(session.clone());
        state.push_info("notice");

        let mut updated = session.clone();
        updated
            .messages
            .push(restflow_core::models::ChatMessage::assistant("hi"));
        state.refresh_current_session(updated);

        assert_eq!(state.conversation_cells.len(), 2);
        assert_eq!(state.runtime_cells.len(), 1);
        assert_eq!(state.runtime_cells[0].title, "Info");
    }

    #[test]
    fn clear_current_session_keeps_notices() {
        let mut state = AppState::empty();
        let mut session = restflow_core::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        session.messages.push(restflow_core::models::ChatMessage::user("hello"));
        state.set_current_session(session);
        state.push_info("notice");

        state.clear_current_session("session missing");

        assert_eq!(state.conversation_cells.len(), 0);
        assert_eq!(state.runtime_cells.len(), 1);
        assert_eq!(state.runtime_cells[0].title, "Info");
    }

    #[test]
    fn switching_session_clears_team_context() {
        let mut state = AppState::empty();
        let first = restflow_core::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        let second = restflow_core::models::ChatSession::new("agent-1".to_string(), "model".to_string());
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
    fn transcript_scroll_uses_bottom_anchored_offsets() {
        let mut state = AppState::empty();
        state.scroll_transcript(-3);
        assert_eq!(state.transcript_scroll, 3);
        state.scroll_transcript(2);
        assert_eq!(state.transcript_scroll, 1);
        state.transcript_scroll_to_bottom();
        assert_eq!(state.transcript_scroll, 0);
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
        let mut session = restflow_core::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        session.messages.push(restflow_core::models::ChatMessage::user("hello"));

        state.set_current_session(session);

        assert_eq!(state.conversation_cells.len(), 1);
        assert_eq!(state.conversation_cells[0].kind, TranscriptCellKind::User);
        assert!(state.runtime_cells.is_empty());
    }

    #[test]
    fn refresh_current_session_preserves_runtime_cells_and_active_cell() {
        let mut state = AppState::empty();
        let mut session = restflow_core::models::ChatSession::new("agent-1".to_string(), "model".to_string());
        session.messages.push(restflow_core::models::ChatMessage::user("hello"));
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
        assert_eq!(state.runtime_cells[0].title, "Info");
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

        state.mark_starting_daemon();
        assert!(
            state
                .startup_state()
                .expect("startup state")
                .starting_daemon
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
