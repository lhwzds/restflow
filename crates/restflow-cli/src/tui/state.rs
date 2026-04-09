use restflow_core::daemon::{ChatSessionEvent, StreamFrame};
use restflow_core::models::{ChatRole, ChatSession, ChatSessionSummary, ExecutionThread, RunSummary, Task};
use restflow_core::runtime::TaskStreamEvent;
use restflow_traits::{PendingTeamApproval, TeamAssignment, TeamMessage, TeamState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptKind {
    User,
    Assistant,
    System,
    Ack,
    Data,
    ToolCall,
    ToolResult,
    SessionEvent,
    TaskEvent,
    Info,
    Error,
}

#[derive(Debug, Clone)]
pub struct TranscriptEntry {
    pub kind: TranscriptKind,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct InputState {
    pub text: String,
    pub cursor: usize,
}

impl InputState {
    pub fn insert_char(&mut self, ch: char) {
        self.text.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    pub fn insert_newline(&mut self) {
        self.insert_char('\n');
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.text[..self.cursor]
            .char_indices()
            .last()
            .map(|(idx, _)| idx)
            .unwrap_or(0);
        self.text.replace_range(prev..self.cursor, "");
        self.cursor = prev;
    }

    pub fn move_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor = self.text[..self.cursor]
            .char_indices()
            .last()
            .map(|(idx, _)| idx)
            .unwrap_or(0);
    }

    pub fn move_right(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }
        let next = self.text[self.cursor..]
            .char_indices()
            .nth(1)
            .map(|(idx, _)| self.cursor + idx)
            .unwrap_or(self.text.len());
        self.cursor = next;
    }

    pub fn take(&mut self) -> String {
        self.cursor = 0;
        std::mem::take(&mut self.text)
    }
}

#[derive(Debug, Clone)]
pub enum RunPickerItem {
    Task { id: String, title: String, status: String },
    Run { run_id: String, title: String, status: String },
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

#[derive(Debug, Clone)]
pub struct AppState {
    pub default_agent_name: Option<String>,
    pub default_agent_id: Option<String>,
    pub current_session: Option<ChatSession>,
    pub current_task_id: Option<String>,
    pub current_run_id: Option<String>,
    pub current_thread: Option<ExecutionThread>,
    pub current_team_state: Option<TeamState>,
    pub current_team_messages: Vec<TeamMessage>,
    pub current_team_assignments: Vec<TeamAssignment>,
    pub current_team_approvals: Vec<PendingTeamApproval>,
    pub sessions: Vec<ChatSessionSummary>,
    pub tasks: Vec<Task>,
    pub runs: Vec<RunSummary>,
    pub child_runs: Vec<RunSummary>,
    pub transcript: Vec<TranscriptEntry>,
    pub overlay: Option<OverlayState>,
    pub transcript_scroll: u16,
    pub input: InputState,
    pub input_history: Vec<String>,
    pub history_cursor: Option<usize>,
    pub status: String,
    pub is_streaming: bool,
}

impl AppState {
    pub fn empty() -> Self {
        Self {
            default_agent_name: None,
            default_agent_id: None,
            current_session: None,
            current_task_id: None,
            current_run_id: None,
            current_thread: None,
            current_team_state: None,
            current_team_messages: Vec::new(),
            current_team_assignments: Vec::new(),
            current_team_approvals: Vec::new(),
            sessions: Vec::new(),
            tasks: Vec::new(),
            runs: Vec::new(),
            child_runs: Vec::new(),
            transcript: Vec::new(),
            overlay: None,
            transcript_scroll: 0,
            input: InputState::default(),
            input_history: Vec::new(),
            history_cursor: None,
            status: "Connecting to daemon...".to_string(),
            is_streaming: false,
        }
    }

    pub fn set_default_agent(&mut self, id: Option<String>, name: Option<String>) {
        self.default_agent_id = id;
        self.default_agent_name = name;
    }

    pub fn set_current_session(&mut self, session: ChatSession) {
        self.current_session = Some(session.clone());
        self.current_task_id = None;
        self.current_run_id = None;
        self.current_thread = None;
        self.transcript = transcript_from_session(&session);
        self.transcript_scroll_to_bottom();
    }

    pub fn set_current_task(&mut self, task_id: String) {
        self.current_task_id = Some(task_id);
        self.current_run_id = None;
        self.current_thread = None;
    }

    pub fn set_current_run(&mut self, run_id: String, thread: ExecutionThread) {
        self.current_run_id = Some(run_id);
        self.current_thread = Some(thread);
    }

    pub fn clear_overlay(&mut self) {
        self.overlay = None;
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
            Some(OverlayState::RunPicker { selected }) => self.run_picker_items().get(*selected).cloned(),
            _ => None,
        }
    }

    pub fn selected_approval(&self) -> Option<&PendingTeamApproval> {
        match self.overlay.as_ref() {
            Some(OverlayState::ApprovalPicker { selected }) => self.current_team_approvals.get(*selected),
            _ => None,
        }
    }

    pub fn run_picker_items(&self) -> Vec<RunPickerItem> {
        let mut items = self
            .tasks
            .iter()
            .map(|task| RunPickerItem::Task {
                id: task.id.clone(),
                title: task.name.clone(),
                status: format!("{:?}", task.status),
            })
            .collect::<Vec<_>>();
        items.extend(self.runs.iter().filter_map(|run| {
            run.run_id.as_ref().map(|run_id| RunPickerItem::Run {
                run_id: run_id.clone(),
                title: run.title.clone(),
                status: run.status.clone(),
            })
        }));
        items.extend(self.child_runs.iter().filter_map(|run| {
            run.run_id.as_ref().map(|run_id| RunPickerItem::Run {
                run_id: run_id.clone(),
                title: format!("↳ {}", run.title),
                status: run.status.clone(),
            })
        }));
        items
    }

    pub fn push_transcript(&mut self, kind: TranscriptKind, text: impl Into<String>) {
        self.transcript.push(TranscriptEntry {
            kind,
            text: text.into(),
        });
        self.transcript_scroll_to_bottom();
    }

    pub fn transcript_scroll_to_bottom(&mut self) {
        self.transcript_scroll = self.transcript.len().saturating_sub(1) as u16;
    }

    pub fn scroll_transcript(&mut self, delta: i16) {
        let next = (self.transcript_scroll as i16 + delta).max(0) as u16;
        self.transcript_scroll = next;
    }

    pub fn push_history(&mut self, entry: String) {
        if entry.trim().is_empty() {
            return;
        }
        if self.input_history.last() != Some(&entry) {
            self.input_history.push(entry);
        }
        self.history_cursor = None;
    }

    pub fn history_previous(&mut self) {
        if self.input_history.is_empty() {
            return;
        }
        let next = match self.history_cursor {
            Some(index) if index > 0 => index - 1,
            Some(index) => index,
            None => self.input_history.len() - 1,
        };
        self.history_cursor = Some(next);
        self.input.text = self.input_history[next].clone();
        self.input.cursor = self.input.text.len();
    }

    pub fn history_next(&mut self) {
        let Some(index) = self.history_cursor else {
            return;
        };
        if index + 1 >= self.input_history.len() {
            self.history_cursor = None;
            self.input.text.clear();
            self.input.cursor = 0;
            return;
        }
        let next = index + 1;
        self.history_cursor = Some(next);
        self.input.text = self.input_history[next].clone();
        self.input.cursor = self.input.text.len();
    }

    pub fn apply_stream_frame(&mut self, frame: StreamFrame) {
        match frame {
            StreamFrame::Start { stream_id } => {
                self.is_streaming = true;
                self.status = format!("Streaming response ({stream_id})");
            }
            StreamFrame::Ack { content } => self.push_transcript(TranscriptKind::Ack, content),
            StreamFrame::Data { content } => self.push_transcript(TranscriptKind::Data, content),
            StreamFrame::ToolCall { id, name, arguments } => self.push_transcript(
                TranscriptKind::ToolCall,
                format!("{name}#{id} {}", arguments),
            ),
            StreamFrame::ToolResult { id, result, success } => self.push_transcript(
                TranscriptKind::ToolResult,
                format!("#{id} success={success} {result}"),
            ),
            StreamFrame::Event { .. } => {}
            StreamFrame::Done { total_tokens } => {
                self.is_streaming = false;
                self.status = match total_tokens {
                    Some(total_tokens) => format!("Stream finished ({total_tokens} tokens)"),
                    None => "Stream finished".to_string(),
                };
            }
            StreamFrame::Error(error) => {
                self.is_streaming = false;
                self.push_transcript(
                    TranscriptKind::Error,
                    format!("Stream error {}: {}", error.code, error.message),
                );
                self.status = "Stream failed".to_string();
            }
        }
    }

    pub fn apply_session_event(&mut self, event: ChatSessionEvent) {
        self.push_transcript(TranscriptKind::SessionEvent, format!("session_event {event:?}"));
    }

    pub fn apply_task_event(&mut self, event: TaskStreamEvent) {
        self.push_transcript(TranscriptKind::TaskEvent, format!("task_event {}", event.task_id));
    }
}

pub fn transcript_from_session(session: &ChatSession) -> Vec<TranscriptEntry> {
    session
        .messages
        .iter()
        .map(|message| TranscriptEntry {
            kind: match message.role {
                ChatRole::User => TranscriptKind::User,
                ChatRole::Assistant => TranscriptKind::Assistant,
                ChatRole::System => TranscriptKind::System,
            },
            text: message.content.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_state_insert_and_backspace_round_trip() {
        let mut input = InputState::default();
        input.insert_char('h');
        input.insert_char('i');
        assert_eq!(input.text, "hi");
        input.backspace();
        assert_eq!(input.text, "h");
    }

    #[test]
    fn app_state_session_picker_uses_overlay() {
        let mut state = AppState::empty();
        state.open_session_picker();
        assert!(matches!(state.overlay, Some(OverlayState::SessionPicker { .. })));
    }

    #[test]
    fn input_history_round_trip() {
        let mut state = AppState::empty();
        state.push_history("first".to_string());
        state.push_history("second".to_string());
        state.history_previous();
        assert_eq!(state.input.text, "second");
        state.history_previous();
        assert_eq!(state.input.text, "first");
        state.history_next();
        assert_eq!(state.input.text, "second");
    }
}
