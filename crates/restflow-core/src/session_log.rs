//! File-backed session transcripts.
//!
//! The JSONL transcript is the durable session source of truth. Each session is
//! stored as one file under `~/.restflow/sessions/YYYY/MM/DD/<session-id>.jsonl`.

use crate::models::chat_session::{
    ChatMessage, ChatMessageMedia, ChatMessageTranscript, ChatRole, ChatSession,
    ChatSessionMetadata, ChatSessionSource, ChatSessionSummary, MessageExecution,
};
use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Datelike, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;
use walkdir::WalkDir;

pub const SESSION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionMessageRole {
    User,
    Assistant,
    System,
}

impl SessionMessageRole {
    fn as_chat_role(&self) -> ChatRole {
        match self {
            Self::User => ChatRole::User,
            Self::Assistant => ChatRole::Assistant,
            Self::System => ChatRole::System,
        }
    }

    fn from_chat_role(role: &ChatRole) -> Self {
        match role {
            ChatRole::User => Self::User,
            ChatRole::Assistant => Self::Assistant,
            ChatRole::System => Self::System,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsageValues {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionLogEvent {
    SessionMeta {
        schema_version: u32,
        id: String,
        created_at: String,
        updated_at: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        provider: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        app_version: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        git_branch: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        skill_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retention: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary_message_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_channel: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_conversation_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        archived_at: Option<String>,
    },
    Message {
        id: String,
        time: String,
        role: SessionMessageRole,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        execution: Option<MessageExecution>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        media: Option<ChatMessageMedia>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        transcript: Option<ChatMessageTranscript>,
    },
    Reasoning {
        id: String,
        time: String,
        text: String,
    },
    ToolCall {
        id: String,
        time: String,
        tool: String,
        #[serde(default)]
        input: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
    ToolResult {
        id: String,
        time: String,
        tool: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
    Compact {
        id: String,
        time: String,
        summary: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        auto: Option<bool>,
    },
    Usage {
        time: String,
        #[serde(flatten)]
        usage: UsageValues,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionMeta {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub app_version: Option<String>,
    pub git_branch: Option<String>,
    pub agent_id: Option<String>,
    pub skill_id: Option<String>,
    pub retention: Option<String>,
    pub summary_message_id: Option<String>,
    pub source_channel: Option<String>,
    pub source_conversation_id: Option<String>,
    pub archived_at: Option<String>,
}

impl SessionMeta {
    pub fn new(id: String, created_at: String, updated_at: String) -> Self {
        Self {
            id,
            created_at,
            updated_at,
            title: None,
            cwd: None,
            model: None,
            provider: None,
            app_version: None,
            git_branch: None,
            agent_id: None,
            skill_id: None,
            retention: None,
            summary_message_id: None,
            source_channel: None,
            source_conversation_id: None,
            archived_at: None,
        }
    }

    pub fn into_event(self) -> SessionLogEvent {
        SessionLogEvent::SessionMeta {
            schema_version: SESSION_SCHEMA_VERSION,
            id: self.id,
            created_at: self.created_at,
            updated_at: self.updated_at,
            title: self.title,
            cwd: self.cwd,
            model: self.model,
            provider: self.provider,
            app_version: self.app_version,
            git_branch: self.git_branch,
            agent_id: self.agent_id,
            skill_id: self.skill_id,
            retention: self.retention,
            summary_message_id: self.summary_message_id,
            source_channel: self.source_channel,
            source_conversation_id: self.source_conversation_id,
            archived_at: self.archived_at,
        }
    }
}

impl TryFrom<&SessionLogEvent> for SessionMeta {
    type Error = anyhow::Error;

    fn try_from(event: &SessionLogEvent) -> Result<Self> {
        match event {
            SessionLogEvent::SessionMeta {
                id,
                created_at,
                updated_at,
                title,
                cwd,
                model,
                provider,
                app_version,
                git_branch,
                agent_id,
                skill_id,
                retention,
                summary_message_id,
                source_channel,
                source_conversation_id,
                archived_at,
                ..
            } => Ok(Self {
                id: id.clone(),
                created_at: created_at.clone(),
                updated_at: updated_at.clone(),
                title: title.clone(),
                cwd: cwd.clone(),
                model: model.clone(),
                provider: provider.clone(),
                app_version: app_version.clone(),
                git_branch: git_branch.clone(),
                agent_id: agent_id.clone(),
                skill_id: skill_id.clone(),
                retention: retention.clone(),
                summary_message_id: summary_message_id.clone(),
                source_channel: source_channel.clone(),
                source_conversation_id: source_conversation_id.clone(),
                archived_at: archived_at.clone(),
            }),
            _ => Err(anyhow!("first session line is not session_meta")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileSession {
    pub meta: SessionMeta,
    pub events: Vec<SessionLogEvent>,
}

impl FileSession {
    pub fn new(meta: SessionMeta, events: Vec<SessionLogEvent>) -> Self {
        Self { meta, events }
    }

    pub fn from_events(mut events: Vec<SessionLogEvent>) -> Result<Self> {
        let first = events
            .first()
            .ok_or_else(|| anyhow!("session transcript is empty"))?;
        SessionMeta::try_from(first)?;
        if let Some(last) = latest_event_time(&events) {
            meta_updated_in_place(&mut events, &last);
        }
        let meta = SessionMeta::try_from(
            events
                .first()
                .ok_or_else(|| anyhow!("session transcript is empty"))?,
        )?;
        Ok(Self { meta, events })
    }

    pub fn from_chat_session(session: &ChatSession) -> Self {
        let created_at = iso_from_millis(session.created_at);
        let updated_at = iso_from_millis(session.updated_at);
        let mut meta = SessionMeta::new(session.id.clone(), created_at, updated_at);
        meta.title = Some(session.name.clone());
        meta.model = Some(session.model.clone());
        meta.provider = Some(session.provider.clone()).filter(|value| !value.trim().is_empty());
        meta.agent_id = Some(session.agent_id.clone());
        meta.skill_id = session.skill_id.clone();
        meta.retention = session.retention.clone();
        meta.summary_message_id = session.summary_message_id.clone();
        meta.source_channel = session
            .source_channel
            .map(|source| session_source_to_str(source).to_string());
        meta.source_conversation_id = session.source_conversation_id.clone();
        meta.archived_at = session.archived_at.map(iso_from_millis);

        let mut events = vec![meta.clone().into_event()];
        for message in &session.messages {
            events.push(message_event_from_chat_message(message));
        }

        Self { meta, events }
    }

    pub fn merge_chat_session(existing: Option<&FileSession>, session: &ChatSession) -> Self {
        let mut next = FileSession::from_chat_session(session);
        let Some(existing) = existing else {
            return next;
        };

        let next_messages = session
            .messages
            .iter()
            .map(|message| (message.id.clone(), message))
            .collect::<HashMap<_, _>>();
        let mut remaining_messages = next_messages.clone();
        let mut merged_events = vec![next.meta.clone().into_event()];

        for event in existing.events.iter().skip(1) {
            if let Some(id) = event_id(event)
                && let Some(message) = next_messages.get(id)
            {
                if matches!(event, SessionLogEvent::Message { .. }) {
                    merged_events.push(message_event_from_chat_message(message));
                } else {
                    merged_events.push(event.clone());
                }
                remaining_messages.remove(id);
                continue;
            }
            merged_events.push(event.clone());
        }

        for event in next.events.drain(1..) {
            if let SessionLogEvent::Message { id, .. } = &event
                && remaining_messages.remove(id).is_some()
            {
                merged_events.push(event);
            }
        }

        next.events = merged_events;
        next
    }

    pub fn to_chat_session(&self) -> ChatSession {
        let mut session = ChatSession {
            id: self.meta.id.clone(),
            name: self
                .meta
                .title
                .clone()
                .unwrap_or_else(|| "Imported Chat".to_string()),
            agent_id: self
                .meta
                .agent_id
                .clone()
                .unwrap_or_else(|| "default".to_string()),
            provider: self.meta.provider.clone().unwrap_or_default(),
            model: self
                .meta
                .model
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            messages: Vec::new(),
            created_at: parse_time_ms(&self.meta.created_at),
            updated_at: parse_time_ms(&self.meta.updated_at),
            skill_id: self.meta.skill_id.clone(),
            retention: self.meta.retention.clone(),
            summary_message_id: self.meta.summary_message_id.clone(),
            prompt_tokens: 0,
            completion_tokens: 0,
            cost: 0.0,
            metadata: ChatSessionMetadata::new(),
            source_channel: self
                .meta
                .source_channel
                .as_deref()
                .and_then(session_source_from_str),
            source_conversation_id: self.meta.source_conversation_id.clone(),
            archived_at: self.meta.archived_at.as_deref().map(parse_time_ms),
        };

        for event in &self.events {
            match event {
                SessionLogEvent::Message {
                    id,
                    time,
                    role,
                    text,
                    execution,
                    media,
                    transcript,
                } => {
                    let message = ChatMessage {
                        id: id.clone(),
                        role: role.as_chat_role(),
                        content: text.clone(),
                        timestamp: parse_time_ms(time),
                        execution: execution.clone(),
                        media: media.clone(),
                        transcript: transcript.clone(),
                    };
                    push_message_unbounded(&mut session, message);
                }
                SessionLogEvent::Reasoning { id, time, text } => {
                    let message = ChatMessage {
                        id: id.clone(),
                        role: ChatRole::System,
                        content: format!("[reasoning]\n{text}"),
                        timestamp: parse_time_ms(time),
                        execution: None,
                        media: None,
                        transcript: None,
                    };
                    push_message_unbounded(&mut session, message);
                }
                SessionLogEvent::ToolCall {
                    id,
                    time,
                    tool,
                    input,
                    ..
                } => {
                    let message = ChatMessage {
                        id: id.clone(),
                        role: ChatRole::System,
                        content: format!("[tool_call:{tool}] {input}"),
                        timestamp: parse_time_ms(time),
                        execution: None,
                        media: None,
                        transcript: None,
                    };
                    push_message_unbounded(&mut session, message);
                }
                SessionLogEvent::ToolResult {
                    id,
                    time,
                    tool,
                    output,
                    status,
                    error,
                    ..
                } => {
                    let body = error.clone().or_else(|| output.clone()).unwrap_or_default();
                    let status = status.clone().unwrap_or_else(|| "completed".to_string());
                    let message = ChatMessage {
                        id: id.clone(),
                        role: ChatRole::System,
                        content: format!("[tool_result:{tool}:{status}]\n{body}"),
                        timestamp: parse_time_ms(time),
                        execution: None,
                        media: None,
                        transcript: None,
                    };
                    push_message_unbounded(&mut session, message);
                }
                SessionLogEvent::Compact {
                    id, time, summary, ..
                } => {
                    let message = ChatMessage {
                        id: id.clone(),
                        role: ChatRole::System,
                        content: format!("[compact]\n{summary}"),
                        timestamp: parse_time_ms(time),
                        execution: None,
                        media: None,
                        transcript: None,
                    };
                    push_message_unbounded(&mut session, message);
                }
                SessionLogEvent::Usage { usage, .. } => {
                    if let Some(input) = usage.input_tokens {
                        session.prompt_tokens += input;
                    }
                    if let Some(output) = usage.output_tokens {
                        session.completion_tokens += output;
                    }
                    if let Some(cost) = usage.cost {
                        session.cost += cost;
                    }
                }
                SessionLogEvent::SessionMeta { .. } => {}
            }
        }

        session.metadata.message_count = session.messages.len() as u32;
        if session.name == "Imported Chat" {
            session.auto_name_from_first_message();
        }
        session.created_at = parse_time_ms(&self.meta.created_at);
        session.updated_at = parse_time_ms(&self.meta.updated_at);
        session
    }
}

#[derive(Debug, Clone)]
pub struct FileSessionStore {
    root: PathBuf,
}

impl FileSessionStore {
    pub fn default_root() -> Result<PathBuf> {
        crate::paths::sessions_dir()
    }

    pub fn open_default() -> Result<Self> {
        Self::new(Self::default_root()?)
    }

    pub fn new(root: PathBuf) -> Result<Self> {
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn create_empty(&self, agent_id: String, model: String) -> Result<FileSession> {
        let now = now_iso();
        let id = Uuid::new_v4().to_string();
        let mut meta = SessionMeta::new(id, now.clone(), now);
        meta.title = Some("New Chat".to_string());
        meta.model = Some(model);
        meta.agent_id = Some(agent_id);
        let session = FileSession::new(meta.clone(), vec![meta.into_event()]);
        self.write_session(&session, false)?;
        Ok(session)
    }

    pub fn write_session(&self, session: &FileSession, force: bool) -> Result<WriteOutcome> {
        let path = self.path_for_meta(&session.meta)?;
        if path.exists() && !force {
            return Ok(WriteOutcome::Skipped { path });
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = File::create(&path)?;
        for event in &session.events {
            write_event_line(&mut file, event)?;
        }
        Ok(WriteOutcome::Written { path })
    }

    pub fn append_event(&self, session_id: &str, event: &SessionLogEvent) -> Result<()> {
        let path = self
            .find_session_path(session_id)?
            .ok_or_else(|| anyhow!("Session not found: {session_id}"))?;
        let mut file = OpenOptions::new().append(true).open(path)?;
        write_event_line(&mut file, event)
    }

    pub fn get(&self, id: &str) -> Result<Option<FileSession>> {
        let Some(path) = self.find_session_path(id)? else {
            return Ok(None);
        };
        read_session_file(&path).map(Some)
    }

    pub fn delete(&self, id: &str) -> Result<bool> {
        let Some(path) = self.find_session_path(id)? else {
            return Ok(false);
        };
        fs::remove_file(path)?;
        Ok(true)
    }

    pub fn list(&self) -> Result<Vec<FileSession>> {
        let mut sessions = Vec::new();
        for path in self.session_paths()? {
            match read_session_file(&path) {
                Ok(session) => sessions.push(session),
                Err(err) => {
                    tracing::warn!(path = %path.display(), error = %err, "Skipping invalid session file")
                }
            }
        }
        sessions.sort_by_key(|session| std::cmp::Reverse(parse_time_ms(&session.meta.updated_at)));
        Ok(sessions)
    }

    pub fn list_summaries(&self) -> Result<Vec<ChatSessionSummary>> {
        Ok(self
            .list()?
            .into_iter()
            .map(|session| ChatSessionSummary::from(&session.to_chat_session()))
            .collect())
    }

    pub fn search(&self, query: &str) -> Result<Vec<FileSession>> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self
            .list()?
            .into_iter()
            .filter(|session| {
                session
                    .meta
                    .title
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(&needle)
                    || session
                        .events
                        .iter()
                        .any(|event| event_text(event).contains(&needle))
            })
            .collect())
    }

    fn path_for_meta(&self, meta: &SessionMeta) -> Result<PathBuf> {
        let created = parse_datetime(&meta.created_at).unwrap_or_else(Utc::now);
        Ok(self
            .root
            .join(format!("{:04}", created.year()))
            .join(format!("{:02}", created.month()))
            .join(format!("{:02}", created.day()))
            .join(format!("{}.jsonl", sanitize_session_id(&meta.id))))
    }

    fn find_session_path(&self, id: &str) -> Result<Option<PathBuf>> {
        let exact_file = format!("{}.jsonl", sanitize_session_id(id));
        let mut prefix_matches = Vec::new();
        for path in self.session_paths()? {
            let Some(file_name) = path.file_name().and_then(|v| v.to_str()) else {
                continue;
            };
            if file_name == exact_file {
                return Ok(Some(path));
            }
            if file_name.starts_with(id) {
                prefix_matches.push(path);
            }
        }
        match prefix_matches.len() {
            0 => Ok(None),
            1 => Ok(prefix_matches.pop()),
            _ => Err(anyhow!("Session id is ambiguous: {id}")),
        }
    }

    fn session_paths(&self) -> Result<Vec<PathBuf>> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut paths = Vec::new();
        for entry in WalkDir::new(&self.root).into_iter().filter_map(Result::ok) {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|v| v.to_str()) == Some("jsonl") {
                paths.push(path.to_path_buf());
            }
        }
        Ok(paths)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteOutcome {
    Written { path: PathBuf },
    Skipped { path: PathBuf },
}

pub fn read_session_file(path: &Path) -> Result<FileSession> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();
    for (index, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("failed to read {}", path.display()))?;
        if line.trim().is_empty() {
            continue;
        }
        let event: SessionLogEvent = serde_json::from_str(&line)
            .with_context(|| format!("invalid JSONL at {}:{}", path.display(), index + 1))?;
        events.push(event);
    }
    FileSession::from_events(events)
}

pub fn stable_session_id(events: &[SessionLogEvent]) -> String {
    let mut hasher = Sha256::new();
    for event in events {
        if matches!(event, SessionLogEvent::SessionMeta { .. }) {
            continue;
        }
        if let Ok(bytes) = serde_json::to_vec(event) {
            hasher.update(bytes);
            hasher.update(b"\n");
        }
    }
    let digest = hasher.finalize();
    hex::encode(digest)[..32].to_string()
}

pub fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn parse_time_ms(value: &str) -> i64 {
    parse_datetime(value)
        .map(|dt| dt.timestamp_millis())
        .unwrap_or_default()
}

pub fn iso_from_millis(value: i64) -> String {
    Utc.timestamp_millis_opt(value)
        .single()
        .unwrap_or_else(Utc::now)
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn text_from_content(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(content_item_text)
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(_) => content_item_text(value).unwrap_or_default(),
        _ => String::new(),
    }
}

fn content_item_text(value: &Value) -> Option<String> {
    let object = value.as_object()?;
    match object.get("type").and_then(Value::as_str) {
        Some("text") | Some("input_text") | Some("output_text") => object
            .get("text")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        Some("thinking") | Some("reasoning") => object
            .get("thinking")
            .or_else(|| object.get("text"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        Some("tool_result") => object
            .get("content")
            .map(text_from_content)
            .filter(|text| !text.is_empty()),
        _ => object
            .get("text")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
    }
}

fn write_event_line(file: &mut File, event: &SessionLogEvent) -> Result<()> {
    serde_json::to_writer(&mut *file, event)?;
    file.write_all(b"\n")?;
    Ok(())
}

fn message_event_from_chat_message(message: &ChatMessage) -> SessionLogEvent {
    SessionLogEvent::Message {
        id: message.id.clone(),
        time: iso_from_millis(message.timestamp),
        role: SessionMessageRole::from_chat_role(&message.role),
        text: message.content.clone(),
        execution: message.execution.clone(),
        media: message.media.clone(),
        transcript: message.transcript.clone(),
    }
}

fn push_message_unbounded(session: &mut ChatSession, message: ChatMessage) {
    if let Some(execution) = &message.execution {
        session.metadata.update(execution.tokens_used);
        if let Some(input) = execution.input_tokens {
            session.prompt_tokens += i64::from(input);
        }
        if let Some(output) = execution.output_tokens {
            session.completion_tokens += i64::from(output);
        }
        if let Some(cost) = execution.cost_usd {
            session.cost += cost;
        }
    } else {
        session.metadata.message_count += 1;
    }
    session.messages.push(message);
}

fn event_id(event: &SessionLogEvent) -> Option<&str> {
    match event {
        SessionLogEvent::Message { id, .. }
        | SessionLogEvent::Reasoning { id, .. }
        | SessionLogEvent::ToolCall { id, .. }
        | SessionLogEvent::ToolResult { id, .. }
        | SessionLogEvent::Compact { id, .. } => Some(id),
        SessionLogEvent::SessionMeta { .. } | SessionLogEvent::Usage { .. } => None,
    }
}

fn parse_datetime(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
}

fn sanitize_session_id(id: &str) -> String {
    id.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn session_source_to_str(source: ChatSessionSource) -> &'static str {
    match source {
        ChatSessionSource::Workspace => "workspace",
        ChatSessionSource::Background => "background",
        ChatSessionSource::Telegram => "telegram",
        ChatSessionSource::Discord => "discord",
        ChatSessionSource::Slack => "slack",
    }
}

fn session_source_from_str(source: &str) -> Option<ChatSessionSource> {
    match source.trim().to_ascii_lowercase().as_str() {
        "workspace" => Some(ChatSessionSource::Workspace),
        "background" => Some(ChatSessionSource::Background),
        "telegram" => Some(ChatSessionSource::Telegram),
        "discord" => Some(ChatSessionSource::Discord),
        "slack" => Some(ChatSessionSource::Slack),
        _ => None,
    }
}

fn latest_event_time(events: &[SessionLogEvent]) -> Option<String> {
    events
        .iter()
        .map(|event| match event {
            SessionLogEvent::SessionMeta { updated_at, .. } => updated_at.clone(),
            SessionLogEvent::Message { time, .. }
            | SessionLogEvent::Reasoning { time, .. }
            | SessionLogEvent::ToolCall { time, .. }
            | SessionLogEvent::ToolResult { time, .. }
            | SessionLogEvent::Compact { time, .. }
            | SessionLogEvent::Usage { time, .. } => time.clone(),
        })
        .max_by_key(|time| parse_time_ms(time))
}

fn meta_updated_in_place(events: &mut [SessionLogEvent], updated_at: &str) {
    if let Some(SessionLogEvent::SessionMeta {
        updated_at: current,
        ..
    }) = events.first_mut()
    {
        *current = updated_at.to_string();
    }
}

fn event_text(event: &SessionLogEvent) -> String {
    match event {
        SessionLogEvent::SessionMeta { title, cwd, .. } => {
            format!(
                "{} {}",
                title.as_deref().unwrap_or(""),
                cwd.as_deref().unwrap_or("")
            )
        }
        SessionLogEvent::Message { text, .. }
        | SessionLogEvent::Reasoning { text, .. }
        | SessionLogEvent::Compact { summary: text, .. } => text.to_lowercase(),
        SessionLogEvent::ToolCall { tool, input, .. } => format!("{tool} {input}").to_lowercase(),
        SessionLogEvent::ToolResult {
            tool,
            output,
            error,
            ..
        } => format!(
            "{tool} {} {}",
            output.as_deref().unwrap_or(""),
            error.as_deref().unwrap_or("")
        )
        .to_lowercase(),
        SessionLogEvent::Usage { .. } => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::chat_session::ExecutionStepInfo;
    use tempfile::tempdir;

    #[test]
    fn writes_and_reads_one_jsonl_session() {
        let dir = tempdir().unwrap();
        let store = FileSessionStore::new(dir.path().join("sessions")).unwrap();
        let mut meta = SessionMeta::new(
            "session-1".to_string(),
            "2026-05-03T00:00:00.000Z".to_string(),
            "2026-05-03T00:00:01.000Z".to_string(),
        );
        meta.title = Some("Hello".to_string());
        let session = FileSession::new(
            meta.clone(),
            vec![
                meta.into_event(),
                SessionLogEvent::Message {
                    id: "msg-1".to_string(),
                    time: "2026-05-03T00:00:01.000Z".to_string(),
                    role: SessionMessageRole::User,
                    text: "hello".to_string(),
                    execution: None,
                    media: None,
                    transcript: None,
                },
            ],
        );
        assert!(matches!(
            store.write_session(&session, false).unwrap(),
            WriteOutcome::Written { .. }
        ));
        let loaded = store.get("session-1").unwrap().unwrap();
        assert_eq!(loaded.meta.id, "session-1");
        assert_eq!(loaded.to_chat_session().messages.len(), 1);
    }

    #[test]
    fn skips_existing_session_without_force() {
        let dir = tempdir().unwrap();
        let store = FileSessionStore::new(dir.path().join("sessions")).unwrap();
        let session = store
            .create_empty("default".to_string(), "gpt-5.4".to_string())
            .unwrap();
        assert!(matches!(
            store.write_session(&session, false).unwrap(),
            WriteOutcome::Skipped { .. }
        ));
    }

    #[test]
    fn extracts_text_from_common_content_shapes() {
        let value = serde_json::json!([
            { "type": "input_text", "text": "hello" },
            { "type": "output_text", "text": "world" }
        ]);
        assert_eq!(text_from_content(&value), "hello\nworld");
    }

    #[test]
    fn converts_chat_session_to_jsonl_session() {
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string())
            .with_name("Build fix")
            .with_skill("release")
            .with_retention("7d")
            .with_source(ChatSessionSource::Workspace, "conversation-1");
        session.add_message(ChatMessage::user("hello"));
        session.add_message(ChatMessage::assistant("world"));
        let summary_message_id = session.messages[1].id.clone();
        session.summary_message_id = Some(summary_message_id.clone());
        session.archive();

        let file_session = FileSession::from_chat_session(&session);
        assert_eq!(file_session.meta.agent_id.as_deref(), Some("agent-1"));
        assert_eq!(file_session.meta.skill_id.as_deref(), Some("release"));
        assert_eq!(file_session.meta.retention.as_deref(), Some("7d"));
        assert!(file_session.meta.archived_at.is_some());
        let reloaded = file_session.to_chat_session();
        assert_eq!(reloaded.skill_id.as_deref(), Some("release"));
        assert_eq!(reloaded.source_channel, Some(ChatSessionSource::Workspace));
        assert_eq!(
            reloaded.source_conversation_id.as_deref(),
            Some("conversation-1")
        );
        assert_eq!(
            reloaded.summary_message_id.as_deref(),
            Some(summary_message_id.as_str())
        );
        assert!(reloaded.is_archived());
    }

    #[test]
    fn jsonl_roundtrip_does_not_truncate_long_sessions() {
        let mut meta = SessionMeta::new(
            "session-1".to_string(),
            "2026-05-03T00:00:00.000Z".to_string(),
            "2026-05-03T00:05:00.000Z".to_string(),
        );
        meta.title = Some("Long chat".to_string());
        let mut events = vec![meta.clone().into_event()];
        for index in 0..250 {
            events.push(SessionLogEvent::Message {
                id: format!("msg-{index}"),
                time: "2026-05-03T00:00:01.000Z".to_string(),
                role: SessionMessageRole::User,
                text: format!("message {index}"),
                execution: None,
                media: None,
                transcript: None,
            });
        }

        let session = FileSession::new(meta, events).to_chat_session();

        assert_eq!(session.messages.len(), 250);
        assert_eq!(session.messages.first().unwrap().content, "message 0");
        assert_eq!(session.messages.last().unwrap().content, "message 249");
    }

    #[test]
    fn merge_chat_session_preserves_existing_non_message_events() {
        let mut meta = SessionMeta::new(
            "session-1".to_string(),
            "2026-05-03T00:00:00.000Z".to_string(),
            "2026-05-03T00:00:02.000Z".to_string(),
        );
        meta.title = Some("Old title".to_string());
        let existing = FileSession::new(
            meta.clone(),
            vec![
                meta.clone().into_event(),
                SessionLogEvent::Message {
                    id: "msg-1".to_string(),
                    time: "2026-05-03T00:00:01.000Z".to_string(),
                    role: SessionMessageRole::User,
                    text: "hello".to_string(),
                    execution: None,
                    media: None,
                    transcript: None,
                },
                SessionLogEvent::ToolCall {
                    id: "tool-1".to_string(),
                    time: "2026-05-03T00:00:02.000Z".to_string(),
                    tool: "bash".to_string(),
                    input: serde_json::json!({ "command": "pwd" }),
                    cwd: Some("/tmp/project".to_string()),
                },
            ],
        );
        let mut chat = existing.to_chat_session();
        chat.rename("New title");

        let merged = FileSession::merge_chat_session(Some(&existing), &chat);

        assert_eq!(merged.meta.title.as_deref(), Some("New title"));
        assert!(matches!(
            merged.events.get(2),
            Some(SessionLogEvent::ToolCall { tool, cwd, .. })
                if tool == "bash" && cwd.as_deref() == Some("/tmp/project")
        ));
    }

    #[test]
    fn message_structured_fields_roundtrip_through_jsonl() {
        let mut execution = MessageExecution::new().complete(1200, 42);
        execution.input_tokens = Some(20);
        execution.output_tokens = Some(22);
        execution.cost_usd = Some(0.01);
        execution.add_step(
            ExecutionStepInfo::new("tool_call", "bash")
                .with_status("completed")
                .with_duration(50),
        );
        let message = ChatMessage::assistant("done")
            .with_execution(execution.clone())
            .with_media(ChatMessageMedia::voice("/tmp/voice.ogg", Some(3)))
            .with_transcript(ChatMessageTranscript {
                text: "done".to_string(),
                model: Some("whisper".to_string()),
                updated_at: Some(1_777_852_800_000),
            });
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        session.add_message(message);

        let reloaded = FileSession::from_chat_session(&session).to_chat_session();
        let reloaded_message = reloaded.messages.first().unwrap();

        assert_eq!(reloaded_message.execution, Some(execution));
        assert_eq!(
            reloaded_message.media,
            Some(ChatMessageMedia::voice("/tmp/voice.ogg", Some(3)))
        );
        assert_eq!(reloaded_message.transcript.as_ref().unwrap().text, "done");
    }
}
