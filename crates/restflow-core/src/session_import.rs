//! Import adapters for local coding-agent transcripts.

use crate::session_log::{
    FileSession, FileSessionStore, SessionLogEvent, SessionMessageRole, SessionMeta, UsageValues,
    WriteOutcome, iso_from_millis, now_iso, stable_session_id, text_from_content,
};
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;
use walkdir::WalkDir;

const MAX_CODEX_IMPORT_LINE_BYTES: usize = 2 * 1024 * 1024;
const MAX_CODEX_IMPORT_FILE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportSource {
    All,
    Claude,
    Codex,
    Opencode,
}

impl ImportSource {
    pub fn concrete_sources(self) -> Vec<ImportSource> {
        match self {
            Self::All => vec![Self::Claude, Self::Codex, Self::Opencode],
            other => vec![other],
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Opencode => "opencode",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImportOptions {
    pub source: ImportSource,
    pub path: Option<PathBuf>,
    pub dry_run: bool,
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImportReport {
    pub imported: usize,
    pub skipped: usize,
    pub failed: usize,
    pub sources: Vec<SourceImportReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceImportReport {
    pub source: String,
    pub discovered: usize,
    pub imported: usize,
    pub skipped: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

impl SourceImportReport {
    fn new(source: ImportSource) -> Self {
        Self {
            source: source.as_str().to_string(),
            discovered: 0,
            imported: 0,
            skipped: 0,
            failed: 0,
            errors: Vec::new(),
        }
    }
}

pub fn import_sessions(store: &FileSessionStore, options: ImportOptions) -> Result<ImportReport> {
    let mut report = ImportReport::default();
    for source in options.source.concrete_sources() {
        let source_report = import_source(store, &options, source)?;
        report.imported += source_report.imported;
        report.skipped += source_report.skipped;
        report.failed += source_report.failed;
        report.sources.push(source_report);
    }
    Ok(report)
}

fn import_source(
    store: &FileSessionStore,
    options: &ImportOptions,
    source: ImportSource,
) -> Result<SourceImportReport> {
    let mut report = SourceImportReport::new(source);
    if options.dry_run {
        match discover_source_count(source, options.path.as_deref()) {
            Ok(count) => {
                report.discovered = count;
                report.imported = count;
            }
            Err(err) => {
                report.failed += 1;
                report.errors.push(err.to_string());
            }
        }
        return Ok(report);
    }

    let sessions = match source {
        ImportSource::All => unreachable!("all is expanded before import"),
        ImportSource::Claude => import_claude(options.path.as_deref()),
        ImportSource::Codex => import_codex(options.path.as_deref()),
        ImportSource::Opencode => import_opencode(options.path.as_deref()),
    };

    let sessions = match sessions {
        Ok(sessions) => sessions,
        Err(err) => {
            report.failed += 1;
            report.errors.push(err.to_string());
            return Ok(report);
        }
    };

    report.discovered = sessions.len();
    for session in sessions {
        if options.dry_run {
            report.imported += 1;
            continue;
        }
        match store.write_session(&session, options.force) {
            Ok(WriteOutcome::Written { .. }) => report.imported += 1,
            Ok(WriteOutcome::Skipped { .. }) => report.skipped += 1,
            Err(err) => {
                report.failed += 1;
                report.errors.push(err.to_string());
            }
        }
    }
    Ok(report)
}

fn discover_source_count(source: ImportSource, path: Option<&Path>) -> Result<usize> {
    match source {
        ImportSource::All => unreachable!("all is expanded before import"),
        ImportSource::Claude => discover_claude_count(path),
        ImportSource::Codex => discover_codex_count(path),
        ImportSource::Opencode => discover_opencode_count(path),
    }
}

fn discover_claude_count(path: Option<&Path>) -> Result<usize> {
    let root = path
        .map(Path::to_path_buf)
        .unwrap_or_else(default_claude_root);
    if !root.exists() {
        return Ok(0);
    }
    Ok(jsonl_files(&root, None)?
        .into_iter()
        .filter(|file| !path_has_component(file, "subagents"))
        .count())
}

fn discover_codex_count(path: Option<&Path>) -> Result<usize> {
    let root = path
        .map(Path::to_path_buf)
        .unwrap_or_else(default_codex_root);
    if !root.exists() {
        return Ok(0);
    }
    Ok(jsonl_files(&root, Some("rollout-"))?.len())
}

fn discover_opencode_count(path: Option<&Path>) -> Result<usize> {
    if let Some(root) = path {
        return discover_opencode_root_count(root);
    }

    let mut total = 0;
    let mut seen = BTreeSet::new();
    for root in default_opencode_roots() {
        let key = root.to_string_lossy().to_string();
        if seen.insert(key) {
            total += discover_opencode_root_count(&root)?;
        }
    }
    Ok(total)
}

fn discover_opencode_root_count(root: &Path) -> Result<usize> {
    if !root.exists() {
        return Ok(0);
    }
    let db_path = if root.is_file() {
        root.to_path_buf()
    } else {
        root.join("opencode.db")
    };
    if db_path.exists() {
        let rows = sqlite_json(&db_path, "select count(*) as count from session")?;
        return Ok(rows
            .first()
            .and_then(|row| int_field(row, "count"))
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(0));
    }
    Ok(json_files(&root.join("storage").join("session"))?.len())
}

fn import_claude(path: Option<&Path>) -> Result<Vec<FileSession>> {
    let root = path
        .map(Path::to_path_buf)
        .unwrap_or_else(default_claude_root);
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut sessions = Vec::new();
    for file in jsonl_files(&root, None)? {
        if path_has_component(&file, "subagents") {
            continue;
        }
        match parse_claude_file(&file) {
            Ok(Some(session)) => sessions.push(session),
            Ok(None) => {}
            Err(err) => {
                tracing::warn!(path = %file.display(), error = %err, "Skipping Claude transcript")
            }
        }
    }
    Ok(sessions)
}

fn parse_claude_file(path: &Path) -> Result<Option<FileSession>> {
    let lines = read_json_lines(path)?;
    if lines.is_empty() {
        return Ok(None);
    }

    let mut session_id = None;
    let mut created_at = None;
    let mut updated_at = None;
    let mut title = None;
    let mut cwd = None;
    let mut model = None;
    let mut app_version = None;
    let mut git_branch = None;
    let mut events = Vec::new();

    for value in lines {
        let Some(kind) = value.get("type").and_then(Value::as_str) else {
            continue;
        };
        if session_id.is_none() {
            session_id = string_field(&value, "sessionId")
                .or_else(|| string_field(&value, "session_id"))
                .or_else(|| {
                    path.file_stem()
                        .and_then(|v| v.to_str())
                        .map(ToOwned::to_owned)
                });
        }
        let time = string_field(&value, "timestamp").unwrap_or_else(now_iso);
        created_at.get_or_insert_with(|| time.clone());
        updated_at = Some(time.clone());
        cwd = cwd.or_else(|| string_field(&value, "cwd"));
        app_version = app_version.or_else(|| string_field(&value, "version"));
        git_branch = git_branch.or_else(|| string_field(&value, "gitBranch"));

        match kind {
            "custom-title" => title = string_field(&value, "customTitle"),
            "ai-title" => title = title.or_else(|| string_field(&value, "aiTitle")),
            "summary" => {
                if let Some(summary) = string_field(&value, "summary") {
                    events.push(SessionLogEvent::Compact {
                        id: string_field(&value, "leafUuid").unwrap_or_else(new_id),
                        time,
                        summary,
                        auto: None,
                    });
                }
            }
            "system" if value.get("subtype").and_then(Value::as_str) == Some("init") => {
                cwd = cwd.or_else(|| string_field(&value, "cwd"));
                model = model.or_else(|| string_field(&value, "model"));
                app_version = app_version.or_else(|| string_field(&value, "claude_code_version"));
            }
            "user" => {
                if value
                    .get("isMeta")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    continue;
                }
                parse_claude_tool_results(&value, &time, &mut events);
                let text = claude_user_text(&value);
                if text.trim().is_empty() {
                    continue;
                }
                if value
                    .get("isCompactSummary")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    events.push(SessionLogEvent::Compact {
                        id: string_field(&value, "uuid").unwrap_or_else(new_id),
                        time,
                        summary: text,
                        auto: None,
                    });
                } else {
                    events.push(SessionLogEvent::Message {
                        id: string_field(&value, "uuid").unwrap_or_else(new_id),
                        time,
                        role: SessionMessageRole::User,
                        text,
                        execution: None,
                        media: None,
                        transcript: None,
                    });
                }
            }
            "assistant" => {
                model = model.or_else(|| {
                    value
                        .get("message")
                        .and_then(|message| message.get("model"))
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                });
                parse_claude_assistant(&value, &time, &mut events);
                if let Some(usage) = value
                    .get("message")
                    .and_then(|message| message.get("usage"))
                {
                    push_claude_usage(usage, &time, &mut events);
                }
            }
            _ => {}
        }
    }

    if events.is_empty() {
        return Ok(None);
    }

    let mut meta = SessionMeta::new(
        session_id.unwrap_or_else(|| stable_session_id(&events)),
        created_at.unwrap_or_else(now_iso),
        updated_at.unwrap_or_else(now_iso),
    );
    meta.title = title.or_else(|| first_user_title(&events));
    meta.cwd = cwd;
    meta.model = model;
    meta.provider = meta.model.as_deref().and_then(infer_provider_from_model);
    meta.app_version = app_version;
    meta.git_branch = git_branch;
    let mut all_events = vec![meta.clone().into_event()];
    all_events.extend(events);
    Ok(Some(FileSession::new(meta, all_events)))
}

fn parse_claude_assistant(value: &Value, time: &str, events: &mut Vec<SessionLogEvent>) {
    let assistant_id = string_field(value, "uuid").unwrap_or_else(new_id);
    let Some(content) = value
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_array)
    else {
        return;
    };

    for (index, block) in content.iter().enumerate() {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = block.get("text").and_then(Value::as_str)
                    && !text.trim().is_empty()
                {
                    events.push(SessionLogEvent::Message {
                        id: format!("{assistant_id}-{index}"),
                        time: time.to_string(),
                        role: SessionMessageRole::Assistant,
                        text: text.to_string(),
                        execution: None,
                        media: None,
                        transcript: None,
                    });
                }
            }
            Some("thinking") => {
                if let Some(text) = block
                    .get("thinking")
                    .or_else(|| block.get("text"))
                    .and_then(Value::as_str)
                    && !text.trim().is_empty()
                {
                    events.push(SessionLogEvent::Reasoning {
                        id: format!("{assistant_id}-{index}"),
                        time: time.to_string(),
                        text: text.to_string(),
                    });
                }
            }
            Some("tool_use") => {
                events.push(SessionLogEvent::ToolCall {
                    id: string_field(block, "id")
                        .unwrap_or_else(|| format!("{assistant_id}-{index}")),
                    time: time.to_string(),
                    tool: string_field(block, "name").unwrap_or_else(|| "tool".to_string()),
                    input: block.get("input").cloned().unwrap_or(Value::Null),
                    cwd: None,
                });
            }
            _ => {}
        }
    }
}

fn parse_claude_tool_results(value: &Value, time: &str, events: &mut Vec<SessionLogEvent>) {
    let mut emitted = false;
    if let Some(content) = value
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_array)
    {
        for (index, block) in content.iter().enumerate() {
            if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                continue;
            }
            let is_error = block
                .get("is_error")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            events.push(SessionLogEvent::ToolResult {
                id: string_field(block, "tool_use_id")
                    .or_else(|| string_field(value, "sourceToolAssistantUUID"))
                    .unwrap_or_else(|| {
                        format!(
                            "{}-{index}",
                            string_field(value, "uuid").unwrap_or_else(new_id)
                        )
                    }),
                time: time.to_string(),
                tool: "tool".to_string(),
                output: block.get("content").map(output_to_text),
                status: Some(if is_error { "error" } else { "completed" }.to_string()),
                error: None,
                exit_code: None,
                duration_ms: None,
            });
            emitted = true;
        }
    }

    if emitted {
        return;
    }

    if let Some(result) = value.get("toolUseResult") {
        events.push(SessionLogEvent::ToolResult {
            id: string_field(value, "sourceToolAssistantUUID")
                .or_else(|| string_field(value, "uuid"))
                .unwrap_or_else(new_id),
            time: time.to_string(),
            tool: "tool".to_string(),
            output: Some(output_to_text(result)),
            status: Some("completed".to_string()),
            error: None,
            exit_code: None,
            duration_ms: None,
        });
    }
}

fn claude_user_text(value: &Value) -> String {
    let Some(content) = value
        .get("message")
        .and_then(|message| message.get("content"))
    else {
        return String::new();
    };

    match content {
        Value::Array(items) => items
            .iter()
            .filter(|item| item.get("type").and_then(Value::as_str) != Some("tool_result"))
            .map(text_from_content)
            .filter(|text| !text.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        other => text_from_content(other),
    }
}

fn push_claude_usage(usage: &Value, time: &str, events: &mut Vec<SessionLogEvent>) {
    let usage = UsageValues {
        input_tokens: int_field(usage, "input_tokens"),
        output_tokens: int_field(usage, "output_tokens"),
        reasoning_tokens: None,
        cache_read: int_field(usage, "cache_read_input_tokens"),
        cache_write: int_field(usage, "cache_creation_input_tokens"),
        cost: None,
    };
    events.push(SessionLogEvent::Usage {
        time: time.to_string(),
        usage,
    });
}

fn import_codex(path: Option<&Path>) -> Result<Vec<FileSession>> {
    let root = path
        .map(Path::to_path_buf)
        .unwrap_or_else(default_codex_root);
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut sessions = Vec::new();
    for file in jsonl_files(&root, Some("rollout-"))? {
        let parsed = match std::fs::metadata(&file) {
            Ok(metadata) if metadata.len() > MAX_CODEX_IMPORT_FILE_BYTES => {
                parse_codex_large_file_stub(&file, metadata.len())
            }
            _ => parse_codex_file(&file),
        };
        match parsed {
            Ok(Some(session)) => sessions.push(session),
            Ok(None) => {}
            Err(err) => {
                tracing::warn!(path = %file.display(), error = %err, "Skipping Codex rollout")
            }
        }
    }
    Ok(sessions)
}

fn parse_codex_file(path: &Path) -> Result<Option<FileSession>> {
    let mut meta = None;
    let mut events = Vec::new();
    let mut last_time = None;
    let mut context_model = None;
    let mut saw_line = false;
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    for (line_number, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("failed to read {}", path.display()))?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        saw_line = true;
        if skip_codex_line_before_parse(line) {
            continue;
        }
        let value: Value = serde_json::from_str(line)
            .with_context(|| format!("invalid JSONL at {}:{}", path.display(), line_number + 1))?;
        let time = string_field(&value, "timestamp").unwrap_or_else(now_iso);
        last_time = Some(time.clone());
        match value.get("type").and_then(Value::as_str) {
            Some("session_meta") => {
                let payload = payload(&value);
                meta = Some(codex_meta_from_payload(path, &time, payload));
            }
            Some("turn_context") => {
                context_model = context_model.or_else(|| string_field(payload(&value), "model"));
                if let Some(next) = meta.as_mut() {
                    next.model = next.model.clone().or_else(|| context_model.clone());
                }
            }
            Some("response_item") => parse_codex_response_item(payload(&value), &time, &mut events),
            Some("compacted") => {
                if let Some(message) = string_field(payload(&value), "message") {
                    events.push(SessionLogEvent::Compact {
                        id: new_id(),
                        time,
                        summary: message,
                        auto: None,
                    });
                }
            }
            Some("event_msg") => parse_codex_event_msg(payload(&value), &time, &mut events),
            _ => {}
        }
    }

    if !saw_line {
        return Ok(None);
    }

    if events.is_empty() {
        return Ok(None);
    }
    let mut meta = meta.unwrap_or_else(|| {
        SessionMeta::new(
            path.file_stem()
                .and_then(|v| v.to_str())
                .unwrap_or("codex-session")
                .to_string(),
            last_time.clone().unwrap_or_else(now_iso),
            last_time.clone().unwrap_or_else(now_iso),
        )
    });
    meta.updated_at = last_time.unwrap_or_else(|| meta.created_at.clone());
    meta.model = meta.model.or(context_model);
    if meta.provider.is_none() {
        meta.provider = meta.model.as_deref().and_then(infer_provider_from_model);
    }
    meta.title = meta.title.or_else(|| first_user_title(&events));
    let mut all_events = vec![meta.clone().into_event()];
    all_events.extend(events);
    Ok(Some(FileSession::new(meta, all_events)))
}

fn parse_codex_large_file_stub(path: &Path, size_bytes: u64) -> Result<Option<FileSession>> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    for (line_number, line) in reader.lines().enumerate().take(20) {
        let line = line.with_context(|| format!("failed to read {}", path.display()))?;
        let line = line.trim();
        if line.is_empty() || skip_codex_line_before_parse(line) {
            continue;
        }
        let value: Value = serde_json::from_str(line)
            .with_context(|| format!("invalid JSONL at {}:{}", path.display(), line_number + 1))?;
        if value.get("type").and_then(Value::as_str) != Some("session_meta") {
            continue;
        }
        let time = string_field(&value, "timestamp").unwrap_or_else(now_iso);
        let mut meta = codex_meta_from_payload(path, &time, payload(&value));
        meta.title = Some(format!("Codex import stub {}", meta.id));
        let summary = format!(
            "Codex transcript omitted because the source JSONL is {} bytes. Original file: {}",
            size_bytes,
            path.display()
        );
        let events = vec![
            meta.clone().into_event(),
            SessionLogEvent::Compact {
                id: new_id(),
                time,
                summary,
                auto: Some(true),
            },
        ];
        return Ok(Some(FileSession::new(meta, events)));
    }
    Ok(None)
}

fn codex_meta_from_payload(path: &Path, time: &str, payload: &Value) -> SessionMeta {
    let mut meta = SessionMeta::new(
        string_field(payload, "id").unwrap_or_else(|| {
            path.file_stem()
                .and_then(|v| v.to_str())
                .unwrap_or("codex-session")
                .to_string()
        }),
        string_field(payload, "timestamp").unwrap_or_else(|| time.to_string()),
        time.to_string(),
    );
    meta.cwd = string_field(payload, "cwd");
    meta.provider = string_field(payload, "model_provider");
    meta.app_version = string_field(payload, "cli_version");
    meta.git_branch = payload
        .get("git")
        .and_then(|git| string_field(git, "branch"));
    meta
}

fn skip_codex_line_before_parse(line: &str) -> bool {
    line.len() > MAX_CODEX_IMPORT_LINE_BYTES || line.contains("\"encrypted_content\"")
}

fn parse_codex_response_item(item: &Value, time: &str, events: &mut Vec<SessionLogEvent>) {
    match item.get("type").and_then(Value::as_str) {
        Some("message") => {
            let role = match item.get("role").and_then(Value::as_str) {
                Some("assistant") => SessionMessageRole::Assistant,
                Some("system") | Some("developer") => SessionMessageRole::System,
                _ => SessionMessageRole::User,
            };
            let text = item
                .get("content")
                .map(text_from_content)
                .unwrap_or_default();
            if !text.trim().is_empty() {
                events.push(SessionLogEvent::Message {
                    id: string_field(item, "id").unwrap_or_else(new_id),
                    time: time.to_string(),
                    role,
                    text,
                    execution: None,
                    media: None,
                    transcript: None,
                });
            }
        }
        Some("reasoning") => {
            let text = item
                .get("content")
                .map(text_from_content)
                .filter(|text| !text.is_empty())
                .or_else(|| item.get("summary").map(text_from_content))
                .unwrap_or_default();
            if !text.trim().is_empty() {
                events.push(SessionLogEvent::Reasoning {
                    id: string_field(item, "id").unwrap_or_else(new_id),
                    time: time.to_string(),
                    text,
                });
            }
        }
        Some("function_call") => {
            events.push(SessionLogEvent::ToolCall {
                id: string_field(item, "call_id").unwrap_or_else(new_id),
                time: time.to_string(),
                tool: string_field(item, "name").unwrap_or_else(|| "function".to_string()),
                input: parse_json_string_field(item, "arguments"),
                cwd: None,
            });
        }
        Some("custom_tool_call") => {
            events.push(SessionLogEvent::ToolCall {
                id: string_field(item, "call_id").unwrap_or_else(new_id),
                time: time.to_string(),
                tool: string_field(item, "name").unwrap_or_else(|| "tool".to_string()),
                input: parse_json_string_field(item, "input"),
                cwd: None,
            });
        }
        Some("function_call_output") | Some("custom_tool_call_output") => {
            events.push(SessionLogEvent::ToolResult {
                id: string_field(item, "call_id").unwrap_or_else(new_id),
                time: time.to_string(),
                tool: string_field(item, "name").unwrap_or_else(|| "tool".to_string()),
                output: item.get("output").map(output_to_text),
                status: Some("completed".to_string()),
                error: None,
                exit_code: None,
                duration_ms: None,
            });
        }
        Some("local_shell_call") => {
            let action = item.get("action").cloned().unwrap_or(Value::Null);
            events.push(SessionLogEvent::ToolCall {
                id: string_field(item, "call_id").unwrap_or_else(new_id),
                time: time.to_string(),
                tool: "shell".to_string(),
                input: action,
                cwd: None,
            });
        }
        Some("web_search_call") => {
            events.push(SessionLogEvent::ToolCall {
                id: string_field(item, "id").unwrap_or_else(new_id),
                time: time.to_string(),
                tool: "web_search".to_string(),
                input: item.get("action").cloned().unwrap_or(Value::Null),
                cwd: None,
            });
        }
        _ => {}
    }
}

fn parse_codex_event_msg(item: &Value, time: &str, events: &mut Vec<SessionLogEvent>) {
    match item.get("type").and_then(Value::as_str) {
        Some("exec_command_begin") => {
            events.push(SessionLogEvent::ToolCall {
                id: string_field(item, "call_id").unwrap_or_else(new_id),
                time: time.to_string(),
                tool: "bash".to_string(),
                input: json!({ "command": item.get("command").cloned().unwrap_or(Value::Null) }),
                cwd: string_field(item, "cwd"),
            });
        }
        Some("exec_command_end") => {
            let duration_ms = string_field(item, "duration")
                .and_then(|value| parse_duration_ms(&value))
                .or_else(|| int_field(item, "duration_ms").and_then(|v| u64::try_from(v).ok()));
            events.push(SessionLogEvent::ToolResult {
                id: string_field(item, "call_id").unwrap_or_else(new_id),
                time: time.to_string(),
                tool: "bash".to_string(),
                output: string_field(item, "aggregated_output")
                    .or_else(|| string_field(item, "formatted_output"))
                    .or_else(|| string_field(item, "stdout")),
                status: string_field(item, "status"),
                error: string_field(item, "stderr").filter(|value| !value.trim().is_empty()),
                exit_code: int_field(item, "exit_code").and_then(|v| i32::try_from(v).ok()),
                duration_ms,
            });
        }
        Some("context_compacted") => {
            events.push(SessionLogEvent::Compact {
                id: new_id(),
                time: time.to_string(),
                summary: "Context compacted".to_string(),
                auto: None,
            });
        }
        _ => {}
    }
}

fn import_opencode(path: Option<&Path>) -> Result<Vec<FileSession>> {
    if let Some(root) = path {
        return import_opencode_root(root);
    }

    let mut sessions = Vec::new();
    let mut seen = BTreeSet::new();
    for root in default_opencode_roots() {
        let key = root.to_string_lossy().to_string();
        if !seen.insert(key) {
            continue;
        }
        sessions.extend(import_opencode_root(&root)?);
    }
    Ok(sessions)
}

fn import_opencode_root(root: &Path) -> Result<Vec<FileSession>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let db_path = if root.is_file() {
        root.to_path_buf()
    } else {
        root.join("opencode.db")
    };
    if db_path.exists() {
        return import_opencode_db(&db_path);
    }
    import_opencode_json_storage(&root.join("storage"))
}

fn import_opencode_db(db_path: &Path) -> Result<Vec<FileSession>> {
    let sessions: Vec<Value> = sqlite_json(
        db_path,
        "select id, slug, directory, title, version, time_created, time_updated from session order by time_updated desc",
    )?;
    let has_session_entry = sqlite_table_exists(db_path, "session_entry")?;
    let session_ids = sessions
        .iter()
        .filter_map(|session| string_field(session, "id"))
        .collect::<Vec<_>>();
    let bulk_messages = if has_session_entry {
        None
    } else {
        Some(opencode_message_rows_by_session(db_path, &session_ids)?)
    };
    let bulk_parts = if has_session_entry || !sqlite_table_exists(db_path, "part")? {
        None
    } else {
        Some(opencode_part_rows_by_session_message(
            db_path,
            &session_ids,
        )?)
    };
    let mut imported = Vec::new();
    for session in sessions {
        let Some(session_id) = string_field(&session, "id") else {
            continue;
        };
        let (mut events, provider, model) = if has_session_entry {
            let entries = sqlite_json(
                db_path,
                &format!(
                    "select id, type, time_created, data from session_entry where session_id = '{}' order by time_created, id",
                    sql_quote(&session_id)
                ),
            )?;
            if entries.is_empty() {
                let parsed = parse_opencode_message_parts(db_path, &session_id)?;
                (parsed.events, parsed.provider, parsed.model)
            } else {
                let parsed = parse_opencode_entries(entries);
                (parsed.events, parsed.provider, parsed.model)
            }
        } else if let Some(messages) = bulk_messages.as_ref() {
            let parsed = parse_opencode_message_rows(
                messages.get(&session_id).cloned().unwrap_or_default(),
                bulk_parts.as_ref().and_then(|parts| parts.get(&session_id)),
            );
            (parsed.events, parsed.provider, parsed.model)
        } else {
            let parsed = parse_opencode_message_parts(db_path, &session_id)?;
            (parsed.events, parsed.provider, parsed.model)
        };
        if events.is_empty() {
            continue;
        }
        let created_at = int_field(&session, "time_created")
            .map(iso_from_millis)
            .unwrap_or_else(now_iso);
        let updated_at = int_field(&session, "time_updated")
            .map(iso_from_millis)
            .unwrap_or_else(|| created_at.clone());
        let mut meta = SessionMeta::new(session_id, created_at, updated_at);
        meta.title = string_field(&session, "title").or_else(|| first_user_title(&events));
        meta.cwd = string_field(&session, "directory");
        meta.app_version = string_field(&session, "version");
        meta.provider = provider;
        meta.model = model;
        let mut all_events = vec![meta.clone().into_event()];
        all_events.append(&mut events);
        imported.push(FileSession::new(meta, all_events));
    }
    Ok(imported)
}

fn parse_opencode_entries(entries: Vec<Value>) -> OpencodeParsedMessages {
    let mut parsed = OpencodeParsedMessages::default();
    for row in entries {
        let data = row
            .get("data")
            .and_then(Value::as_str)
            .and_then(|text| serde_json::from_str::<Value>(text).ok())
            .unwrap_or_else(|| row.get("data").cloned().unwrap_or(Value::Null));
        let kind = string_field(&row, "type")
            .or_else(|| string_field(&data, "type"))
            .unwrap_or_default();
        let time = int_field(&row, "time_created")
            .map(iso_from_millis)
            .or_else(|| {
                data.get("time")
                    .and_then(|time| string_field(time, "created"))
            })
            .unwrap_or_else(now_iso);
        let id = string_field(&row, "id")
            .or_else(|| string_field(&data, "id"))
            .unwrap_or_else(new_id);
        merge_opencode_provider_model(&mut parsed, &data);
        match kind.as_str() {
            "user" => {
                if let Some(text) = string_field(&data, "text")
                    && !text.trim().is_empty()
                {
                    parsed.events.push(SessionLogEvent::Message {
                        id,
                        time,
                        role: SessionMessageRole::User,
                        text,
                        execution: None,
                        media: None,
                        transcript: None,
                    });
                }
            }
            "synthetic" => {
                if let Some(text) = string_field(&data, "text") {
                    parsed.events.push(SessionLogEvent::Message {
                        id,
                        time,
                        role: SessionMessageRole::System,
                        text,
                        execution: None,
                        media: None,
                        transcript: None,
                    });
                }
            }
            "assistant" => parse_opencode_assistant(&id, &time, &data, &mut parsed.events),
            "compaction" | "compacted" => {
                parsed.events.push(SessionLogEvent::Compact {
                    id,
                    time,
                    summary: "Context compacted".to_string(),
                    auto: data.get("auto").and_then(Value::as_bool),
                });
            }
            _ => {}
        }
    }
    parsed
}

fn parse_opencode_assistant(id: &str, time: &str, data: &Value, events: &mut Vec<SessionLogEvent>) {
    if let Some(content) = data.get("content").and_then(Value::as_array) {
        for (index, part) in content.iter().enumerate() {
            match part.get("type").and_then(Value::as_str) {
                Some("text") => {
                    if let Some(text) = string_field(part, "text")
                        && !text.trim().is_empty()
                    {
                        events.push(SessionLogEvent::Message {
                            id: format!("{id}-{index}"),
                            time: time.to_string(),
                            role: SessionMessageRole::Assistant,
                            text,
                            execution: None,
                            media: None,
                            transcript: None,
                        });
                    }
                }
                Some("reasoning") => {
                    if let Some(text) = string_field(part, "text")
                        && !text.trim().is_empty()
                    {
                        events.push(SessionLogEvent::Reasoning {
                            id: format!("{id}-{index}"),
                            time: time.to_string(),
                            text,
                        });
                    }
                }
                Some("tool") => {
                    parse_opencode_tool_part(id, index, time, part, events);
                }
                _ => {}
            }
        }
    }
    if let Some(tokens) = data.get("tokens") {
        events.push(SessionLogEvent::Usage {
            time: time.to_string(),
            usage: opencode_usage_values(tokens, data.get("cost").and_then(Value::as_f64)),
        });
    }
}

fn opencode_usage_values(tokens: &Value, cost: Option<f64>) -> UsageValues {
    UsageValues {
        input_tokens: int_field(tokens, "input"),
        output_tokens: int_field(tokens, "output"),
        reasoning_tokens: int_field(tokens, "reasoning"),
        cache_read: tokens
            .get("cache")
            .and_then(|cache| int_field(cache, "read")),
        cache_write: tokens
            .get("cache")
            .and_then(|cache| int_field(cache, "write")),
        cost,
    }
}

fn merge_opencode_provider_model(parsed: &mut OpencodeParsedMessages, data: &Value) {
    parsed.provider = parsed.provider.clone().or_else(|| {
        string_field(data, "providerID")
            .or_else(|| string_field(data, "model_providerID"))
            .or_else(|| {
                data.get("model")
                    .and_then(|model| string_field(model, "providerID"))
            })
    });
    parsed.model = parsed.model.clone().or_else(|| {
        string_field(data, "modelID")
            .or_else(|| string_field(data, "model_modelID"))
            .or_else(|| {
                data.get("model")
                    .and_then(|model| string_field(model, "modelID"))
            })
    });
    if parsed.provider.is_none() {
        parsed.provider = parsed.model.as_deref().and_then(infer_provider_from_model);
    }
}

fn parse_opencode_tool_part(
    id: &str,
    index: usize,
    time: &str,
    part: &Value,
    events: &mut Vec<SessionLogEvent>,
) {
    let state = part.get("state").unwrap_or(&Value::Null);
    let call_id = string_field(part, "callID").unwrap_or_else(|| format!("{id}-{index}"));
    let tool = string_field(part, "tool")
        .or_else(|| string_field(part, "name"))
        .unwrap_or_else(|| "tool".to_string());
    events.push(SessionLogEvent::ToolCall {
        id: call_id.clone(),
        time: time.to_string(),
        tool: tool.clone(),
        input: state.get("input").cloned().unwrap_or(Value::Null),
        cwd: None,
    });
    if matches!(
        string_field(state, "status").as_deref(),
        Some("completed") | Some("error")
    ) {
        events.push(SessionLogEvent::ToolResult {
            id: call_id,
            time: time.to_string(),
            tool,
            output: string_field(state, "output"),
            status: string_field(state, "status"),
            error: string_field(state, "error"),
            exit_code: None,
            duration_ms: None,
        });
    }
}

#[derive(Debug, Default)]
struct OpencodeParsedMessages {
    events: Vec<SessionLogEvent>,
    provider: Option<String>,
    model: Option<String>,
}

fn parse_opencode_message_parts(
    db_path: &Path,
    session_id: &str,
) -> Result<OpencodeParsedMessages> {
    let rows = opencode_message_rows_for_session(db_path, session_id)?;
    let has_part_table = sqlite_table_exists(db_path, "part")?;
    let parts_by_message = if has_part_table {
        Some(opencode_parts_by_message(db_path, session_id)?)
    } else {
        None
    };
    Ok(parse_opencode_message_rows(rows, parts_by_message.as_ref()))
}

fn opencode_message_rows_for_session(db_path: &Path, session_id: &str) -> Result<Vec<Value>> {
    sqlite_json(
        db_path,
        &format!(
            "select id, time_created, data from message where session_id = '{}' order by time_created, id",
            sql_quote(session_id)
        ),
    )
}

fn opencode_message_rows_by_session(
    db_path: &Path,
    session_ids: &[String],
) -> Result<BTreeMap<String, Vec<Value>>> {
    let mut by_session = BTreeMap::<String, Vec<Value>>::new();
    for chunk in session_ids.chunks(32) {
        let rows = sqlite_json(
            db_path,
            &format!(
                "select id, session_id, time_created, json_extract(data, '$.role') as role, json_extract(data, '$.providerID') as providerID, json_extract(data, '$.modelID') as modelID, json_extract(data, '$.model.providerID') as model_providerID, json_extract(data, '$.model.modelID') as model_modelID, json_extract(data, '$.tokens') as tokens, json_extract(data, '$.cost') as cost, json_extract(data, '$.content') as content, json_extract(data, '$.text') as text from message where session_id in ({})",
                sql_in_list(chunk)
            ),
        )?;
        for row in rows {
            let Some(session_id) = string_field(&row, "session_id") else {
                continue;
            };
            by_session.entry(session_id).or_default().push(row);
        }
    }
    sort_opencode_rows_by_time(&mut by_session);
    Ok(by_session)
}

fn parse_opencode_message_rows(
    rows: Vec<Value>,
    parts_by_message: Option<&BTreeMap<String, Vec<Value>>>,
) -> OpencodeParsedMessages {
    let mut parsed = OpencodeParsedMessages::default();
    for row in rows {
        let id = string_field(&row, "id").unwrap_or_else(new_id);
        let time = int_field(&row, "time_created")
            .map(iso_from_millis)
            .unwrap_or_else(now_iso);
        let data = opencode_row_data(&row);
        merge_opencode_provider_model(&mut parsed, &data);
        let role = match string_field(&data, "role").as_deref() {
            Some("assistant") => SessionMessageRole::Assistant,
            Some("system") => SessionMessageRole::System,
            _ => SessionMessageRole::User,
        };
        let mut emitted_message = false;
        if let Some(parts_by_message) = parts_by_message {
            for (index, part_row) in parts_by_message
                .get(&id)
                .into_iter()
                .flat_map(|parts| parts.iter())
                .enumerate()
            {
                let part_id = string_field(part_row, "id").unwrap_or_else(new_id);
                let part_time = int_field(part_row, "time_created")
                    .map(iso_from_millis)
                    .unwrap_or_else(|| time.clone());
                let part = opencode_row_data(part_row);
                match part.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(text) = string_field(&part, "text")
                            && !text.trim().is_empty()
                        {
                            parsed.events.push(SessionLogEvent::Message {
                                id: part_id,
                                time: part_time,
                                role: role.clone(),
                                text,
                                execution: None,
                                media: None,
                                transcript: None,
                            });
                            emitted_message = true;
                        }
                    }
                    Some("reasoning") => {
                        if let Some(text) = string_field(&part, "text")
                            && !text.trim().is_empty()
                        {
                            parsed.events.push(SessionLogEvent::Reasoning {
                                id: part_id,
                                time: part_time,
                                text,
                            });
                        }
                    }
                    Some("tool") => {
                        parse_opencode_tool_part(&id, index, &part_time, &part, &mut parsed.events);
                    }
                    Some("compaction") => {
                        parsed.events.push(SessionLogEvent::Compact {
                            id: part_id,
                            time: part_time,
                            summary: "Context compacted".to_string(),
                            auto: part.get("auto").and_then(Value::as_bool),
                        });
                    }
                    Some("step-finish") => {
                        if let Some(tokens) = part.get("tokens") {
                            parsed.events.push(SessionLogEvent::Usage {
                                time: part_time,
                                usage: opencode_usage_values(
                                    tokens,
                                    part.get("cost").and_then(Value::as_f64),
                                ),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
        let text = data
            .get("content")
            .map(text_from_content)
            .or_else(|| string_field(&data, "text"))
            .unwrap_or_default();
        if !emitted_message && !text.trim().is_empty() {
            parsed.events.push(SessionLogEvent::Message {
                id,
                time: time.clone(),
                role,
                text,
                execution: None,
                media: None,
                transcript: None,
            });
        }
        if let Some(tokens) = data.get("tokens") {
            parsed.events.push(SessionLogEvent::Usage {
                time,
                usage: opencode_usage_values(tokens, data.get("cost").and_then(Value::as_f64)),
            });
        }
    }
    parsed
}

fn opencode_row_data(row: &Value) -> Value {
    if let Some(data) = row
        .get("data")
        .and_then(Value::as_str)
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
    {
        return data;
    }
    let Some(object) = row.as_object() else {
        return row.clone();
    };
    let mut data = Map::new();
    for (key, value) in object {
        if matches!(
            key.as_str(),
            "id" | "session_id" | "message_id" | "time_created"
        ) {
            continue;
        }
        data.insert(key.clone(), parse_json_extracted_value(value));
    }
    Value::Object(data)
}

fn parse_json_extracted_value(value: &Value) -> Value {
    match value {
        Value::String(text) if text.starts_with('{') || text.starts_with('[') => {
            serde_json::from_str(text).unwrap_or_else(|_| value.clone())
        }
        _ => value.clone(),
    }
}

fn opencode_parts_by_message(
    db_path: &Path,
    session_id: &str,
) -> Result<BTreeMap<String, Vec<Value>>> {
    let rows = sqlite_json(
        db_path,
        &format!(
            "select id, message_id, time_created, json_extract(data, '$.type') as type, json_extract(data, '$.text') as text, json_extract(data, '$.callID') as callID, json_extract(data, '$.tool') as tool, json_extract(data, '$.name') as name, json_extract(data, '$.state') as state, json_extract(data, '$.tokens') as tokens, json_extract(data, '$.cost') as cost, json_extract(data, '$.auto') as auto from part where session_id = '{}' and {} order by time_created, id",
            sql_quote(session_id),
            opencode_part_type_filter_sql()
        ),
    )?;
    let mut by_message = BTreeMap::<String, Vec<Value>>::new();
    for row in rows {
        let Some(message_id) = string_field(&row, "message_id") else {
            continue;
        };
        by_message.entry(message_id).or_default().push(row);
    }
    Ok(by_message)
}

fn opencode_part_rows_by_session_message(
    db_path: &Path,
    session_ids: &[String],
) -> Result<BTreeMap<String, BTreeMap<String, Vec<Value>>>> {
    let mut by_session = BTreeMap::<String, BTreeMap<String, Vec<Value>>>::new();
    for chunk in session_ids.chunks(32) {
        let rows = sqlite_json(
            db_path,
            &format!(
                "select id, session_id, message_id, time_created, json_extract(data, '$.type') as type, json_extract(data, '$.text') as text, json_extract(data, '$.callID') as callID, json_extract(data, '$.tool') as tool, json_extract(data, '$.name') as name, json_extract(data, '$.state') as state, json_extract(data, '$.tokens') as tokens, json_extract(data, '$.cost') as cost, json_extract(data, '$.auto') as auto from part where session_id in ({}) and {}",
                sql_in_list(chunk),
                opencode_part_type_filter_sql()
            ),
        )?;
        for row in rows {
            let Some(session_id) = string_field(&row, "session_id") else {
                continue;
            };
            let Some(message_id) = string_field(&row, "message_id") else {
                continue;
            };
            by_session
                .entry(session_id)
                .or_default()
                .entry(message_id)
                .or_default()
                .push(row);
        }
    }
    for by_message in by_session.values_mut() {
        sort_opencode_rows_by_time(by_message);
    }
    Ok(by_session)
}

fn sort_opencode_rows_by_time(rows_by_key: &mut BTreeMap<String, Vec<Value>>) {
    for rows in rows_by_key.values_mut() {
        rows.sort_by(|left, right| {
            int_field(left, "time_created")
                .cmp(&int_field(right, "time_created"))
                .then_with(|| string_field(left, "id").cmp(&string_field(right, "id")))
        });
    }
}

fn import_opencode_json_storage(storage_dir: &Path) -> Result<Vec<FileSession>> {
    if !storage_dir.exists() {
        return Ok(Vec::new());
    }
    let mut sessions = Vec::new();
    for file in json_files(&storage_dir.join("session"))? {
        let data: Value = serde_json::from_reader(File::open(&file)?)?;
        let id = file
            .file_stem()
            .and_then(|v| v.to_str())
            .unwrap_or("opencode-session")
            .to_string();
        let message_dir = storage_dir.join("message").join(&id);
        let mut events = Vec::new();
        for message_file in json_files(&message_dir)? {
            let message: Value = serde_json::from_reader(File::open(&message_file)?)?;
            let text = message
                .get("content")
                .map(text_from_content)
                .or_else(|| string_field(&message, "text"))
                .unwrap_or_default();
            if text.trim().is_empty() {
                continue;
            }
            events.push(SessionLogEvent::Message {
                id: message_file
                    .file_stem()
                    .and_then(|v| v.to_str())
                    .unwrap_or("message")
                    .to_string(),
                time: message
                    .get("time")
                    .and_then(|time| int_field(time, "created"))
                    .or_else(|| int_field(&message, "time_created"))
                    .map(iso_from_millis)
                    .unwrap_or_else(now_iso),
                role: match string_field(&message, "role").as_deref() {
                    Some("assistant") => SessionMessageRole::Assistant,
                    Some("system") => SessionMessageRole::System,
                    _ => SessionMessageRole::User,
                },
                text,
                execution: None,
                media: None,
                transcript: None,
            });
        }
        if events.is_empty() {
            continue;
        }
        let created_at = data
            .get("time")
            .and_then(|time| int_field(time, "created"))
            .map(iso_from_millis)
            .unwrap_or_else(now_iso);
        let updated_at = data
            .get("time")
            .and_then(|time| int_field(time, "updated"))
            .map(iso_from_millis)
            .unwrap_or_else(|| created_at.clone());
        let mut meta = SessionMeta::new(id, created_at, updated_at);
        meta.title = string_field(&data, "title").or_else(|| first_user_title(&events));
        meta.cwd = string_field(&data, "directory");
        meta.provider = string_field(&data, "providerID")
            .or_else(|| string_field(&data, "provider"))
            .or_else(|| {
                string_field(&data, "model")
                    .as_deref()
                    .and_then(infer_provider_from_model)
            });
        meta.model = string_field(&data, "modelID").or_else(|| string_field(&data, "model"));
        let mut all_events = vec![meta.clone().into_event()];
        all_events.extend(events);
        sessions.push(FileSession::new(meta, all_events));
    }
    Ok(sessions)
}

fn sqlite_json(db_path: &Path, sql: &str) -> Result<Vec<Value>> {
    let output = Command::new("sqlite3")
        .arg("-readonly")
        .arg("-json")
        .arg(db_path)
        .arg(sql)
        .output()
        .with_context(|| "failed to execute sqlite3; OpenCode database import requires sqlite3")?;
    if !output.status.success() {
        return Err(anyhow!(
            "sqlite3 failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let stdout = String::from_utf8(output.stdout)?;
    if stdout.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&stdout).with_context(|| "failed to parse sqlite3 JSON output")
}

fn sqlite_table_exists(db_path: &Path, table: &str) -> Result<bool> {
    let rows = sqlite_json(
        db_path,
        &format!(
            "select name from sqlite_master where type = 'table' and name = '{}'",
            sql_quote(table)
        ),
    )?;
    Ok(!rows.is_empty())
}

fn read_json_lines(path: &Path) -> Result<Vec<Value>> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut lines = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        lines.push(serde_json::from_str(&line)?);
    }
    Ok(lines)
}

fn jsonl_files(root: &Path, prefix: Option<&str>) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| entry.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|v| v.to_str()) != Some("jsonl") {
            continue;
        }
        if let Some(prefix) = prefix {
            let Some(file_name) = path.file_name().and_then(|v| v.to_str()) else {
                continue;
            };
            if !file_name.starts_with(prefix) {
                continue;
            }
        }
        files.push(path.to_path_buf());
    }
    Ok(files)
}

fn path_has_component(path: &Path, component: &str) -> bool {
    path.components()
        .any(|part| part.as_os_str().to_string_lossy() == component)
}

fn json_files(root: &Path) -> Result<Vec<PathBuf>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| entry.ok())
    {
        if entry.file_type().is_file()
            && entry.path().extension().and_then(|v| v.to_str()) == Some("json")
        {
            files.push(entry.path().to_path_buf());
        }
    }
    Ok(files)
}

fn default_claude_root() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude")
        .join("projects")
}

fn default_codex_root() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codex")
        .join("sessions")
}

fn default_opencode_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(value) = std::env::var("XDG_DATA_HOME") {
        roots.push(PathBuf::from(value).join("opencode"));
    }
    if let Some(home) = home_dir() {
        roots.push(home.join(".local").join("share").join("opencode"));
    }
    if let Some(data_dir) = dirs::data_dir() {
        roots.push(data_dir.join("opencode"));
    }
    roots
}

fn home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}

fn payload(value: &Value) -> &Value {
    value.get("payload").unwrap_or(value)
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .filter(|value| !value.is_empty())
}

fn int_field(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_u64().and_then(|v| i64::try_from(v).ok()))
            .or_else(|| value.as_str().and_then(|v| v.parse().ok()))
    })
}

fn parse_json_string_field(value: &Value, key: &str) -> Value {
    match value.get(key) {
        Some(Value::String(text)) => {
            serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.clone()))
        }
        Some(other) => other.clone(),
        None => Value::Null,
    }
}

fn output_to_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Object(object) => output_object_to_text(object),
        Value::Array(_) => text_from_content(value),
        _ => value.to_string(),
    }
}

fn output_object_to_text(object: &Map<String, Value>) -> String {
    if let Some(content) = object.get("content").and_then(Value::as_str) {
        return content.to_string();
    }
    if let Some(items) = object.get("content_items") {
        let text = text_from_content(items);
        if !text.is_empty() {
            return text;
        }
    }
    Value::Object(object.clone()).to_string()
}

fn first_user_title(events: &[SessionLogEvent]) -> Option<String> {
    events.iter().find_map(|event| {
        let SessionLogEvent::Message {
            role: SessionMessageRole::User,
            text,
            ..
        } = event
        else {
            return None;
        };
        let title: String = text.chars().take(60).collect();
        if title.trim().is_empty() {
            None
        } else if text.chars().count() > 60 {
            Some(format!("{title}..."))
        } else {
            Some(title)
        }
    })
}

fn parse_duration_ms(value: &str) -> Option<u64> {
    if let Some(stripped) = value.strip_suffix("ms") {
        return stripped.trim().parse().ok();
    }
    if let Some(stripped) = value.strip_suffix('s') {
        let seconds: f64 = stripped.trim().parse().ok()?;
        return Some((seconds * 1000.0).round() as u64);
    }
    value.parse().ok()
}

fn sql_quote(value: &str) -> String {
    value.replace('\'', "''")
}

fn infer_provider_from_model(model: &str) -> Option<String> {
    let normalized = model.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }
    if normalized.contains("claude") {
        Some("anthropic".to_string())
    } else if normalized.contains("gpt") || normalized.contains("o3") || normalized.contains("o4") {
        Some("openai".to_string())
    } else if normalized.contains("gemini") {
        Some("gemini".to_string())
    } else {
        None
    }
}

fn opencode_part_type_filter_sql() -> &'static str {
    "(data like '%\"type\":\"text\"%' or data like '%\"type\": \"text\"%' or data like '%\"type\":\"reasoning\"%' or data like '%\"type\": \"reasoning\"%' or data like '%\"type\":\"tool\"%' or data like '%\"type\": \"tool\"%' or data like '%\"type\":\"compaction\"%' or data like '%\"type\": \"compaction\"%' or data like '%\"type\":\"step-finish\"%' or data like '%\"type\": \"step-finish\"%')"
}

fn sql_in_list(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("'{}'", sql_quote(value)))
        .collect::<Vec<_>>()
        .join(",")
}

fn new_id() -> String {
    Uuid::new_v4().to_string()
}

#[allow(dead_code)]
fn stable_id_if_missing(events: &[SessionLogEvent], id: Option<String>) -> String {
    id.unwrap_or_else(|| stable_session_id(events))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::tempdir;

    #[test]
    fn imports_codex_rollout_jsonl() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("codex");
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("rollout-test.jsonl"),
            concat!(
                "{\"timestamp\":\"2026-05-03T00:00:00.000Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-1\",\"timestamp\":\"2026-05-03T00:00:00.000Z\",\"cwd\":\"/tmp/proj\",\"cli_version\":\"1.0\",\"model_provider\":\"openai\"}}\n",
                "{\"timestamp\":\"2026-05-03T00:00:00.500Z\",\"type\":\"turn_context\",\"payload\":{\"model\":\"gpt-5.4\"}}\n",
                "{\"timestamp\":\"2026-05-03T00:00:01.000Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"hello\"}]}}\n",
                "{\"timestamp\":\"2026-05-03T00:00:02.000Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"hi\"}]}}\n"
            ),
        )
        .unwrap();

        let sessions = import_codex(Some(&source)).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].meta.id, "thread-1");
        assert_eq!(sessions[0].meta.model.as_deref(), Some("gpt-5.4"));
        assert_eq!(sessions[0].to_chat_session().messages.len(), 2);
    }

    #[test]
    fn imports_codex_skips_encrypted_payload_lines() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("codex");
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("rollout-test.jsonl"),
            concat!(
                "{\"timestamp\":\"2026-05-03T00:00:00.000Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-1\",\"timestamp\":\"2026-05-03T00:00:00.000Z\",\"cwd\":\"/tmp/proj\",\"model_provider\":\"openai\"}}\n",
                "{\"timestamp\":\"2026-05-03T00:00:01.000Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"reasoning\",\"summary\":[],\"content\":null,\"encrypted_content\":\"large\"}}\n",
                "{\"timestamp\":\"2026-05-03T00:00:02.000Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"hello\"}]}}\n"
            ),
        )
        .unwrap();

        let sessions = import_codex(Some(&source)).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].to_chat_session().messages.len(), 1);
    }

    #[test]
    fn codex_import_skips_oversized_jsonl_lines_before_parse() {
        let oversized = "x".repeat(MAX_CODEX_IMPORT_LINE_BYTES + 1);
        assert!(skip_codex_line_before_parse(&oversized));
    }

    #[test]
    fn codex_large_file_import_preserves_session_meta_as_stub() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("rollout-large.jsonl");
        fs::write(
            &file,
            concat!(
                "{\"timestamp\":\"2026-05-03T00:00:00.000Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-large\",\"timestamp\":\"2026-05-03T00:00:00.000Z\",\"cwd\":\"/tmp/proj\",\"cli_version\":\"1.0\",\"model_provider\":\"openai\"}}\n",
                "{\"timestamp\":\"2026-05-03T00:00:01.000Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"hello\"}]}}\n"
            ),
        )
        .unwrap();

        let session = parse_codex_large_file_stub(&file, MAX_CODEX_IMPORT_FILE_BYTES + 1).unwrap();
        let session = session.expect("large Codex rollout should preserve session metadata");
        assert_eq!(session.meta.id, "thread-large");
        assert_eq!(session.meta.cwd.as_deref(), Some("/tmp/proj"));
        assert_eq!(session.meta.provider.as_deref(), Some("openai"));
        assert!(matches!(
            session.events.last(),
            Some(SessionLogEvent::Compact { summary, auto, .. })
                if summary.contains("transcript omitted") && auto == &Some(true)
        ));
    }

    #[test]
    fn imports_claude_transcript_jsonl() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("claude");
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("session-1.jsonl"),
            concat!(
                "{\"type\":\"user\",\"uuid\":\"u1\",\"sessionId\":\"s1\",\"timestamp\":\"2026-05-03T00:00:00.000Z\",\"cwd\":\"/tmp/proj\",\"version\":\"1.0\",\"message\":{\"role\":\"user\",\"content\":\"hello\"}}\n",
                "{\"type\":\"assistant\",\"uuid\":\"a1\",\"sessionId\":\"s1\",\"timestamp\":\"2026-05-03T00:00:01.000Z\",\"message\":{\"role\":\"assistant\",\"model\":\"claude\",\"content\":[{\"type\":\"text\",\"text\":\"hi\"}],\"usage\":{\"input_tokens\":1,\"output_tokens\":2}}}\n"
            ),
        )
        .unwrap();

        let sessions = import_claude(Some(&source)).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].meta.id, "s1");
        assert_eq!(sessions[0].meta.provider.as_deref(), Some("anthropic"));
        assert_eq!(sessions[0].to_chat_session().messages.len(), 2);
    }

    #[test]
    fn imports_claude_tool_results_as_tool_result_events() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("claude");
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("session-1.jsonl"),
            concat!(
                "{\"type\":\"assistant\",\"uuid\":\"a1\",\"sessionId\":\"s1\",\"timestamp\":\"2026-05-03T00:00:00.000Z\",\"message\":{\"role\":\"assistant\",\"model\":\"claude\",\"content\":[{\"type\":\"tool_use\",\"id\":\"tool-1\",\"name\":\"Bash\",\"input\":{\"command\":\"pwd\"}}]}}\n",
                "{\"type\":\"user\",\"uuid\":\"u1\",\"sessionId\":\"s1\",\"timestamp\":\"2026-05-03T00:00:01.000Z\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"tool-1\",\"content\":\"/tmp\"}]}}\n"
            ),
        )
        .unwrap();

        let sessions = import_claude(Some(&source)).unwrap();
        assert_eq!(sessions.len(), 1);
        assert!(matches!(
            sessions[0].events.iter().find(|event| matches!(event, SessionLogEvent::ToolResult { .. })),
            Some(SessionLogEvent::ToolResult { id, output, .. }) if id == "tool-1" && output.as_deref() == Some("/tmp")
        ));
    }

    #[test]
    fn imports_claude_skips_subagent_transcripts() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("claude");
        let subagents = source.join("project").join("subagents");
        fs::create_dir_all(&subagents).unwrap();
        fs::write(
            subagents.join("agent-a.jsonl"),
            "{\"type\":\"user\",\"uuid\":\"u1\",\"sessionId\":\"sub-1\",\"timestamp\":\"2026-05-03T00:00:00.000Z\",\"message\":{\"role\":\"user\",\"content\":\"internal\"}}\n",
        )
        .unwrap();
        fs::write(
            source.join("session-1.jsonl"),
            "{\"type\":\"user\",\"uuid\":\"u2\",\"sessionId\":\"main-1\",\"timestamp\":\"2026-05-03T00:00:00.000Z\",\"message\":{\"role\":\"user\",\"content\":\"hello\"}}\n",
        )
        .unwrap();

        let sessions = import_claude(Some(&source)).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].meta.id, "main-1");
    }

    #[test]
    fn import_report_counts_dry_run() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("codex");
        let target = dir.path().join("restflow");
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("rollout-test.jsonl"),
            "{\"timestamp\":\"2026-05-03T00:00:00.000Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"hello\"}]}}\n",
        )
        .unwrap();
        let store = FileSessionStore::new(target).unwrap();
        let report = import_sessions(
            &store,
            ImportOptions {
                source: ImportSource::Codex,
                path: Some(source),
                dry_run: true,
                force: false,
            },
        )
        .unwrap();
        assert_eq!(report.imported, 1);
    }

    #[test]
    fn imports_opencode_sqlite_message_parts_without_session_entry() {
        if Command::new("sqlite3").arg("-version").output().is_err() {
            return;
        }

        let dir = tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        let sql = concat!(
            "create table session (id text primary key, slug text not null, directory text not null, title text not null, version text not null, time_created integer not null, time_updated integer not null);",
            "create table message (id text primary key, session_id text not null, time_created integer not null, time_updated integer not null, data text not null);",
            "create table part (id text primary key, message_id text not null, session_id text not null, time_created integer not null, time_updated integer not null, data text not null);",
            "insert into session values ('ses_1','test','/tmp/project','OpenCode import','0.1',1777852800000,1777852802000);",
            "insert into message values ('msg_1','ses_1',1777852801000,1777852801000,'{\"role\":\"user\",\"agent\":\"build\",\"model\":{\"providerID\":\"openai\",\"modelID\":\"gpt-5.4\"}}');",
            "insert into part values ('part_1','msg_1','ses_1',1777852801000,1777852801000,'{\"type\":\"text\",\"text\":\"hello\"}');",
            "insert into message values ('msg_2','ses_1',1777852802000,1777852802000,'{\"role\":\"assistant\",\"providerID\":\"openai\",\"modelID\":\"gpt-5.4\",\"tokens\":{\"input\":1,\"output\":2,\"reasoning\":0,\"cache\":{\"read\":0,\"write\":0}},\"cost\":0.1}');",
            "insert into part values ('part_2','msg_2','ses_1',1777852802000,1777852802000,'{\"type\":\"text\",\"text\":\"hi\"}');"
        );
        let status = Command::new("sqlite3").arg(&db).arg(sql).status().unwrap();
        assert!(status.success());

        let sessions = import_opencode_db(&db).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].meta.id, "ses_1");
        assert_eq!(sessions[0].meta.provider.as_deref(), Some("openai"));
        assert_eq!(sessions[0].meta.model.as_deref(), Some("gpt-5.4"));
        assert_eq!(sessions[0].to_chat_session().messages.len(), 2);
    }

    #[test]
    fn imports_opencode_session_entry_provider_model() {
        if Command::new("sqlite3").arg("-version").output().is_err() {
            return;
        }

        let dir = tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        let sql = concat!(
            "create table session (id text primary key, slug text not null, directory text not null, title text not null, version text not null, time_created integer not null, time_updated integer not null);",
            "create table session_entry (id text primary key, session_id text not null, type text not null, time_created integer not null, time_updated integer not null, data text not null);",
            "insert into session values ('ses_1','test','/tmp/project','OpenCode import','0.1',1777852800000,1777852802000);",
            "insert into session_entry values ('entry_1','ses_1','assistant',1777852801000,1777852801000,'{\"providerID\":\"openai\",\"modelID\":\"gpt-5.4\",\"content\":[{\"type\":\"text\",\"text\":\"hi\"}]}');"
        );
        let status = Command::new("sqlite3").arg(&db).arg(sql).status().unwrap();
        assert!(status.success());

        let sessions = import_opencode_db(&db).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].meta.provider.as_deref(), Some("openai"));
        assert_eq!(sessions[0].meta.model.as_deref(), Some("gpt-5.4"));
    }

    #[test]
    fn opencode_dry_run_counts_sessions_without_parsing_messages() {
        if Command::new("sqlite3").arg("-version").output().is_err() {
            return;
        }

        let dir = tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        let sql = concat!(
            "create table session (id text primary key, slug text not null, directory text not null, title text not null, version text not null, time_created integer not null, time_updated integer not null);",
            "insert into session values ('ses_1','test','/tmp/project','OpenCode import','0.1',1777852800000,1777852802000);"
        );
        let status = Command::new("sqlite3").arg(&db).arg(sql).status().unwrap();
        assert!(status.success());

        let target = dir.path().join("restflow");
        let store = FileSessionStore::new(target).unwrap();
        let report = import_sessions(
            &store,
            ImportOptions {
                source: ImportSource::Opencode,
                path: Some(dir.path().to_path_buf()),
                dry_run: true,
                force: false,
            },
        )
        .unwrap();

        assert_eq!(report.imported, 1);
        assert_eq!(report.sources[0].discovered, 1);
    }
}
