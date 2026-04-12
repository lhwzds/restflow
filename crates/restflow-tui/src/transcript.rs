use restflow_core::daemon::{ChatSessionEvent, StreamFrame};
use restflow_core::models::{ChatRole, ChatSession};
use restflow_core::runtime::TaskStreamEvent;
use restflow_core::runtime::background_agent::StreamEventKind;
use restflow_traits::{TeamMessage, TeamMessageKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellMessage {
    UserMessage { content: String },
    AssistantMessage { content: String },
    SystemMessage { content: String },
    AssistantStream { content: String },
    ToolCall {
        call_id: String,
        name: String,
        arguments: String,
    },
    ToolResult {
        call_id: String,
        success: bool,
        result: String,
    },
    TaskNotice { content: String },
    ApprovalNotice {
        approval_id: Option<String>,
        content: String,
    },
    TeamNotice { content: String },
    InfoNotice { content: String },
    ErrorNotice { content: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageGroup {
    Conversation,
    RuntimeNotice,
    ToolActivity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptCellKind {
    User,
    Assistant,
    System,
    Notice,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptCell {
    pub kind: TranscriptCellKind,
    pub title: String,
    pub subtitle: Option<String>,
    pub body: String,
    pub group: MessageGroup,
    pub is_active: bool,
}

impl TranscriptCell {
    // Conversation cells are the durable chat history reconstructed from session messages.
    pub fn is_conversation_cell(&self) -> bool {
        matches!(
            self.kind,
            TranscriptCellKind::User | TranscriptCellKind::Assistant | TranscriptCellKind::System
        )
    }

    pub fn append_chunk(&mut self, chunk: &str) -> bool {
        match self.kind {
            TranscriptCellKind::Assistant if self.is_active => {
                self.body.push_str(chunk);
                true
            }
            _ => false,
        }
    }

    pub fn finalize(&mut self) -> bool {
        match self.kind {
            TranscriptCellKind::Assistant if self.is_active => {
                self.is_active = false;
                self.subtitle = None;
                true
            }
            _ => false,
        }
    }
}

impl ShellMessage {
    pub fn group(&self) -> MessageGroup {
        match self {
            Self::UserMessage { .. }
            | Self::AssistantMessage { .. }
            | Self::AssistantStream { .. }
            | Self::SystemMessage { .. } => MessageGroup::Conversation,
            Self::ToolCall { .. } | Self::ToolResult { .. } => MessageGroup::ToolActivity,
            Self::TaskNotice { .. }
            | Self::ApprovalNotice { .. }
            | Self::TeamNotice { .. }
            | Self::InfoNotice { .. }
            | Self::ErrorNotice { .. } => MessageGroup::RuntimeNotice,
        }
    }
}

pub fn messages_from_session(session: &ChatSession) -> Vec<ShellMessage> {
    session
        .messages
        .iter()
        .map(|message| match message.role {
            ChatRole::User => ShellMessage::UserMessage {
                content: message.content.clone(),
            },
            ChatRole::Assistant => ShellMessage::AssistantMessage {
                content: message.content.clone(),
            },
            ChatRole::System => ShellMessage::SystemMessage {
                content: message.content.clone(),
            },
        })
        .collect()
}

pub fn transcript_cells(messages: &[ShellMessage], assistant_name: &str) -> Vec<TranscriptCell> {
    messages
        .iter()
        .map(|message| cell_from_message(message, assistant_name))
        .collect()
}

pub fn cell_from_message(message: &ShellMessage, assistant_name: &str) -> TranscriptCell {
    match message {
        ShellMessage::UserMessage { content } => TranscriptCell {
            kind: TranscriptCellKind::User,
            title: "You".to_string(),
            subtitle: None,
            body: content.clone(),
            group: message.group(),
            is_active: false,
        },
        ShellMessage::AssistantMessage { content } => TranscriptCell {
            kind: TranscriptCellKind::Assistant,
            title: assistant_name.to_string(),
            subtitle: None,
            body: content.clone(),
            group: message.group(),
            is_active: false,
        },
        ShellMessage::AssistantStream { content } => TranscriptCell {
            kind: TranscriptCellKind::Assistant,
            title: assistant_name.to_string(),
            subtitle: Some("typing…".to_string()),
            body: content.clone(),
            group: message.group(),
            is_active: true,
        },
        ShellMessage::SystemMessage { content } => TranscriptCell {
            kind: TranscriptCellKind::System,
            title: "System".to_string(),
            subtitle: Some("context".to_string()),
            body: content.clone(),
            group: message.group(),
            is_active: false,
        },
        ShellMessage::ToolCall {
            call_id,
            name,
            arguments,
        } => TranscriptCell {
            kind: TranscriptCellKind::Tool,
            title: format!("Tool · {name}"),
            subtitle: Some(format!("#{call_id}")),
            body: arguments.clone(),
            group: message.group(),
            is_active: false,
        },
        ShellMessage::ToolResult {
            call_id,
            success,
            result,
        } => TranscriptCell {
            kind: TranscriptCellKind::Tool,
            title: if *success {
                "Tool Result".to_string()
            } else {
                "Tool Error".to_string()
            },
            subtitle: Some(format!("#{call_id}")),
            body: result.clone(),
            group: message.group(),
            is_active: false,
        },
        ShellMessage::TaskNotice { content } => TranscriptCell {
            kind: TranscriptCellKind::Notice,
            title: "Task".to_string(),
            subtitle: None,
            body: content.clone(),
            group: message.group(),
            is_active: false,
        },
        ShellMessage::ApprovalNotice {
            approval_id,
            content,
        } => TranscriptCell {
            kind: TranscriptCellKind::Notice,
            title: "Approval".to_string(),
            subtitle: approval_id.as_ref().map(|id| format!("#{id}")),
            body: content.clone(),
            group: message.group(),
            is_active: false,
        },
        ShellMessage::TeamNotice { content } => TranscriptCell {
            kind: TranscriptCellKind::Notice,
            title: "Team".to_string(),
            subtitle: None,
            body: content.clone(),
            group: message.group(),
            is_active: false,
        },
        ShellMessage::InfoNotice { content } => TranscriptCell {
            kind: TranscriptCellKind::Notice,
            title: "Info".to_string(),
            subtitle: None,
            body: content.clone(),
            group: message.group(),
            is_active: false,
        },
        ShellMessage::ErrorNotice { content } => TranscriptCell {
            kind: TranscriptCellKind::Notice,
            title: "Error".to_string(),
            subtitle: None,
            body: content.clone(),
            group: message.group(),
            is_active: false,
        },
    }
}

pub fn message_from_stream_frame(frame: &StreamFrame) -> Option<ShellMessage> {
    match frame {
        StreamFrame::Ack { .. } | StreamFrame::Data { .. } => None,
        StreamFrame::ToolCall {
            id,
            name,
            arguments,
        } => Some(ShellMessage::ToolCall {
            call_id: id.clone(),
            name: name.clone(),
            arguments: arguments.to_string(),
        }),
        StreamFrame::ToolResult {
            id,
            result,
            success,
        } => Some(ShellMessage::ToolResult {
            call_id: id.clone(),
            success: *success,
            result: result.clone(),
        }),
        StreamFrame::Error(error) => Some(ShellMessage::ErrorNotice {
            content: format!("Stream error {}: {}", error.code, error.message),
        }),
        StreamFrame::Start { .. } | StreamFrame::Event { .. } | StreamFrame::Done { .. } => None,
    }
}

pub fn message_from_session_event(event: &ChatSessionEvent) -> Option<ShellMessage> {
    match event {
        ChatSessionEvent::Created { .. }
        | ChatSessionEvent::Updated { .. }
        | ChatSessionEvent::MessageAdded { .. }
        | ChatSessionEvent::Deleted { .. } => None,
    }
}

pub fn message_from_task_event(event: &TaskStreamEvent) -> ShellMessage {
    let content = match &event.kind {
        StreamEventKind::Started {
            task_name,
            execution_mode,
            ..
        } => format!(
            "Task {} started: {task_name} via {execution_mode}",
            event.task_id
        ),
        StreamEventKind::Output {
            text,
            is_stderr,
            is_complete,
        } => {
            let stream = if *is_stderr { "stderr" } else { "stdout" };
            let suffix = if *is_complete { "" } else { " (partial)" };
            format!(
                "Task {} {stream}{suffix}: {}",
                event.task_id,
                text.trim_end()
            )
        }
        StreamEventKind::Progress {
            phase,
            percent,
            details,
        } => match (percent, details) {
            (Some(percent), Some(details)) => {
                format!("Task {} progress: {phase} ({percent}%) {details}", event.task_id)
            }
            (Some(percent), None) => {
                format!("Task {} progress: {phase} ({percent}%)", event.task_id)
            }
            (None, Some(details)) => {
                format!("Task {} progress: {phase} {details}", event.task_id)
            }
            (None, None) => format!("Task {} progress: {phase}", event.task_id),
        },
        StreamEventKind::Completed {
            result,
            duration_ms,
            ..
        } => format!(
            "Task {} completed in {} ms: {}",
            event.task_id,
            duration_ms,
            result.trim()
        ),
        StreamEventKind::Failed {
            error,
            error_code,
            duration_ms,
            recoverable,
        } => match error_code {
            Some(error_code) => format!(
                "Task {} failed in {} ms (recoverable={}): {} [{}]",
                event.task_id, duration_ms, recoverable, error, error_code
            ),
            None => format!(
                "Task {} failed in {} ms (recoverable={}): {}",
                event.task_id, duration_ms, recoverable, error
            ),
        },
        StreamEventKind::Interrupted {
            reason,
            duration_ms,
        } => format!(
            "Task {} interrupted after {} ms: {}",
            event.task_id, duration_ms, reason
        ),
        StreamEventKind::Heartbeat { elapsed_ms } => {
            format!("Task {} heartbeat at {} ms", event.task_id, elapsed_ms)
        }
    };
    ShellMessage::TaskNotice { content }
}

pub fn message_from_team_message(message: &TeamMessage) -> ShellMessage {
    match message.kind {
        TeamMessageKind::ApprovalRequest => ShellMessage::ApprovalNotice {
            approval_id: extract_approval_id(&message.content),
            content: format!("{} requested approval: {}", message.from_member_id, message.content),
        },
        TeamMessageKind::ApprovalResolution => ShellMessage::TeamNotice {
            content: format!("Approval resolved by {}: {}", message.from_member_id, message.content),
        },
        TeamMessageKind::Assignment => ShellMessage::TeamNotice {
            content: format!(
                "Assignment from {} to {:?}: {}",
                message.from_member_id, message.to_member_id, message.content
            ),
        },
        TeamMessageKind::Note => ShellMessage::TeamNotice {
            content: format!("{}: {}", message.from_member_id, message.content),
        },
    }
}

fn extract_approval_id(content: &str) -> Option<String> {
    content
        .split_whitespace()
        .last()
        .map(|segment| segment.trim_matches(|ch| ch == '(' || ch == ')'))
        .filter(|segment| !segment.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::{
        MessageGroup, ShellMessage, TranscriptCellKind, cell_from_message,
        message_from_session_event, message_from_stream_frame, message_from_task_event,
        messages_from_session, transcript_cells,
    };
    use restflow_core::daemon::{ChatSessionEvent, StreamFrame};
    use restflow_core::models::{ChatMessage, ChatSession};
    use restflow_core::runtime::TaskStreamEvent;

    #[test]
    fn appends_and_finalizes_assistant_stream() {
        let mut cell = cell_from_message(
            &ShellMessage::AssistantStream {
                content: "hel".to_string(),
            },
            "Agent",
        );
        assert!(cell.append_chunk("lo"));
        assert!(cell.finalize());
        assert_eq!(cell.kind, TranscriptCellKind::Assistant);
        assert_eq!(cell.body, "hello");
        assert!(!cell.is_active);
        assert!(cell.subtitle.is_none());
    }

    #[test]
    fn maps_session_messages_to_typed_entries() {
        let mut session = ChatSession::new("agent-1".to_string(), "model".to_string());
        session.messages = vec![
            ChatMessage::user("hello"),
            ChatMessage::assistant("hi"),
            ChatMessage::system("stay focused"),
        ];

        let transcript = messages_from_session(&session);
        assert_eq!(transcript.len(), 3);
        assert!(matches!(transcript[0], ShellMessage::UserMessage { .. }));
        assert!(matches!(transcript[1], ShellMessage::AssistantMessage { .. }));
        assert!(matches!(transcript[2], ShellMessage::SystemMessage { .. }));
    }

    #[test]
    fn task_progress_is_projected_to_task_notice() {
        let event = TaskStreamEvent::progress(
            "task-1",
            "Compiling",
            Some(50),
            Some("main.rs".to_string()),
        );
        let message = message_from_task_event(&event);
        assert!(matches!(message, ShellMessage::TaskNotice { .. }));
    }

    #[test]
    fn identifies_session_projection_messages() {
        assert!(cell_from_message(
            &ShellMessage::UserMessage {
                content: "hi".to_string()
            },
            "Agent"
        )
        .is_conversation_cell());
        assert!(cell_from_message(
            &ShellMessage::AssistantStream {
                content: "chunk".to_string()
            },
            "Agent"
        )
        .is_conversation_cell());
        assert!(!cell_from_message(
            &ShellMessage::InfoNotice {
                content: "note".to_string()
            },
            "Agent"
        )
        .is_conversation_cell());
    }

    #[test]
    fn groups_messages_by_visual_family() {
        assert_eq!(
            ShellMessage::ToolCall {
                call_id: "1".to_string(),
                name: "bash".to_string(),
                arguments: "{}".to_string(),
            }
            .group(),
            MessageGroup::ToolActivity
        );
        assert_eq!(
            ShellMessage::ErrorNotice {
                content: "boom".to_string()
            }
            .group(),
            MessageGroup::RuntimeNotice
        );
    }

    #[test]
    fn creates_active_cell_for_streaming_assistant_message() {
        let cells = transcript_cells(
            &[ShellMessage::AssistantStream {
                content: "chunk".to_string(),
            }],
            "RestFlow",
        );
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].kind, TranscriptCellKind::Assistant);
        assert!(cells[0].is_active);
        assert_eq!(cells[0].title, "RestFlow");
    }

    #[test]
    fn suppresses_ack_and_data_in_message_projection() {
        assert!(message_from_stream_frame(&StreamFrame::Ack {
            content: "working".to_string(),
        })
        .is_none());
        assert!(message_from_stream_frame(&StreamFrame::Data {
            content: "body".to_string(),
        })
        .is_none());
    }

    #[test]
    fn suppresses_session_events_in_main_transcript() {
        let event = ChatSessionEvent::MessageAdded {
            session_id: "session-1".to_string(),
            source: "ipc".to_string(),
        };
        assert!(message_from_session_event(&event).is_none());
    }
}
