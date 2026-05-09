use std::collections::BTreeMap;

use runtime::models::{RunKind, RunSummary};
use serde_json::Value;
use types::{StreamEventKind, TaskStreamEvent};

use crate::transcript::{MessageGroup, TranscriptCell, TranscriptCellKind};

const MAX_ACTIVITY_ROWS: usize = 5;
const MAX_BACKGROUND_TERMINAL_ENTRIES: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityEntry {
    pub id: String,
    pub title: String,
    pub status: String,
    pub detail: String,
    pub run_id: Option<String>,
    pub is_active: bool,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActivityState {
    pub revision: u64,
    tools: BTreeMap<String, ActivityEntry>,
    subagents: BTreeMap<String, ActivityEntry>,
}

impl ActivityState {
    pub fn clear(&mut self) {
        if self.tools.is_empty() && self.subagents.is_empty() {
            return;
        }
        self.tools.clear();
        self.subagents.clear();
        self.bump();
    }

    pub fn record_tool_call(&mut self, call_id: &str, name: &str, body: &str) {
        let entry = ActivityEntry {
            id: call_id.to_string(),
            title: if is_subagent_tool(name) {
                subagent_activity_title(name)
            } else {
                name.to_string()
            },
            status: "running".to_string(),
            detail: compact_detail(body),
            run_id: None,
            is_active: true,
            updated_at: 0,
        };
        if is_subagent_tool(name) {
            self.subagents.insert(call_id.to_string(), entry);
        } else {
            self.tools.insert(call_id.to_string(), entry);
        }
        self.bump();
    }

    pub fn record_tool_result(&mut self, call_id: &str, success: bool, body: &str) {
        let status = if success { "completed" } else { "failed" };
        if let Some(entry) = self.subagents.get_mut(call_id) {
            entry.status = status.to_string();
            entry.detail = compact_detail(body);
            entry.is_active = false;
            self.bump();
            return;
        }
        if let Some(entry) = self.tools.get_mut(call_id) {
            entry.status = status.to_string();
            entry.detail = compact_detail(body);
            entry.is_active = false;
            self.bump();
        }
    }

    pub fn sync_child_runs(&mut self, runs: &[RunSummary], active_run_id: Option<&str>) {
        let mut changed = false;
        for run in runs {
            if run.kind != RunKind::SubagentRun || !run_matches_active_turn(run, active_run_id) {
                continue;
            }
            let key = run.run_id.as_deref().unwrap_or(run.id.as_str()).to_string();
            let next = ActivityEntry {
                id: key.clone(),
                title: run.title.clone(),
                status: run.status.clone(),
                detail: run
                    .subtitle
                    .clone()
                    .unwrap_or_else(|| run.provider_model_label()),
                run_id: run.run_id.clone().or_else(|| Some(run.id.clone())),
                is_active: is_running_status(&run.status),
                updated_at: 0,
            };
            if self.subagents.get(&key) != Some(&next) {
                self.subagents.insert(key, next);
                changed = true;
            }
        }
        if changed {
            self.bump();
        }
    }

    #[cfg(test)]
    pub fn live_cells(&self) -> Vec<TranscriptCell> {
        let mut cells = Vec::new();
        if !self.tools.is_empty() {
            cells.push(group_cell(
                TranscriptCellKind::Tool,
                "Tool activity",
                &self.tools,
                MessageGroup::ToolActivity,
            ));
        }
        if !self.subagents.is_empty() {
            cells.push(group_cell(
                TranscriptCellKind::Subagent,
                "Subagents",
                &self.subagents,
                MessageGroup::ToolActivity,
            ));
        }
        cells
    }

    pub fn subagent_live_cells(&self) -> Vec<TranscriptCell> {
        if self.subagents.is_empty() {
            return Vec::new();
        }
        vec![group_cell(
            TranscriptCellKind::Subagent,
            "Subagents",
            &self.subagents,
            MessageGroup::ToolActivity,
        )]
    }

    pub fn has_subagent_activity(&self) -> bool {
        !self.subagents.is_empty()
    }

    fn bump(&mut self) {
        self.revision = self.revision.saturating_add(1);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BackgroundWorkStatus {
    pub revision: u64,
    entries: BTreeMap<String, ActivityEntry>,
}

impl BackgroundWorkStatus {
    pub fn clear(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.entries.clear();
        self.bump();
    }

    pub fn clear_terminal_entries(&mut self) {
        let before = self.entries.len();
        self.entries.retain(|_, entry| !is_terminal_entry(entry));
        if self.entries.len() != before {
            self.bump();
        }
    }

    pub fn record_task_event(&mut self, event: &TaskStreamEvent, detail: String) {
        let (status, is_active) = match &event.kind {
            StreamEventKind::Completed { .. } => ("completed", false),
            StreamEventKind::Failed { .. } => ("failed", false),
            StreamEventKind::Interrupted { .. } => ("interrupted", false),
            StreamEventKind::Started { .. }
            | StreamEventKind::Output { .. }
            | StreamEventKind::Progress { .. }
            | StreamEventKind::Heartbeat { .. } => ("running", true),
        };
        let existing = self.entries.get(&event.task_id);
        let title = task_title(event, existing);
        let run_id = event
            .run_id
            .clone()
            .or_else(|| existing.and_then(|entry| entry.run_id.clone()));
        self.entries.insert(
            event.task_id.clone(),
            ActivityEntry {
                id: event.task_id.clone(),
                title,
                status: status.to_string(),
                detail: compact_detail(&detail),
                run_id,
                is_active,
                updated_at: event.timestamp,
            },
        );
        if is_terminal_status(status) {
            self.prune_terminal_entries();
        }
        self.bump();
    }

    pub fn footer_label(&self) -> Option<String> {
        let running = self
            .entries
            .values()
            .filter(|entry| entry.is_active || is_running_status(&entry.status))
            .count();
        if self.entries.is_empty() {
            return None;
        }
        let completed = self
            .entries
            .values()
            .filter(|entry| entry.status.eq_ignore_ascii_case("completed"))
            .count();
        let failed = self
            .entries
            .values()
            .filter(|entry| {
                matches!(
                    entry.status.trim().to_ascii_lowercase().as_str(),
                    "failed" | "interrupted"
                )
            })
            .count();
        let mut parts = Vec::new();
        if running > 0 {
            parts.push(format!("{running} running"));
        }
        if completed > 0 {
            parts.push(format!("{completed} done"));
        }
        if failed > 0 {
            parts.push(format!("{failed} failed"));
        }
        Some(format!("Work {}", parts.join(" · ")))
    }

    fn bump(&mut self) {
        self.revision = self.revision.saturating_add(1);
    }

    fn prune_terminal_entries(&mut self) {
        let terminal_count = self
            .entries
            .values()
            .filter(|entry| is_terminal_entry(entry))
            .count();
        let remove_count = terminal_count.saturating_sub(MAX_BACKGROUND_TERMINAL_ENTRIES);
        if remove_count == 0 {
            return;
        }

        let mut terminal_entries = self
            .entries
            .values()
            .filter(|entry| is_terminal_entry(entry))
            .map(|entry| (entry.updated_at, entry.id.clone()))
            .collect::<Vec<_>>();
        terminal_entries.sort();
        for (_, id) in terminal_entries.into_iter().take(remove_count) {
            self.entries.remove(&id);
        }
    }
}

trait RunSummaryExt {
    fn provider_model_label(&self) -> String;
}

impl RunSummaryExt for RunSummary {
    fn provider_model_label(&self) -> String {
        match (&self.provider, &self.effective_model) {
            (Some(provider), Some(model)) => format!("{provider} · {model}"),
            (Some(provider), None) => provider.clone(),
            (None, Some(model)) => model.clone(),
            (None, None) => "child run".to_string(),
        }
    }
}

fn group_cell(
    kind: TranscriptCellKind,
    title: &str,
    entries: &BTreeMap<String, ActivityEntry>,
    group: MessageGroup,
) -> TranscriptCell {
    let active = entries.values().any(|entry| entry.is_active);
    let running = entries
        .values()
        .filter(|entry| entry.is_active || is_running_status(&entry.status))
        .count();
    let subtitle = if active {
        Some(format!("running · {running}/{}", entries.len()))
    } else {
        Some(format!("updated · {}", entries.len()))
    };
    let mut lines = Vec::new();
    for entry in entries.values().take(MAX_ACTIVITY_ROWS) {
        let run = entry
            .run_id
            .as_ref()
            .map(|run_id| format!(" · run {}", short_id(run_id)))
            .unwrap_or_default();
        let detail = if entry.detail.trim().is_empty() {
            String::new()
        } else {
            format!(" · {}", entry.detail.trim())
        };
        lines.push(format!(
            "- {} · {}{}{}",
            entry.title, entry.status, run, detail
        ));
    }
    if entries.len() > MAX_ACTIVITY_ROWS {
        lines.push(format!("+{} more", entries.len() - MAX_ACTIVITY_ROWS));
    }

    TranscriptCell {
        kind,
        title: title.to_string(),
        subtitle,
        body: lines.join("\n"),
        group,
        is_active: active,
    }
}

fn is_subagent_tool(name: &str) -> bool {
    matches!(
        name,
        "spawn_subagent_batch" | "spawn_subagent" | "wait_subagents"
    )
}

fn subagent_activity_title(name: &str) -> String {
    match name {
        "wait_subagents" => "wait".to_string(),
        "spawn_subagent_batch" => "batch".to_string(),
        "spawn_subagent" => "spawn".to_string(),
        _ => "subagent".to_string(),
    }
}

fn task_title(event: &TaskStreamEvent, existing: Option<&ActivityEntry>) -> String {
    match &event.kind {
        StreamEventKind::Started { task_name, .. } if !task_name.trim().is_empty() => {
            task_name.trim().to_string()
        }
        _ => existing
            .map(|entry| entry.title.clone())
            .unwrap_or_else(|| event.task_id.clone()),
    }
}

fn is_terminal_entry(entry: &ActivityEntry) -> bool {
    !entry.is_active && is_terminal_status(&entry.status)
}

fn is_terminal_status(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "completed" | "failed" | "interrupted"
    )
}

fn run_matches_active_turn(run: &RunSummary, active_run_id: Option<&str>) -> bool {
    let Some(active_run_id) = active_run_id else {
        return is_running_status(&run.status);
    };
    run.run_id.as_deref() == Some(active_run_id)
        || run.root_run_id.as_deref() == Some(active_run_id)
        || run.parent_run_id.as_deref() == Some(active_run_id)
}

fn is_running_status(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "running" | "active" | "pending" | "starting"
    )
}

fn compact_detail(value: &str) -> String {
    if let Some(detail) = summarize_activity_detail(value) {
        return truncate(&detail, 96);
    }
    let text = value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    truncate(&text, 96)
}

fn summarize_activity_detail(value: &str) -> Option<String> {
    if let Some(error) = text_after_label(value, "Error:") {
        return Some(format!("error: {}", compact_label_text(error)));
    }
    if let Some(output) = json_after_label(value, "Output:")
        && let Some(exit_code) = output.get("exit_code").and_then(Value::as_i64)
    {
        return Some(format!("exit {exit_code}"));
    }
    if let Some(input) = json_after_label(value, "Input:")
        && let Some(command) = input
            .get("command")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|command| !command.is_empty())
    {
        return Some(format!("$ {}", compact_label_text(command)));
    }
    None
}

fn json_after_label(value: &str, label: &str) -> Option<Value> {
    let start = value.find(label)? + label.len();
    let rest = value[start..].trim_start();
    let end = ["\nInput:", "\nOutput:", "\nError:"]
        .iter()
        .filter_map(|marker| rest.find(marker))
        .min()
        .unwrap_or(rest.len());
    serde_json::from_str(rest[..end].trim()).ok()
}

fn text_after_label<'a>(value: &'a str, label: &str) -> Option<&'a str> {
    let start = value.find(label)? + label.len();
    let rest = value[start..].trim_start();
    let end = ["\nInput:", "\nOutput:", "\nError:"]
        .iter()
        .filter_map(|marker| rest.find(marker))
        .min()
        .unwrap_or(rest.len());
    let text = rest[..end].trim();
    (!text.is_empty()).then_some(text)
}

fn compact_label_text(value: &str) -> String {
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut text = value
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    text.push('…');
    text
}

fn short_id(value: &str) -> String {
    value.chars().take(8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::StreamEventKind;

    #[test]
    fn records_multiple_task_events_without_overwriting() {
        let mut state = BackgroundWorkStatus::default();
        state.record_task_event(
            &TaskStreamEvent {
                task_id: "task-1".to_string(),
                run_id: None,
                session_id: None,
                parent_run_id: None,
                scope: None,
                timestamp: 1,
                kind: StreamEventKind::Progress {
                    phase: "checking".to_string(),
                    percent: None,
                    details: None,
                },
            },
            "Task task-1 progress: checking".to_string(),
        );
        state.record_task_event(
            &TaskStreamEvent {
                task_id: "task-2".to_string(),
                run_id: None,
                session_id: None,
                parent_run_id: None,
                scope: None,
                timestamp: 2,
                kind: StreamEventKind::Progress {
                    phase: "building".to_string(),
                    percent: None,
                    details: None,
                },
            },
            "Task task-2 progress: building".to_string(),
        );

        assert_eq!(state.footer_label().as_deref(), Some("Work 2 running"));
    }

    #[test]
    fn retains_terminal_task_events_in_footer() {
        let mut state = BackgroundWorkStatus::default();
        state.record_task_event(
            &TaskStreamEvent {
                task_id: "task-1".to_string(),
                run_id: None,
                session_id: None,
                parent_run_id: None,
                scope: None,
                timestamp: 1,
                kind: StreamEventKind::Progress {
                    phase: "checking".to_string(),
                    percent: None,
                    details: None,
                },
            },
            "Task task-1 progress: checking".to_string(),
        );
        state.record_task_event(
            &TaskStreamEvent {
                task_id: "task-1".to_string(),
                run_id: None,
                session_id: None,
                parent_run_id: None,
                scope: None,
                timestamp: 2,
                kind: StreamEventKind::Completed {
                    result: "Done".to_string(),
                    duration_ms: 42,
                    stats: None,
                },
            },
            "Task task-1 completed: Done".to_string(),
        );
        state.record_task_event(
            &TaskStreamEvent {
                task_id: "task-2".to_string(),
                run_id: None,
                session_id: None,
                parent_run_id: None,
                scope: None,
                timestamp: 3,
                kind: StreamEventKind::Failed {
                    error: "boom".to_string(),
                    error_code: None,
                    duration_ms: 43,
                    recoverable: false,
                },
            },
            "Task task-2 failed: boom".to_string(),
        );

        assert_eq!(
            state.footer_label().as_deref(),
            Some("Work 1 done · 1 failed")
        );
    }

    #[test]
    fn clears_terminal_task_events_without_dropping_running_work() {
        let mut state = BackgroundWorkStatus::default();
        state.record_task_event(
            &TaskStreamEvent {
                task_id: "running-task".to_string(),
                run_id: None,
                session_id: None,
                parent_run_id: None,
                scope: None,
                timestamp: 1,
                kind: StreamEventKind::Progress {
                    phase: "checking".to_string(),
                    percent: None,
                    details: None,
                },
            },
            "Task running-task progress: checking".to_string(),
        );
        state.record_task_event(
            &TaskStreamEvent {
                task_id: "completed-task".to_string(),
                run_id: None,
                session_id: None,
                parent_run_id: None,
                scope: None,
                timestamp: 2,
                kind: StreamEventKind::Completed {
                    result: "Done".to_string(),
                    duration_ms: 42,
                    stats: None,
                },
            },
            "Task completed-task completed: Done".to_string(),
        );

        state.clear_terminal_entries();

        assert!(state.entries.contains_key("running-task"));
        assert!(!state.entries.contains_key("completed-task"));
        assert_eq!(state.footer_label().as_deref(), Some("Work 1 running"));
    }

    #[test]
    fn prunes_old_terminal_task_events_but_keeps_active_work() {
        let mut state = BackgroundWorkStatus::default();
        for index in 0..(MAX_BACKGROUND_TERMINAL_ENTRIES + 5) {
            state.record_task_event(
                &TaskStreamEvent {
                    task_id: format!("task-{index}"),
                    run_id: None,
                    session_id: None,
                    parent_run_id: None,
                    scope: None,
                    timestamp: index as i64,
                    kind: StreamEventKind::Completed {
                        result: "Done".to_string(),
                        duration_ms: 42,
                        stats: None,
                    },
                },
                format!("Task task-{index} completed: Done"),
            );
        }
        state.record_task_event(
            &TaskStreamEvent {
                task_id: "active-task".to_string(),
                run_id: None,
                session_id: None,
                parent_run_id: None,
                scope: None,
                timestamp: -1,
                kind: StreamEventKind::Progress {
                    phase: "running".to_string(),
                    percent: None,
                    details: None,
                },
            },
            "Task active-task progress: running".to_string(),
        );

        assert_eq!(state.entries.len(), MAX_BACKGROUND_TERMINAL_ENTRIES + 1);
        assert!(state.entries.contains_key("active-task"));
        assert!(!state.entries.contains_key("task-0"));
        assert!(!state.entries.contains_key("task-4"));
        assert!(state.entries.contains_key("task-5"));
        assert_eq!(
            state.footer_label().as_deref(),
            Some("Work 1 running · 50 done")
        );
    }

    #[test]
    fn renders_subagent_activity_group_from_tool_call() {
        let mut state = ActivityState::default();
        state.record_tool_call("call-1", "spawn_subagent", "Starting 1 subagent");
        let cells = state.live_cells();

        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].kind, TranscriptCellKind::Subagent);
        assert_eq!(cells[0].title, "Subagents");
        assert!(cells[0].body.contains("spawn"));
        assert!(cells[0].body.contains("running"));
    }
}
