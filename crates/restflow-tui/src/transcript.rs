use restflow_contracts::{ChatSessionEvent, StreamEventKind, StreamFrame, TaskStreamEvent};
use restflow_core::models::{ChatRole, ChatSession, ChatTurnEventKind, ChatTurnStatus};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellMessage {
    UserMessage {
        content: String,
    },
    AssistantMessage {
        content: String,
    },
    SystemMessage {
        content: String,
    },
    AssistantStream {
        content: String,
    },
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
    TaskNotice {
        content: String,
    },
    InfoNotice {
        content: String,
    },
    ErrorNotice {
        content: String,
    },
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
    Subagent,
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

    #[cfg(test)]
    pub fn append_chunk(&mut self, chunk: &str) -> bool {
        match self.kind {
            TranscriptCellKind::Assistant if self.is_active => {
                let normalized_chunk = chunk.trim_start_matches(['\r', '\n']);
                if chunk == self.body
                    || normalized_chunk == self.body
                    || (!normalized_chunk.trim().is_empty()
                        && normalized_chunk.trim() == self.body.trim())
                    || self.body.starts_with(chunk)
                    || self.body.starts_with(normalized_chunk)
                {
                    return true;
                }
                if chunk.starts_with(&self.body) {
                    self.body = chunk.to_string();
                    return true;
                }
                if normalized_chunk.starts_with(&self.body) {
                    self.body = normalized_chunk.to_string();
                    return true;
                }
                self.body.push_str(chunk);
                true
            }
            _ => false,
        }
    }

    pub fn finalize(&mut self) -> bool {
        match self.kind {
            TranscriptCellKind::Assistant
            | TranscriptCellKind::Tool
            | TranscriptCellKind::Subagent
                if self.is_active =>
            {
                self.is_active = false;
                if self.kind == TranscriptCellKind::Assistant {
                    self.subtitle = None;
                } else if let Some(call_id) = self.tool_call_id().map(ToOwned::to_owned) {
                    self.subtitle = Some(format!("#{call_id}"));
                }
                true
            }
            _ => false,
        }
    }

    pub fn tool_call_id(&self) -> Option<&str> {
        if !matches!(
            self.kind,
            TranscriptCellKind::Tool | TranscriptCellKind::Subagent
        ) {
            return None;
        }
        let subtitle = self.subtitle.as_deref()?.strip_prefix('#')?;
        Some(subtitle.split(" · ").next().unwrap_or(subtitle).trim())
    }

    pub fn merge_tool_result(&mut self, success: bool, result: &str) -> bool {
        if !matches!(
            self.kind,
            TranscriptCellKind::Tool | TranscriptCellKind::Subagent
        ) {
            return false;
        }
        let call_id = self.tool_call_id().map(ToOwned::to_owned);
        let label = if success { "Output" } else { "Error" };
        let rendered_result = if self.kind == TranscriptCellKind::Subagent {
            summarize_team_tool_result(success, result)
        } else {
            result.trim().to_string()
        };
        let marker = format!("\n{label}:");
        let base = self
            .body
            .find("\nOutput:")
            .or_else(|| self.body.find("\nError:"))
            .map(|index| self.body[..index].trim_end().to_string())
            .unwrap_or_else(|| self.body.trim_end().to_string());
        self.body = if base.is_empty() {
            format!("{label}: {rendered_result}")
        } else {
            format!("{base}{marker} {rendered_result}")
        };
        self.is_active = false;
        if let Some(call_id) = call_id {
            self.subtitle = Some(format!("#{call_id}"));
        }
        true
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
            Self::TaskNotice { .. } | Self::InfoNotice { .. } | Self::ErrorNotice { .. } => {
                MessageGroup::RuntimeNotice
            }
        }
    }
}

pub fn messages_from_session(session: &ChatSession) -> Vec<ShellMessage> {
    if session.turns.iter().any(|turn| !turn.events.is_empty()) {
        return session
            .turns
            .iter()
            .flat_map(|turn| {
                let hide_team_activity = matches!(
                    turn.status,
                    ChatTurnStatus::Completed | ChatTurnStatus::Canceled | ChatTurnStatus::Failed
                );
                let team_call_ids = if hide_team_activity {
                    turn.events
                        .iter()
                        .filter_map(|event| match &event.kind {
                            ChatTurnEventKind::ToolCall { call_id, name, .. }
                                if is_team_tool_name(name) =>
                            {
                                Some(call_id.clone())
                            }
                            _ => None,
                        })
                        .collect::<HashSet<_>>()
                } else {
                    HashSet::new()
                };
                turn.events
                    .iter()
                    .filter_map(move |event| match &event.kind {
                        ChatTurnEventKind::UserMessage { content } => {
                            Some(ShellMessage::UserMessage {
                                content: content.clone(),
                            })
                        }
                        ChatTurnEventKind::AssistantMessage { content } => {
                            Some(ShellMessage::AssistantMessage {
                                content: content.clone(),
                            })
                        }
                        ChatTurnEventKind::ToolCall { name, .. }
                            if hide_team_activity && is_team_tool_name(name) =>
                        {
                            None
                        }
                        ChatTurnEventKind::ToolCall {
                            call_id,
                            name,
                            arguments,
                        } => Some(ShellMessage::ToolCall {
                            call_id: call_id.clone(),
                            name: name.clone(),
                            arguments: arguments.clone(),
                        }),
                        ChatTurnEventKind::ToolResult {
                            call_id,
                            success,
                            result,
                        } if hide_team_activity && team_call_ids.contains(call_id) => None,
                        ChatTurnEventKind::ToolResult {
                            call_id,
                            success,
                            result,
                        } => Some(ShellMessage::ToolResult {
                            call_id: call_id.clone(),
                            success: *success,
                            result: result.clone(),
                        }),
                        ChatTurnEventKind::Error { message } => Some(ShellMessage::ErrorNotice {
                            content: message.clone(),
                        }),
                        ChatTurnEventKind::Canceled => Some(ShellMessage::InfoNotice {
                            content: "Turn canceled".to_string(),
                        }),
                    })
            })
            .collect();
    }

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

fn is_team_tool_name(name: &str) -> bool {
    matches!(
        name,
        "spawn_subagent_batch" | "spawn_subagent" | "wait_subagents"
    )
}

pub fn transcript_cells(messages: &[ShellMessage], assistant_name: &str) -> Vec<TranscriptCell> {
    let mut cells: Vec<TranscriptCell> = Vec::new();
    for message in messages {
        if let ShellMessage::ToolResult {
            call_id,
            success,
            result,
        } = message
            && let Some(cell) = cells
                .iter_mut()
                .rev()
                .find(|cell| cell.tool_call_id() == Some(call_id.as_str()))
        {
            let _ = cell.merge_tool_result(*success, result);
            continue;
        }
        cells.push(cell_from_message(message, assistant_name));
    }
    cells
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
        } => {
            let is_team_tool = matches!(
                name.as_str(),
                "spawn_subagent_batch" | "spawn_subagent" | "wait_subagents"
            );
            TranscriptCell {
                kind: if is_team_tool {
                    TranscriptCellKind::Subagent
                } else {
                    TranscriptCellKind::Tool
                },
                title: if is_team_tool {
                    "Subagent".to_string()
                } else {
                    format!("Tool · {name}")
                },
                subtitle: Some(format!("#{call_id}")),
                body: if is_team_tool {
                    summarize_team_tool_call(name, arguments)
                } else {
                    format!("Input: {}", arguments.trim())
                },
                group: message.group(),
                is_active: false,
            }
        }
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
            body: if *success {
                format!("Output: {}", result.trim())
            } else {
                format!("Error: {}", result.trim())
            },
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

fn summarize_team_tool_call(name: &str, arguments: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(arguments) else {
        return format!("Starting team\nInput: {}", arguments.trim());
    };

    if name == "wait_subagents" {
        let count = value
            .get("task_ids")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0);
        return format!("Waiting for {count} subagent{}", plural_suffix(count));
    }

    let mut specs = value
        .get("specs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if specs.is_empty() && name == "spawn_subagent" {
        specs.push(value.clone());
    }
    let mut count = 0usize;
    let mut lines = Vec::new();
    for (index, spec) in specs.iter().enumerate() {
        let spec_count = spec
            .get("tasks")
            .and_then(Value::as_array)
            .map(Vec::len)
            .or_else(|| {
                spec.get("count")
                    .and_then(Value::as_u64)
                    .and_then(|value| usize::try_from(value).ok())
            })
            .unwrap_or(1);
        count += spec_count;

        let name = spec
            .get("inline_name")
            .or_else(|| spec.get("agent"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("worker {}", index + 1));
        let task = spec
            .get("task")
            .or_else(|| {
                spec.get("tasks")
                    .and_then(Value::as_array)
                    .and_then(|tasks| tasks.first())
            })
            .or_else(|| value.get("task"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("delegated work");
        lines.push(format!("- {name}: {}", truncate_for_notice(task, 90)));
    }

    if count == 0 {
        count = 1;
    }
    let mut body = vec![format!("Starting {count} subagent{}", plural_suffix(count))];
    body.extend(lines.into_iter().take(6));
    body.join("\n")
}

fn summarize_team_tool_result(success: bool, result: &str) -> String {
    if !success {
        return result.trim().to_string();
    }
    let Ok(value) = serde_json::from_str::<Value>(result) else {
        return result.trim().to_string();
    };

    let status = value
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("completed");
    let count = value
        .get("spawned_count")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .or_else(|| {
            value
                .get("task_ids")
                .and_then(Value::as_array)
                .map(Vec::len)
        })
        .or_else(|| value.get("results").and_then(Value::as_array).map(Vec::len))
        .or_else(|| {
            if value.get("task_id").is_some()
                || value.get("output").is_some()
                || value.get("agent").is_some()
            {
                Some(1)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let mut lines = vec![format!("{status} {count} subagent{}", plural_suffix(count))];

    if let Some(results) = value.get("results").and_then(Value::as_array) {
        for item in results.iter().take(6) {
            push_subagent_result_line(&mut lines, item);
        }
    } else if value.get("task_id").is_some()
        || value.get("output").is_some()
        || value.get("agent").is_some()
    {
        push_subagent_result_line(&mut lines, &value);
    }

    lines.join("\n")
}

fn push_subagent_result_line(lines: &mut Vec<String>, item: &Value) {
    let task_id = item
        .get("task_id")
        .and_then(Value::as_str)
        .map(short_id)
        .or_else(|| {
            item.get("agent")
                .and_then(Value::as_str)
                .map(|value| truncate_for_notice(value.trim(), 24))
        })
        .unwrap_or_else(|| "subagent".to_string());
    let item_status = item
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("completed");
    let output = item
        .get("output")
        .or_else(|| item.get("error"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(item_status);
    lines.push(format!("- {task_id}: {}", truncate_for_notice(output, 90)));
}

fn plural_suffix(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

fn short_id(value: &str) -> String {
    value.chars().take(8).collect()
}

fn truncate_for_notice(value: &str, max_chars: usize) -> String {
    let mut text = value.replace('\n', " ");
    if text.chars().count() <= max_chars {
        return text;
    }
    text = text.chars().take(max_chars.saturating_sub(1)).collect();
    text.push('…');
    text
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
                format!(
                    "Task {} progress: {phase} ({percent}%) {details}",
                    event.task_id
                )
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

#[cfg(test)]
mod tests {
    use super::{
        MessageGroup, ShellMessage, TranscriptCellKind, cell_from_message,
        message_from_session_event, message_from_stream_frame, message_from_task_event,
        messages_from_session, transcript_cells,
    };
    use restflow_contracts::{ChatSessionEvent, StreamFrame, TaskStreamEvent};
    use restflow_core::models::{ChatMessage, ChatSession, ChatTurnEventKind};

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
    fn stream_append_ignores_duplicate_full_chunk() {
        let mut cell = cell_from_message(
            &ShellMessage::AssistantStream {
                content: "OK".to_string(),
            },
            "Agent",
        );

        assert!(cell.append_chunk("OK"));

        assert_eq!(cell.body, "OK");
    }

    #[test]
    fn stream_append_ignores_duplicate_payload_with_leading_newlines() {
        let mut cell = cell_from_message(
            &ShellMessage::AssistantStream {
                content: "OK".to_string(),
            },
            "Agent",
        );

        assert!(cell.append_chunk("\n\nOK"));

        assert_eq!(cell.body, "OK");
    }

    #[test]
    fn stream_append_replaces_with_cumulative_chunk() {
        let mut cell = cell_from_message(
            &ShellMessage::AssistantStream {
                content: "hello".to_string(),
            },
            "Agent",
        );

        assert!(cell.append_chunk("hello world"));

        assert_eq!(cell.body, "hello world");
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
        assert!(matches!(
            transcript[1],
            ShellMessage::AssistantMessage { .. }
        ));
        assert!(matches!(transcript[2], ShellMessage::SystemMessage { .. }));
    }

    #[test]
    fn maps_session_turn_events_before_legacy_messages() {
        let mut session = ChatSession::new("agent-1".to_string(), "model".to_string());
        session.add_message(ChatMessage::user("legacy user"));
        session.add_message(ChatMessage::assistant("legacy assistant"));
        session.record_turn_user_message("turn-1", "hello");
        session.record_turn_event(
            "turn-1",
            ChatTurnEventKind::ToolCall {
                call_id: "call-1".to_string(),
                name: "bash".to_string(),
                arguments: "pwd".to_string(),
            },
        );
        session.record_turn_event(
            "turn-1",
            ChatTurnEventKind::ToolResult {
                call_id: "call-1".to_string(),
                success: true,
                result: "/tmp".to_string(),
            },
        );
        session.complete_turn_with_assistant_message("turn-1", "done");

        let transcript = messages_from_session(&session);
        assert_eq!(transcript.len(), 4);
        assert!(matches!(transcript[0], ShellMessage::UserMessage { .. }));
        assert!(matches!(transcript[1], ShellMessage::ToolCall { .. }));
        assert!(matches!(transcript[2], ShellMessage::ToolResult { .. }));
        assert!(matches!(
            transcript[3],
            ShellMessage::AssistantMessage { .. }
        ));
    }

    #[test]
    fn messages_from_session_hides_completed_team_activity() {
        let mut session = ChatSession::new("agent-1".to_string(), "model".to_string());
        session.record_turn_user_message("turn-1", "coordinate team");
        session.record_turn_event(
            "turn-1",
            ChatTurnEventKind::ToolCall {
                call_id: "call-team".to_string(),
                name: "spawn_subagent_batch".to_string(),
                arguments: serde_json::json!({"specs":[{"task":"reply ok"}]}).to_string(),
            },
        );
        session.record_turn_event(
            "turn-1",
            ChatTurnEventKind::ToolResult {
                call_id: "call-team".to_string(),
                success: true,
                result: serde_json::json!({"status":"completed"}).to_string(),
            },
        );
        session.complete_turn_with_assistant_message("turn-1", "team done");

        let transcript = messages_from_session(&session);

        assert_eq!(transcript.len(), 2);
        assert!(matches!(transcript[0], ShellMessage::UserMessage { .. }));
        assert!(matches!(
            transcript[1],
            ShellMessage::AssistantMessage { .. }
        ));
    }

    #[test]
    fn transcript_cells_merge_tool_result_into_tool_call() {
        let cells = transcript_cells(
            &[
                ShellMessage::UserMessage {
                    content: "run pwd".to_string(),
                },
                ShellMessage::ToolCall {
                    call_id: "call-1".to_string(),
                    name: "bash".to_string(),
                    arguments: "{\"command\":\"pwd\"}".to_string(),
                },
                ShellMessage::ToolResult {
                    call_id: "call-1".to_string(),
                    success: true,
                    result: "/tmp".to_string(),
                },
                ShellMessage::AssistantMessage {
                    content: "done".to_string(),
                },
            ],
            "Agent",
        );

        assert_eq!(cells.len(), 3);
        assert_eq!(cells[1].kind, TranscriptCellKind::Tool);
        assert_eq!(cells[1].title, "Tool · bash");
        assert_eq!(cells[1].tool_call_id(), Some("call-1"));
        assert!(cells[1].body.contains("Input:"));
        assert!(cells[1].body.contains("Output: /tmp"));
    }

    #[test]
    fn transcript_cells_render_subagent_batch_as_team_block() {
        let cells = transcript_cells(
            &[
                ShellMessage::ToolCall {
                    call_id: "call-team".to_string(),
                    name: "spawn_subagent_batch".to_string(),
                    arguments: serde_json::json!({
                        "specs": [
                            {
                                "inline_name": "Team A",
                                "tasks": ["return TEAM_A_OK"]
                            },
                            {
                                "inline_name": "Team B",
                                "tasks": ["return TEAM_B_OK"]
                            }
                        ],
                        "wait": true
                    })
                    .to_string(),
                },
                ShellMessage::ToolResult {
                    call_id: "call-team".to_string(),
                    success: true,
                    result: serde_json::json!({
                        "status": "completed",
                        "spawned_count": 2,
                        "results": [
                            {
                                "task_id": "82f897d4-582c",
                                "status": "completed",
                                "output": "TEAM_A_OK"
                            },
                            {
                                "task_id": "c7bf01aa-1234",
                                "status": "completed",
                                "output": "TEAM_B_OK"
                            }
                        ]
                    })
                    .to_string(),
                },
            ],
            "Agent",
        );

        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].kind, TranscriptCellKind::Subagent);
        assert_eq!(cells[0].title, "Subagent");
        assert!(cells[0].body.contains("Starting 2 subagents"));
        assert!(cells[0].body.contains("- Team A: return TEAM_A_OK"));
        assert!(cells[0].body.contains("Output: completed 2 subagents"));
        assert!(cells[0].body.contains("- 82f897d4: TEAM_A_OK"));
        assert!(!cells[0].body.contains("\"specs\""));
    }

    #[test]
    fn transcript_cells_render_single_subagent_result_count() {
        let cells = transcript_cells(
            &[
                ShellMessage::ToolCall {
                    call_id: "call-team".to_string(),
                    name: "spawn_subagent".to_string(),
                    arguments: serde_json::json!({
                        "inline_name": "panel-check",
                        "task": "reply SUBAGENT_PANEL_OK",
                        "wait": true
                    })
                    .to_string(),
                },
                ShellMessage::ToolResult {
                    call_id: "call-team".to_string(),
                    success: true,
                    result: serde_json::json!({
                        "agent": "panel-check",
                        "duration_ms": 6246,
                        "output": "SUBAGENT_PANEL_OK",
                        "status": "completed",
                        "task_id": "3f181f52-c11e-4338-823a-b7a1855f7159"
                    })
                    .to_string(),
                },
            ],
            "Agent",
        );

        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].kind, TranscriptCellKind::Subagent);
        assert_eq!(cells[0].title, "Subagent");
        assert!(cells[0].body.contains("Starting 1 subagent"));
        assert!(
            cells[0]
                .body
                .contains("- panel-check: reply SUBAGENT_PANEL_OK")
        );
        assert!(cells[0].body.contains("Output: completed 1 subagent"));
        assert!(cells[0].body.contains("- 3f181f52: SUBAGENT_PANEL_OK"));
        assert!(!cells[0].body.contains("completed 0 subagents"));
    }

    #[test]
    fn transcript_cells_render_wait_subagents_as_team_block() {
        let cells = transcript_cells(
            &[
                ShellMessage::ToolCall {
                    call_id: "call-wait".to_string(),
                    name: "wait_subagents".to_string(),
                    arguments: serde_json::json!({
                        "task_ids": ["task-a", "task-b"],
                        "timeout_secs": 30
                    })
                    .to_string(),
                },
                ShellMessage::ToolResult {
                    call_id: "call-wait".to_string(),
                    success: true,
                    result: serde_json::json!({
                        "results": [
                            {
                                "task_id": "task-a",
                                "status": "completed",
                                "output": "TEAM_A_OK"
                            },
                            {
                                "task_id": "task-b",
                                "status": "completed",
                                "output": "TEAM_B_OK"
                            }
                        ]
                    })
                    .to_string(),
                },
            ],
            "Agent",
        );

        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].kind, TranscriptCellKind::Subagent);
        assert_eq!(cells[0].title, "Subagent");
        assert!(cells[0].body.contains("Waiting for 2 subagents"));
        assert!(cells[0].body.contains("Output: completed 2 subagents"));
        assert!(cells[0].body.contains("- task-a: TEAM_A_OK"));
    }

    #[test]
    fn task_progress_is_projected_to_task_notice() {
        let event =
            TaskStreamEvent::progress("task-1", "Compiling", Some(50), Some("main.rs".to_string()));
        let message = message_from_task_event(&event);
        assert!(matches!(message, ShellMessage::TaskNotice { .. }));
    }

    #[test]
    fn identifies_session_projection_messages() {
        assert!(
            cell_from_message(
                &ShellMessage::UserMessage {
                    content: "hi".to_string()
                },
                "Agent"
            )
            .is_conversation_cell()
        );
        assert!(
            cell_from_message(
                &ShellMessage::AssistantStream {
                    content: "chunk".to_string()
                },
                "Agent"
            )
            .is_conversation_cell()
        );
        assert!(
            !cell_from_message(
                &ShellMessage::InfoNotice {
                    content: "note".to_string()
                },
                "Agent"
            )
            .is_conversation_cell()
        );
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
        assert!(
            message_from_stream_frame(&StreamFrame::Ack {
                content: "working".to_string(),
            })
            .is_none()
        );
        assert!(
            message_from_stream_frame(&StreamFrame::Data {
                content: "body".to_string(),
            })
            .is_none()
        );
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
