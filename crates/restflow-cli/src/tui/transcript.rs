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
    StreamAck { content: String },
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
    SessionNotice { content: String },
    TaskNotice { content: String },
    ApprovalNotice {
        approval_id: Option<String>,
        content: String,
    },
    TeamNotice { content: String },
    InfoNotice { content: String },
    ErrorNotice { content: String },
}

impl ShellMessage {
    pub fn is_session_projection(&self) -> bool {
        matches!(
            self,
            Self::UserMessage { .. }
                | Self::AssistantMessage { .. }
                | Self::SystemMessage { .. }
                | Self::AssistantStream { .. }
        )
    }

    pub fn append_assistant_chunk(&mut self, chunk: &str) -> bool {
        match self {
            Self::AssistantStream { content } => {
                content.push_str(chunk);
                true
            }
            _ => false,
        }
    }

    pub fn finalize_stream(&mut self) -> bool {
        match self {
            Self::AssistantStream { content } => {
                let completed = std::mem::take(content);
                *self = Self::AssistantMessage { content: completed };
                true
            }
            _ => false,
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

pub fn message_from_stream_frame(frame: &StreamFrame) -> Option<ShellMessage> {
    match frame {
        StreamFrame::Ack { content } => Some(ShellMessage::StreamAck {
            content: content.clone(),
        }),
        StreamFrame::Data { content } => Some(ShellMessage::AssistantStream {
            content: content.clone(),
        }),
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

pub fn message_from_session_event(event: &ChatSessionEvent) -> ShellMessage {
    let content = match event {
        ChatSessionEvent::Created { session_id } => format!("Session created: {session_id}"),
        ChatSessionEvent::Updated { session_id } => format!("Session updated: {session_id}"),
        ChatSessionEvent::MessageAdded { session_id, source } => {
            format!("Message added to {session_id} from {source}")
        }
        ChatSessionEvent::Deleted { session_id } => format!("Session deleted: {session_id}"),
    };
    ShellMessage::SessionNotice { content }
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
    use super::{ShellMessage, message_from_task_event, messages_from_session};
    use restflow_core::models::{ChatMessage, ChatSession};
    use restflow_core::runtime::TaskStreamEvent;

    #[test]
    fn appends_and_finalizes_assistant_stream() {
        let mut message = ShellMessage::AssistantStream {
            content: "hel".to_string(),
        };
        assert!(message.append_assistant_chunk("lo"));
        assert!(message.finalize_stream());
        assert_eq!(
            message,
            ShellMessage::AssistantMessage {
                content: "hello".to_string()
            }
        );
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
        assert!(ShellMessage::UserMessage {
            content: "hi".to_string()
        }
        .is_session_projection());
        assert!(ShellMessage::AssistantStream {
            content: "chunk".to_string()
        }
        .is_session_projection());
        assert!(!ShellMessage::InfoNotice {
            content: "note".to_string()
        }
        .is_session_projection());
    }
}
