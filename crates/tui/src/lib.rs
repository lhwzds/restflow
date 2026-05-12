mod activity {
    use std::collections::BTreeMap;

    use crate::transcript::{MessageGroup, TranscriptCell, TranscriptCellKind};
    use serde_json::Value;

    const MAX_ACTIVITY_ROWS: usize = 5;

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
            if self.subagents.remove(call_id).is_some() {
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

        pub fn has_subagent_activity_for(&self, call_id: &str) -> bool {
            self.subagents.contains_key(call_id)
        }

        fn bump(&mut self) {
            self.revision = self.revision.saturating_add(1);
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
            let detail = if entry.detail.trim().is_empty() {
                String::new()
            } else {
                format!(" · {}", entry.detail.trim())
            };
            lines.push(format!("- {} · {}{}", entry.title, entry.status, detail));
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

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn renders_subagent_activity_group_from_tool_call() {
            let mut state = ActivityState::default();
            state.record_tool_call("call-1", "spawn_subagent", "Starting 1 subagent");
            state.subagents.get_mut("call-1").unwrap().run_id =
                Some("run-should-not-render".to_string());
            let cells = state.live_cells();

            assert_eq!(cells.len(), 1);
            assert_eq!(cells[0].kind, TranscriptCellKind::Subagent);
            assert_eq!(cells[0].title, "Subagents");
            assert!(cells[0].body.contains("spawn"));
            assert!(cells[0].body.contains("running"));
            assert!(!cells[0].body.contains("run-should-not-render"));
            assert!(!cells[0].body.contains("run run"));
        }

        #[test]
        fn subagent_activity_disappears_after_result() {
            let mut state = ActivityState::default();
            state.record_tool_call("call-1", "wait_subagents", "Waiting for 1 subagent");
            state.record_tool_result(
                "call-1",
                false,
                "Error: Tool error: Tool wait_subagents timed out",
            );

            assert!(state.subagent_live_cells().is_empty());
        }
    }
}

mod app {
    use anyhow::Result;
    use crossterm::cursor::{Hide, Show};
    use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
    use crossterm::execute;
    use crossterm::style::Print;
    use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
    use std::io;
    use std::panic;
    use std::sync::Once;

    use crate::controller::ShellController;
    use crate::daemon_client::TuiDaemonClient;
    use crate::event_loop::run_event_loop;
    use crate::state::AppState;

    use super::TuiLaunchOptions;

    struct TerminalGuard {
        stdout: io::Stdout,
    }

    impl TerminalGuard {
        fn new() -> Result<Self> {
            enable_raw_mode()?;
            let mut stdout = io::stdout();
            execute!(stdout, EnableBracketedPaste, Hide)?;
            install_terminal_panic_hook();
            Ok(Self { stdout })
        }
    }

    impl Drop for TerminalGuard {
        fn drop(&mut self) {
            restore_terminal(&mut self.stdout);
        }
    }

    fn restore_terminal(stdout: &mut io::Stdout) {
        let _ = disable_raw_mode();
        let _ = execute!(stdout, Print("\x1b[r"), DisableBracketedPaste, Show);
    }

    fn install_terminal_panic_hook() {
        static INSTALL: Once = Once::new();
        INSTALL.call_once(|| {
            let previous = panic::take_hook();
            panic::set_hook(Box::new(move |info| {
                let mut stdout = io::stdout();
                restore_terminal(&mut stdout);
                previous(info);
            }));
        });
    }

    pub async fn run_tui(options: TuiLaunchOptions) -> Result<()> {
        let controller = ShellController::new(TuiDaemonClient::new().await?);

        let mut state = AppState::empty();
        state.set_pending_initial_message(options.message);

        let daemon_running = controller.daemon_running().await;
        let agent = controller
            .resolve_default_agent(options.agent.as_deref())
            .await?;
        if let Some(agent) = agent {
            state.set_default_agent(Some(agent.id.clone()), Some(agent.name.clone()));
            if let Some(session) = controller
                .resolve_or_create_session(&agent, options.session.as_deref())
                .await?
            {
                state.set_current_session(session);
            } else {
                state.set_pending_session(Some(controller.pending_session_for_agent(&agent).await));
            }
            state.status = if daemon_running {
                "Foreground agent ready; daemon connected.".to_string()
            } else {
                "Foreground agent ready; daemon offline.".to_string()
            };
        } else {
            state.status =
                "No default agent configured. Create one from the standard CLI.".to_string();
            state.push_info(
            "No default agent configured. Create one from the standard CLI before using the TUI.",
        );
        }

        let _terminal = TerminalGuard::new()?;
        run_event_loop(controller, state).await
    }
}

mod composer {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ComposerMode {
        Compose,
        Command,
    }

    #[derive(Debug, Clone, Default)]
    pub struct ComposerState {
        draft: String,
        cursor: usize,
        history: Vec<String>,
        history_cursor: Option<usize>,
    }

    impl ComposerState {
        pub fn draft(&self) -> &str {
            &self.draft
        }

        pub fn current_skill_mention_query(&self) -> Option<String> {
            self.current_skill_mention_range_and_query()
                .map(|(_, query)| query)
        }

        pub fn replace_current_skill_mention(&mut self, skill_id: &str) -> bool {
            let Some((start, _)) = self.current_skill_mention_range_and_query() else {
                return false;
            };
            let replacement = format!("@{skill_id} ");
            self.draft.replace_range(start..self.cursor, &replacement);
            self.cursor = start + replacement.len();
            self.history_cursor = None;
            true
        }

        pub fn visible_row_count(&self, width: u16) -> u16 {
            Self::wrapped_lines(&self.draft, width).len() as u16
        }

        pub fn visible_lines(&self, width: u16, max_rows: u16) -> Vec<String> {
            let lines = Self::wrapped_lines(&self.draft, width);
            let scroll = self.effective_scroll(width, max_rows);
            let end = (scroll + max_rows).min(lines.len() as u16) as usize;
            lines[scroll as usize..end].to_vec()
        }

        pub fn cursor_position(&self, width: u16, max_rows: u16) -> (u16, u16) {
            let width = width.max(1);
            let (column, row) = Self::wrapped_cursor_position(&self.draft[..self.cursor], width);
            let scroll = self.effective_scroll(width, max_rows);
            (column, row.saturating_sub(scroll))
        }

        pub fn mode(&self) -> ComposerMode {
            if Self::is_command_text(&self.draft) {
                ComposerMode::Command
            } else {
                ComposerMode::Compose
            }
        }

        pub fn is_blank(&self) -> bool {
            self.draft.trim().is_empty()
        }

        pub fn is_navigating_history(&self) -> bool {
            self.history_cursor.is_some()
        }

        pub fn is_command_text(text: &str) -> bool {
            text.trim_start().starts_with('/')
        }

        pub fn insert_char(&mut self, ch: char) {
            self.history_cursor = None;
            self.draft.insert(self.cursor, ch);
            self.cursor += ch.len_utf8();
        }

        pub fn insert_newline(&mut self) {
            self.insert_char('\n');
        }

        pub fn clear(&mut self) {
            self.draft.clear();
            self.cursor = 0;
            self.history_cursor = None;
        }

        pub fn replace(&mut self, value: impl Into<String>) {
            self.draft = value.into();
            self.cursor = self.draft.len();
            self.history_cursor = None;
        }

        pub fn backspace(&mut self) {
            if self.cursor == 0 {
                return;
            }
            self.history_cursor = None;
            let previous = self.draft[..self.cursor]
                .char_indices()
                .last()
                .map(|(idx, _)| idx)
                .unwrap_or(0);
            self.draft.replace_range(previous..self.cursor, "");
            self.cursor = previous;
        }

        pub fn move_left(&mut self) {
            if self.cursor == 0 {
                return;
            }
            self.cursor = self.draft[..self.cursor]
                .char_indices()
                .last()
                .map(|(idx, _)| idx)
                .unwrap_or(0);
        }

        pub fn move_right(&mut self) {
            if self.cursor >= self.draft.len() {
                return;
            }
            let next = self.draft[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(idx, _)| self.cursor + idx)
                .unwrap_or(self.draft.len());
            self.cursor = next;
        }

        pub fn move_start(&mut self) {
            self.cursor = 0;
            self.history_cursor = None;
        }

        pub fn move_end(&mut self) {
            self.cursor = self.draft.len();
            self.history_cursor = None;
        }

        pub fn take_submission(&mut self) -> String {
            self.cursor = 0;
            self.history_cursor = None;
            std::mem::take(&mut self.draft)
        }

        pub fn remember_submission(&mut self, value: &str) {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                self.history_cursor = None;
                return;
            }
            if self.history.last().is_none_or(|entry| entry != value) {
                self.history.push(value.to_string());
            }
            self.history_cursor = None;
        }

        pub fn history_previous(&mut self) {
            if self.history.is_empty() {
                return;
            }
            let next = match self.history_cursor {
                Some(index) if index > 0 => index - 1,
                Some(index) => index,
                None => self.history.len() - 1,
            };
            self.history_cursor = Some(next);
            self.draft = self.history[next].clone();
            self.cursor = self.draft.len();
        }

        pub fn history_next(&mut self) {
            let Some(index) = self.history_cursor else {
                return;
            };
            if index + 1 >= self.history.len() {
                self.history_cursor = None;
                self.draft.clear();
                self.cursor = 0;
                return;
            }
            let next = index + 1;
            self.history_cursor = Some(next);
            self.draft = self.history[next].clone();
            self.cursor = self.draft.len();
        }

        fn effective_scroll(&self, width: u16, max_rows: u16) -> u16 {
            let width = width.max(1);
            let total_rows = self.visible_row_count(width);
            if total_rows <= max_rows {
                return 0;
            }

            let (_, cursor_row) = Self::wrapped_cursor_position(&self.draft[..self.cursor], width);
            cursor_row.saturating_sub(max_rows.saturating_sub(1))
        }

        fn current_skill_mention_range_and_query(&self) -> Option<(usize, String)> {
            let prefix = &self.draft[..self.cursor];
            let start = prefix
                .char_indices()
                .rev()
                .find_map(|(idx, ch)| ch.is_whitespace().then_some(idx + ch.len_utf8()))
                .unwrap_or(0);
            let token = &prefix[start..];
            let query = token.strip_prefix('@')?;
            if query
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
            {
                return Some((start, query.to_string()));
            }
            None
        }

        fn wrapped_lines(text: &str, width: u16) -> Vec<String> {
            let width = width.max(1) as usize;
            let mut lines = Vec::new();
            let mut current = String::new();
            let mut current_width = 0usize;

            for ch in text.chars() {
                if ch == '\n' {
                    lines.push(std::mem::take(&mut current));
                    current_width = 0;
                    continue;
                }

                let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                if current_width > 0 && ch_width > 0 && current_width + ch_width > width {
                    lines.push(std::mem::take(&mut current));
                    current_width = 0;
                }

                current.push(ch);
                current_width += ch_width;
            }

            lines.push(current);
            if lines.is_empty() {
                vec![String::new()]
            } else {
                lines
            }
        }

        fn wrapped_cursor_position(text: &str, width: u16) -> (u16, u16) {
            let width = width.max(1) as usize;
            let mut row = 0u16;
            let mut column = 0usize;

            for ch in text.chars() {
                if ch == '\n' {
                    row += 1;
                    column = 0;
                    continue;
                }

                let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                if column > 0 && ch_width > 0 && column + ch_width > width {
                    row += 1;
                    column = 0;
                }

                column += ch_width;
            }

            (column as u16, row)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{ComposerMode, ComposerState};

        #[test]
        fn composer_insert_and_backspace_round_trip() {
            let mut composer = ComposerState::default();
            composer.insert_char('h');
            composer.insert_char('i');
            assert_eq!(composer.draft(), "hi");
            composer.backspace();
            assert_eq!(composer.draft(), "h");
        }

        #[test]
        fn composer_detects_command_mode() {
            let mut composer = ComposerState::default();
            composer.insert_char('/');
            composer.insert_char('h');
            composer.insert_char('e');
            composer.insert_char('l');
            composer.insert_char('p');
            assert_eq!(composer.mode(), ComposerMode::Command);
        }

        #[test]
        fn composer_detects_current_skill_mention() {
            let mut composer = ComposerState::default();
            composer.replace("please use @tea");
            assert_eq!(
                composer.current_skill_mention_query().as_deref(),
                Some("tea")
            );
        }

        #[test]
        fn composer_replaces_current_skill_mention() {
            let mut composer = ComposerState::default();
            composer.replace("please use @tea");
            assert!(composer.replace_current_skill_mention("team"));
            assert_eq!(composer.draft(), "please use @team ");
        }

        #[test]
        fn composer_history_round_trip() {
            let mut composer = ComposerState::default();
            composer.remember_submission("first");
            composer.remember_submission("second");
            composer.history_previous();
            assert_eq!(composer.draft(), "second");
            composer.history_previous();
            assert_eq!(composer.draft(), "first");
            composer.history_next();
            assert_eq!(composer.draft(), "second");
        }

        #[test]
        fn cursor_position_uses_display_width_for_wide_characters() {
            let mut composer = ComposerState::default();
            composer.insert_char('你');
            composer.insert_char('好');
            composer.insert_char('a');

            assert_eq!(composer.cursor_position(20, 6), (5, 0));
        }

        #[test]
        fn composer_moves_to_start_and_end() {
            let mut composer = ComposerState::default();
            composer.replace("hello");

            composer.move_start();
            composer.insert_char('>');
            assert_eq!(composer.draft(), ">hello");

            composer.move_end();
            composer.insert_char('<');
            assert_eq!(composer.draft(), ">hello<");
        }

        #[test]
        fn visible_row_count_accounts_for_wrapped_lines() {
            let mut composer = ComposerState::default();
            for ch in "abcdef".chars() {
                composer.insert_char(ch);
            }

            assert_eq!(composer.visible_row_count(3), 2);
        }

        #[test]
        fn visible_lines_clamp_to_max_rows() {
            let mut composer = ComposerState::default();
            for line in ["one", "two", "three", "four", "five", "six", "seven"] {
                for ch in line.chars() {
                    composer.insert_char(ch);
                }
                composer.insert_newline();
            }

            let visible = composer.visible_lines(20, 6);
            assert_eq!(visible.len(), 6);
            assert_eq!(visible[0], "three");
            assert_eq!(visible[5], "");
        }

        #[test]
        fn cursor_position_stays_visible_with_scroll() {
            let mut composer = ComposerState::default();
            for line in ["one", "two", "three", "four", "five", "six", "seven"] {
                for ch in line.chars() {
                    composer.insert_char(ch);
                }
                composer.insert_newline();
            }

            let (_column, row) = composer.cursor_position(20, 6);
            assert_eq!(row, 5);
        }
    }
}

mod controller {
    use anyhow::{Result, bail};
    use std::collections::{HashMap, HashSet};
    use tokio::sync::mpsc;

    use ::daemon::StoredAgent;
    use types::{
        ChatSession, ChatSessionSummary, ModelId, ModelMetadataDTO, RunSummary, SkillSource,
    };

    use super::daemon_client::TuiDaemonClient;
    use super::event_loop::AppEvent;
    use super::reducer::{ShellAction, ShellEffect};
    use super::slash_command::{SLASH_COMMAND_SPECS, SlashCommand};
    use super::state::{
        AppState, ModelPickerCategory, ModelPickerItem, OverlayState, PendingSessionState,
        ProviderPickerItem, SkillManagerSelection, SkillPickerItem, WorkPickerItem,
        build_work_picker_items,
    };

    #[derive(Clone)]
    pub struct ShellController {
        client: TuiDaemonClient,
    }

    impl ShellController {
        pub fn new(client: TuiDaemonClient) -> Self {
            Self { client }
        }

        pub async fn daemon_running(&self) -> bool {
            self.client.daemon_running().await
        }

        pub async fn resolve_default_agent(
            &self,
            explicit: Option<&str>,
        ) -> Result<Option<StoredAgent>> {
            self.client.resolve_default_agent(explicit).await
        }

        pub async fn resolve_or_create_session(
            &self,
            agent: &StoredAgent,
            session_override: Option<&str>,
        ) -> Result<Option<ChatSession>> {
            self.client
                .resolve_or_create_session(agent, session_override)
                .await
        }

        pub async fn pending_session_for_agent(&self, agent: &StoredAgent) -> PendingSessionState {
            let mut pending = PendingSessionState::from_agent(agent);
            let available = self
                .client
                .list_available_models()
                .await
                .unwrap_or_default();
            if let Some(item) = self
                .client
                .configured_default_model()
                .and_then(|model| resolve_model_picker_item(&available, &model))
            {
                pending.update_model(item.provider, item.model, item.name);
                return pending;
            }
            let sessions = self.client.list_sessions().await.unwrap_or_default();
            if let Some(item) = select_default_model_item(
                &sessions,
                &available,
                Some((&pending.provider, &pending.model)),
            ) {
                pending.update_model(item.provider, item.model, item.name);
            }
            pending
        }

        pub fn spawn_session_events(
            &self,
            tx: mpsc::UnboundedSender<AppEvent>,
        ) -> tokio::task::JoinHandle<()> {
            self.client.spawn_session_events(tx)
        }

        pub async fn execute_effect(
            &self,
            effect: ShellEffect,
            state: &AppState,
            tx: mpsc::UnboundedSender<AppEvent>,
        ) -> Result<Vec<ShellAction>> {
            match effect {
                ShellEffect::RefreshState => self.refresh_actions(state).await,
                ShellEffect::ReloadCurrentSession => {
                    self.reload_current_session_actions(state).await
                }
                ShellEffect::ActivateOverlaySelection => {
                    self.overlay_selection_actions(state).await
                }
                ShellEffect::CreateSessionForSubmit { message } => {
                    self.create_session_for_submit_actions(state, message).await
                }
                ShellEffect::SubmitMessage { message, stream_id } => {
                    self.submit_message_effect(state, message, stream_id, tx)
                        .await?;
                    Ok(Vec::new())
                }
                ShellEffect::SteerMessage {
                    session_id,
                    instruction,
                } => self.steer_message_actions(session_id, instruction).await,
                ShellEffect::CancelStream { stream_id } => {
                    self.cancel_stream_actions(stream_id).await
                }
                ShellEffect::ExecuteSlashCommand(command) => {
                    self.slash_command_actions(state, command).await
                }
                ShellEffect::DeleteSession { session_id } => {
                    self.delete_session_actions(session_id).await
                }
                ShellEffect::ListSkillsForMention => self.skill_mention_picker_actions().await,
                ShellEffect::ListSessionsInline => self.session_picker_actions().await,
                ShellEffect::ListRunsInline => self.list_runs_inline_actions(state).await,
                ShellEffect::ClearScreen => Ok(Vec::new()),
            }
        }

        async fn refresh_actions(&self, state: &AppState) -> Result<Vec<ShellAction>> {
            let mut sessions: Vec<ChatSessionSummary> = if should_refresh_session_list(state) {
                self.client.list_sessions().await.unwrap_or_default()
            } else {
                state.sessions.clone()
            };
            if matches!(state.overlay, Some(OverlayState::SessionPicker { .. })) {
                sessions = filter_resume_sessions(sessions, &HashSet::new());
            }
            let runs = if let Some(session_id) = state.current_session_id() {
                self.client
                    .list_runs_for_session(session_id)
                    .await
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            if refreshed_state_is_unchanged(state, &sessions, &runs) {
                return Ok(Vec::new());
            }

            let actions = vec![ShellAction::StateRefreshed { sessions, runs }];

            Ok(actions)
        }

        async fn reload_current_session_actions(
            &self,
            state: &AppState,
        ) -> Result<Vec<ShellAction>> {
            let session_id = match preferred_reload_session_id(state, None) {
                Some(session_id) => session_id,
                None if state.is_streaming || state.active_turn.is_some() => {
                    return Ok(vec![ShellAction::CurrentSessionReloaded {
                        session: None,
                        runs: state.thread.runs.clone(),
                    }]);
                }
                None if state.active_turn_has_tool_call() => {
                    return Err(anyhow::anyhow!("No active session available."));
                }
                None => match self.newest_session_id().await {
                    Some(session_id) => session_id,
                    None => return self.refresh_actions(state).await,
                },
            };

            let session = self.client.get_session(&session_id).await.ok();
            let runs = self
                .session_runs_for_reload(state, session.as_ref().map(|_| session_id.as_str()))
                .await;
            if state.is_streaming || state.active_turn.is_some() {
                return Ok(vec![ShellAction::CurrentSessionReloaded {
                    session: session.map(Box::new),
                    runs,
                }]);
            }

            let mut actions = vec![ShellAction::CurrentSessionReloaded {
                session: session.map(Box::new),
                runs,
            }];
            actions.extend(self.refresh_actions(state).await?);
            Ok(actions)
        }

        async fn newest_session_id(&self) -> Option<String> {
            self.client
                .list_sessions()
                .await
                .unwrap_or_default()
                .into_iter()
                .max_by_key(|summary| summary.updated_at)
                .map(|summary| summary.id)
        }

        async fn session_runs_for_reload(
            &self,
            state: &AppState,
            session_id: Option<&str>,
        ) -> Vec<RunSummary> {
            let Some(session_id) = session_id else {
                return Vec::new();
            };
            let Ok(runs) = self.client.list_runs_for_session(session_id).await else {
                return state.thread.runs.clone();
            };
            let _ = state;
            runs
        }

        async fn start_daemon_actions(
            &self,
            explicit_agent: Option<&str>,
            session_override: Option<&str>,
        ) -> Result<Vec<ShellAction>> {
            self.client.start_daemon().await?;
            Ok(vec![
                self.build_daemon_started_action(explicit_agent, session_override)
                    .await?,
            ])
        }

        async fn build_daemon_started_action(
            &self,
            explicit_agent: Option<&str>,
            session_override: Option<&str>,
        ) -> Result<ShellAction> {
            let agent = self.resolve_default_agent(explicit_agent).await?;
            let session = if let Some(agent) = agent.as_ref() {
                self.resolve_or_create_session(agent, session_override)
                    .await?
            } else {
                None
            };
            let pending_session = if session.is_none() {
                if let Some(agent) = agent.as_ref() {
                    Some(self.pending_session_for_agent(agent).await)
                } else {
                    None
                }
            } else {
                None
            };

            let status = if agent.is_some() {
                "Connected to daemon".to_string()
            } else {
                "No default agent configured. Create one from the standard CLI.".to_string()
            };

            Ok(ShellAction::DaemonStarted {
                agent: agent.map(Box::new),
                session: session.map(Box::new),
                pending_session,
                status,
            })
        }

        async fn overlay_selection_actions(&self, state: &AppState) -> Result<Vec<ShellAction>> {
            match state.overlay.clone() {
                Some(OverlayState::CommandPicker { .. }) => {
                    let Some(index) = state.selected_command_index() else {
                        return Ok(Vec::new());
                    };
                    let Some(spec) = SLASH_COMMAND_SPECS.get(index) else {
                        return Ok(Vec::new());
                    };
                    if matches!(spec.command, "/daemon") {
                        return Ok(vec![ShellAction::OpenDaemonPicker]);
                    }
                    if matches!(spec.command, "/new") {
                        return Ok(vec![ShellAction::SubmitText {
                            text: "/new".to_string(),
                        }]);
                    }
                    if matches!(spec.command, "/skill") {
                        return self.skill_picker_actions().await;
                    }
                    if matches!(spec.command, "/model") {
                        return self.provider_picker_actions(state).await;
                    }
                    let command = command_display(spec.command, spec.args);
                    if spec.args.is_empty() {
                        return Ok(vec![ShellAction::SubmitText { text: command }]);
                    }
                    Ok(vec![ShellAction::CommandPicked {
                        text: format!("{command} "),
                    }])
                }
                Some(OverlayState::DaemonPicker { .. }) => {
                    let Some(action) = state.selected_daemon_action() else {
                        return Ok(Vec::new());
                    };
                    Ok(vec![ShellAction::SubmitText {
                        text: format!("/daemon {action}"),
                    }])
                }
                Some(OverlayState::SessionPicker { .. }) => {
                    let Some(session_id) = state.selected_session_id().map(str::to_string) else {
                        return Ok(Vec::new());
                    };
                    let session = self.client.get_session(&session_id).await?;
                    let runs = self
                        .client
                        .list_runs_for_session(&session_id)
                        .await
                        .unwrap_or_default();
                    Ok(vec![ShellAction::SessionOpened {
                        session: Box::new(session),
                        runs,
                        status: format!("Opened session {session_id}"),
                    }])
                }
                Some(OverlayState::SkillManager { .. }) => {
                    let Some(item) = state.selected_skill_manager_item() else {
                        return Ok(Vec::new());
                    };
                    match item {
                        SkillManagerSelection::Skill(skill) => {
                            self.skill_detail_actions(skill.id).await
                        }
                    }
                }
                Some(OverlayState::ProviderPicker { .. }) => {
                    let Some(item) = state.selected_provider_item() else {
                        return Ok(Vec::new());
                    };
                    self.model_picker_actions_for_provider(state, item.provider)
                        .await
                }
                Some(OverlayState::ModelPicker { .. }) => {
                    let Some(item) = state.selected_model_item() else {
                        return Ok(Vec::new());
                    };
                    if state.current_session_id().is_none() {
                        return Ok(vec![ShellAction::PendingSessionModelSelected {
                            provider: item.provider,
                            model: item.model,
                            model_name: item.name,
                            status: "Model selected for new chat.".to_string(),
                        }]);
                    }
                    self.switch_model_actions(state, item.model).await
                }
                Some(OverlayState::RunPicker { .. }) => {
                    let Some(item) = state.selected_run_picker_item() else {
                        return Ok(Vec::new());
                    };
                    self.work_picker_selection_actions(item).await
                }
                Some(OverlayState::SkillMentionPicker { .. })
                | Some(OverlayState::SkillDetail)
                | Some(OverlayState::Help)
                | None => Ok(Vec::new()),
            }
        }

        async fn submit_message_effect(
            &self,
            state: &AppState,
            message: String,
            stream_id: String,
            tx: mpsc::UnboundedSender<AppEvent>,
        ) -> Result<()> {
            let session_id = match state.current_session_id() {
                Some(session_id) => session_id.to_string(),
                None => bail!("No active session available."),
            };
            self.client
                .spawn_chat_stream(session_id, message, stream_id, tx);
            Ok(())
        }

        async fn steer_message_actions(
            &self,
            session_id: String,
            instruction: String,
        ) -> Result<Vec<ShellAction>> {
            match self.client.steer_chat_stream(session_id, instruction).await {
                Ok(true) => Ok(vec![ShellAction::StatusUpdated(
                    "Queued update for current response.".to_string(),
                )]),
                Ok(false) => Ok(vec![ShellAction::StatusUpdated(
                    "No active response accepted the queued update.".to_string(),
                )]),
                Err(error) => Ok(vec![ShellAction::Error(format!(
                    "Failed to queue update: {error}"
                ))]),
            }
        }

        async fn cancel_stream_actions(&self, stream_id: String) -> Result<Vec<ShellAction>> {
            match self.client.cancel_chat_stream(&stream_id).await {
                Ok(true) => Ok(vec![ShellAction::StatusUpdated(
                    "Canceled current response.".to_string(),
                )]),
                Ok(false) => Ok(vec![ShellAction::StatusUpdated(
                    "No active response to cancel.".to_string(),
                )]),
                Err(error) => Ok(vec![ShellAction::Error(format!(
                    "Failed to cancel response: {error}"
                ))]),
            }
        }

        async fn create_session_for_submit_actions(
            &self,
            state: &AppState,
            message: String,
        ) -> Result<Vec<ShellAction>> {
            let agent_id = state
                .pending_session
                .as_ref()
                .map(|session| session.agent_id.as_str())
                .or(state.default_agent_id.as_deref());
            let Some(agent_id) = agent_id else {
                bail!("No default agent configured. Create one from the standard CLI.");
            };
            let session = self
                .client
                .create_session_for_agent(
                    agent_id,
                    state
                        .pending_session
                        .as_ref()
                        .map(|session| session.model.as_str()),
                )
                .await?;
            Ok(vec![ShellAction::SessionCreatedForSubmit {
                session: Box::new(session),
                runs: Vec::new(),
                message,
            }])
        }

        async fn slash_command_actions(
            &self,
            state: &AppState,
            command: SlashCommand,
        ) -> Result<Vec<ShellAction>> {
            match command {
                SlashCommand::Daemon => Ok(vec![ShellAction::OpenDaemonPicker]),
                SlashCommand::NewChat => Ok(vec![ShellAction::NewChatStarted {
                    status: "Started new chat".to_string(),
                }]),
                SlashCommand::Quit => Ok(vec![ShellAction::Quit]),
                SlashCommand::Start => {
                    match self
                        .start_daemon_actions(
                            state
                                .startup_state()
                                .and_then(|startup| startup.agent_override.as_deref()),
                            state
                                .startup_state()
                                .and_then(|startup| startup.session_override.as_deref()),
                        )
                        .await
                    {
                        Ok(actions) => Ok(actions),
                        Err(err) => Ok(start_daemon_error_actions(err)),
                    }
                }
                SlashCommand::Stop => {
                    let stopped = self.client.stop_daemon().await?;
                    let status = if stopped {
                        "RestFlow daemon stopped".to_string()
                    } else {
                        "RestFlow daemon was not running".to_string()
                    };
                    Ok(vec![ShellAction::DaemonStopped { status }])
                }
                SlashCommand::Help => Ok(vec![ShellAction::OpenHelpOverlay]),
                SlashCommand::ListSessions => self.session_picker_actions().await,
                SlashCommand::ListSkills => self.skill_picker_actions().await,
                SlashCommand::ListModels => self.provider_picker_actions(state).await,
                SlashCommand::ListModelsForProvider { provider } => {
                    match self
                        .resolve_provider_for_model_command(state, &provider)
                        .await?
                    {
                        ModelCommandTarget::Provider(provider) => {
                            self.model_picker_actions_for_provider(state, provider)
                                .await
                        }
                        ModelCommandTarget::Model(model) => {
                            self.switch_model_actions(state, model).await
                        }
                    }
                }
                SlashCommand::ListRuns => self.list_runs_inline_actions(state).await,
                SlashCommand::SwitchModel { model } => {
                    self.switch_model_actions(state, model).await
                }
                SlashCommand::SetDefaultModel { model } => {
                    self.set_default_model_actions(state, model).await
                }
            }
        }

        async fn session_picker_actions(&self) -> Result<Vec<ShellAction>> {
            let sessions = self.client.list_sessions().await?;
            let sessions = filter_resume_sessions(sessions, &HashSet::new());
            let status = if sessions.is_empty() {
                "No sessions to resume yet.".to_string()
            } else {
                "Select a session to resume".to_string()
            };
            Ok(vec![ShellAction::SessionPickerLoaded { sessions, status }])
        }

        async fn skill_picker_actions(&self) -> Result<Vec<ShellAction>> {
            let skills = self.sorted_skill_items().await?;
            let status = if skills.is_empty() {
                "No skills installed.".to_string()
            } else {
                "View skills".to_string()
            };
            Ok(vec![ShellAction::SkillPickerLoaded { skills, status }])
        }

        async fn skill_mention_picker_actions(&self) -> Result<Vec<ShellAction>> {
            let skills = self.sorted_skill_items().await?;
            let status = if skills.is_empty() {
                "No skills installed.".to_string()
            } else {
                "Select a skill mention".to_string()
            };
            Ok(vec![ShellAction::SkillMentionPickerLoaded {
                skills,
                status,
            }])
        }

        async fn sorted_skill_items(&self) -> Result<Vec<SkillPickerItem>> {
            let mut skills = self
                .client
                .list_skills()
                .await?
                .into_iter()
                .map(SkillPickerItem::from)
                .collect::<Vec<_>>();
            skills.sort_by(|left, right| {
                skill_source_order(left.source)
                    .cmp(&skill_source_order(right.source))
                    .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
                    .then_with(|| left.id.cmp(&right.id))
            });
            Ok(skills)
        }

        async fn skill_detail_actions(&self, skill_id: String) -> Result<Vec<ShellAction>> {
            match self.client.get_skill(&skill_id).await? {
                Some(skill) => Ok(vec![ShellAction::SkillDetailLoaded {
                    status: format!("Showing skill {}", skill.id),
                    skill: Box::new(skill),
                }]),
                None => Ok(vec![ShellAction::StatusUpdated(format!(
                    "Skill not found: {skill_id}"
                ))]),
            }
        }

        async fn provider_picker_actions(&self, state: &AppState) -> Result<Vec<ShellAction>> {
            let available = if state.available_models.is_empty() {
                self.client.list_available_models().await?
            } else {
                state.available_models.clone()
            };
            let sessions = if state.sessions.is_empty() {
                self.client.list_sessions().await.unwrap_or_default()
            } else {
                state.sessions.clone()
            };
            let items =
                build_provider_picker_items(&sessions, &available, state.current_model_identity());
            let status = if items.is_empty() {
                "No available providers. Configure provider credentials first.".to_string()
            } else {
                "Select a provider".to_string()
            };
            Ok(vec![ShellAction::ProviderPickerLoaded {
                items,
                available_models: available,
                sessions,
                status,
            }])
        }

        async fn model_picker_actions_for_provider(
            &self,
            state: &AppState,
            provider: String,
        ) -> Result<Vec<ShellAction>> {
            let available = if state.available_models.is_empty() {
                self.client.list_available_models().await?
            } else {
                state.available_models.clone()
            };
            let default_model = self.client.configured_default_model();
            let sessions = if state.sessions.is_empty() {
                self.client.list_sessions().await.unwrap_or_default()
            } else {
                state.sessions.clone()
            };
            let items = build_model_picker_items_for_provider(
                &sessions,
                &available,
                state.current_model_identity(),
                default_model.as_deref(),
                &provider,
            );
            let status = if items.is_empty() {
                format!("No available models for {provider}.")
            } else {
                format!("Select a {provider} model")
            };
            Ok(vec![ShellAction::ModelPickerLoaded {
                provider,
                items,
                status,
            }])
        }

        async fn switch_model_actions(
            &self,
            state: &AppState,
            model: String,
        ) -> Result<Vec<ShellAction>> {
            let Some(session_id) = state.current_session_id() else {
                let available = self
                    .client
                    .list_available_models()
                    .await
                    .unwrap_or_default();
                let Some(item) = resolve_model_picker_item(&available, &model) else {
                    return Ok(vec![ShellAction::StatusUpdated(format!(
                        "Unknown or unavailable model: {model}"
                    ))]);
                };
                return Ok(vec![ShellAction::PendingSessionModelSelected {
                    provider: item.provider,
                    model: item.model,
                    model_name: item.name,
                    status: "Model selected for new chat.".to_string(),
                }]);
            };
            let available = self
                .client
                .list_available_models()
                .await
                .unwrap_or_default();
            let Some(item) = resolve_model_picker_item(&available, &model) else {
                return Ok(vec![ShellAction::StatusUpdated(format!(
                    "Unknown or unavailable model: {model}"
                ))]);
            };
            match self
                .client
                .switch_session_model(session_id, &item.provider, &item.model)
                .await
            {
                Ok(session) => Ok(vec![ShellAction::ModelSwitched {
                    session: Box::new(session),
                    status: format!("Switched model to {}", item.model),
                }]),
                Err(error) => Ok(vec![ShellAction::StatusUpdated(format!(
                    "Failed to switch model: {error}"
                ))]),
            }
        }

        async fn set_default_model_actions(
            &self,
            state: &AppState,
            model: String,
        ) -> Result<Vec<ShellAction>> {
            let available = self
                .client
                .list_available_models()
                .await
                .unwrap_or_default();
            let Some(item) = resolve_model_picker_item(&available, &model) else {
                return Ok(vec![ShellAction::StatusUpdated(format!(
                    "Unknown or unavailable model: {model}"
                ))]);
            };
            self.client.set_default_model(&item.model).await?;
            let status = format!("Default model set to {}", item.model);
            if state.current_session_id().is_none() && state.pending_session.is_some() {
                return Ok(vec![ShellAction::PendingSessionModelSelected {
                    provider: item.provider,
                    model: item.model,
                    model_name: item.name,
                    status,
                }]);
            }
            Ok(vec![ShellAction::StatusUpdated(status)])
        }

        async fn resolve_provider_for_model_command(
            &self,
            state: &AppState,
            value: &str,
        ) -> Result<ModelCommandTarget> {
            let available = self.client.list_available_models().await?;
            if available
                .iter()
                .any(|metadata| metadata.provider.as_canonical_str() == value)
            {
                return Ok(ModelCommandTarget::Provider(value.to_string()));
            }
            let Some(item) = resolve_model_picker_item(&available, value) else {
                return Ok(ModelCommandTarget::Provider(value.to_string()));
            };
            if state.current_session_id().is_none() {
                return Ok(ModelCommandTarget::Model(item.model));
            }
            Ok(ModelCommandTarget::Model(item.model))
        }

        async fn delete_session_actions(&self, session_id: String) -> Result<Vec<ShellAction>> {
            let delete_result = self.client.delete_session(&session_id).await;
            let sessions = self.client.list_sessions().await.unwrap_or_default();
            let sessions = filter_resume_sessions(sessions, &HashSet::new());
            let (deleted, status) = match delete_result {
                Ok(deleted) if deleted => (true, format!("Deleted session {session_id}")),
                Ok(_) => (false, format!("Session {session_id} was not deleted")),
                Err(error) => (
                    false,
                    delete_session_error_message(&session_id, error.to_string()),
                ),
            };
            Ok(vec![ShellAction::SessionDeleted {
                session_id,
                deleted,
                sessions,
                status,
            }])
        }

        async fn list_runs_inline_actions(&self, state: &AppState) -> Result<Vec<ShellAction>> {
            let runs = if let Some(session_id) = state.current_session_id() {
                self.client
                    .list_runs_for_session(session_id)
                    .await
                    .unwrap_or_else(|_| state.thread.runs.clone())
            } else {
                state.thread.runs.clone()
            };
            let items = build_work_picker_items(&runs);
            let status = if items.is_empty() {
                "No active runs.".to_string()
            } else {
                "Work picker opened.".to_string()
            };

            Ok(vec![ShellAction::RunPickerLoaded { runs, status }])
        }

        async fn work_picker_selection_actions(
            &self,
            item: WorkPickerItem,
        ) -> Result<Vec<ShellAction>> {
            let WorkPickerItem::Run { .. } = item;
            Ok(vec![ShellAction::StatusUpdated(
                "Run detail view was removed.".to_string(),
            )])
        }
    }

    fn refreshed_state_is_unchanged(
        state: &AppState,
        sessions: &[ChatSessionSummary],
        runs: &[types::RunSummary],
    ) -> bool {
        if state.sessions != sessions {
            return false;
        }
        if state.current_session_id().is_some() {
            state.thread.runs == runs
        } else {
            runs.is_empty() && state.thread.runs.is_empty()
        }
    }

    fn should_refresh_session_list(state: &AppState) -> bool {
        matches!(state.overlay, Some(OverlayState::SessionPicker { .. }))
    }

    fn filter_resume_sessions(
        sessions: Vec<ChatSessionSummary>,
        bound_session_ids: &std::collections::HashSet<String>,
    ) -> Vec<ChatSessionSummary> {
        sessions
            .into_iter()
            .filter(|session| session.message_count > 0 && !bound_session_ids.contains(&session.id))
            .collect()
    }

    fn preferred_reload_session_id(
        state: &AppState,
        newest_session_id: Option<String>,
    ) -> Option<String> {
        state
            .active_refresh_session_id()
            .map(ToOwned::to_owned)
            .or_else(|| state.current_session_id().map(ToOwned::to_owned))
            .or(newest_session_id)
    }

    #[derive(Debug, Clone, Default)]
    struct ModelUsage {
        count: usize,
        last_used_at: Option<i64>,
    }

    enum ModelCommandTarget {
        Provider(String),
        Model(String),
    }

    fn model_key(provider: &str, model: &str) -> String {
        format!("{}:{}", provider.trim(), model.trim())
    }

    fn model_usage_by_key(sessions: &[ChatSessionSummary]) -> HashMap<String, ModelUsage> {
        let mut usage = HashMap::<String, ModelUsage>::new();
        for session in sessions {
            if session.provider.trim().is_empty() || session.model.trim().is_empty() {
                continue;
            }
            let entry = usage
                .entry(model_key(&session.provider, &session.model))
                .or_default();
            entry.count += 1;
            entry.last_used_at = Some(
                entry
                    .last_used_at
                    .map(|existing| existing.max(session.updated_at))
                    .unwrap_or(session.updated_at),
            );
        }
        usage
    }

    fn provider_usage_by_key(sessions: &[ChatSessionSummary]) -> HashMap<String, ModelUsage> {
        let mut usage = HashMap::<String, ModelUsage>::new();
        for session in sessions {
            if session.provider.trim().is_empty() {
                continue;
            }
            let entry = usage.entry(session.provider.clone()).or_default();
            entry.count += 1;
            entry.last_used_at = Some(
                entry
                    .last_used_at
                    .map(|existing| existing.max(session.updated_at))
                    .unwrap_or(session.updated_at),
            );
        }
        usage
    }

    fn recent_usage_keys(usage: &HashMap<String, ModelUsage>, limit: usize) -> HashSet<String> {
        let mut entries = usage
            .iter()
            .filter_map(|(key, usage)| usage.last_used_at.map(|last| (key.clone(), last)))
            .collect::<Vec<_>>();
        entries.sort_by_key(|(_, last)| std::cmp::Reverse(*last));
        entries
            .into_iter()
            .take(limit)
            .map(|(key, _)| key)
            .collect()
    }

    fn category_for_usage(
        key: &str,
        usage: Option<&ModelUsage>,
        recent_keys: &HashSet<String>,
    ) -> ModelPickerCategory {
        if recent_keys.contains(key) {
            ModelPickerCategory::Recent
        } else if usage.is_some_and(|usage| usage.count > 0) {
            ModelPickerCategory::Frequent
        } else {
            ModelPickerCategory::Available
        }
    }

    fn picker_catalog_metadata_by_key() -> HashMap<String, ModelMetadataDTO> {
        ModelId::all_with_metadata()
            .into_iter()
            .filter(|metadata| !metadata.model.is_opencode_cli() && !metadata.model.is_gemini_cli())
            .map(|metadata| {
                (
                    model_key(
                        metadata.provider.as_canonical_str(),
                        metadata.model.as_serialized_str(),
                    ),
                    metadata,
                )
            })
            .collect()
    }

    fn build_provider_picker_items(
        sessions: &[ChatSessionSummary],
        available: &[ModelMetadataDTO],
        current_model: Option<(&str, &str)>,
    ) -> Vec<ProviderPickerItem> {
        let usage = provider_usage_by_key(sessions);
        let recent_keys = recent_usage_keys(&usage, 5);
        let current_provider = current_model.map(|(provider, _)| provider.to_string());
        let mut items_by_provider = HashMap::<String, ProviderPickerItem>::new();

        for (provider, usage) in &usage {
            let category = category_for_usage(provider, Some(usage), &recent_keys);
            items_by_provider.insert(
                provider.clone(),
                ProviderPickerItem {
                    label: provider.clone(),
                    is_current: current_provider.as_deref() == Some(provider.as_str()),
                    provider: provider.clone(),
                    category,
                    usage_count: usage.count,
                    last_used_at: usage.last_used_at,
                },
            );
        }

        for metadata in available {
            let provider = metadata.provider.as_canonical_str().to_string();
            let usage = usage.get(&provider).cloned().unwrap_or_default();
            let category = category_for_usage(&provider, Some(&usage), &recent_keys);
            items_by_provider
                .entry(provider.clone())
                .and_modify(|item| {
                    item.is_current = current_provider.as_deref() == Some(provider.as_str());
                })
                .or_insert_with(|| ProviderPickerItem {
                    label: provider.clone(),
                    is_current: current_provider.as_deref() == Some(provider.as_str()),
                    provider,
                    category,
                    usage_count: usage.count,
                    last_used_at: usage.last_used_at,
                });
        }

        if let Some(provider) = current_provider
            && !provider.trim().is_empty()
        {
            items_by_provider
                .entry(provider.clone())
                .or_insert_with(|| ProviderPickerItem {
                    label: provider.clone(),
                    is_current: true,
                    provider,
                    category: ModelPickerCategory::Recent,
                    usage_count: 0,
                    last_used_at: None,
                });
        }

        let mut items = items_by_provider.into_values().collect::<Vec<_>>();
        items.sort_by(|left, right| {
            model_category_order(left.category)
                .cmp(&model_category_order(right.category))
                .then_with(|| match left.category {
                    ModelPickerCategory::Recent => right.last_used_at.cmp(&left.last_used_at),
                    ModelPickerCategory::Frequent => right.usage_count.cmp(&left.usage_count),
                    ModelPickerCategory::Available => std::cmp::Ordering::Equal,
                })
                .then_with(|| left.label.cmp(&right.label))
        });
        items
    }

    fn build_model_picker_items_for_provider(
        sessions: &[ChatSessionSummary],
        available: &[ModelMetadataDTO],
        current_model: Option<(&str, &str)>,
        default_model: Option<&str>,
        provider_filter: &str,
    ) -> Vec<ModelPickerItem> {
        let usage = model_usage_by_key(sessions);
        let recent_keys = recent_usage_keys(&usage, 5);
        let catalog = picker_catalog_metadata_by_key();

        let current_key = current_model
            .map(|(provider, model)| model_key(provider, model))
            .filter(|key| key != ":");
        let default_key = default_model.and_then(|model| {
            let item = resolve_model_picker_item(available, model)?;
            Some(model_key(&item.provider, &item.model))
        });

        let mut items_by_key = HashMap::<String, ModelPickerItem>::new();
        let mut available_keys = HashSet::<String>::new();

        for metadata in available
            .iter()
            .filter(|metadata| metadata.provider.as_canonical_str() == provider_filter)
        {
            let provider = metadata.provider.as_canonical_str().to_string();
            let model = metadata.model.as_serialized_str().to_string();
            let key = model_key(&provider, &model);
            available_keys.insert(key.clone());
            let usage = usage.get(&key).cloned().unwrap_or_default();
            let category = category_for_usage(&key, Some(&usage), &recent_keys);
            items_by_key
                .entry(key.clone())
                .and_modify(|item| {
                    item.name = metadata.name.clone();
                    item.is_current = current_key.as_deref() == Some(key.as_str());
                    item.is_default = default_key.as_deref() == Some(key.as_str());
                })
                .or_insert_with(|| ModelPickerItem {
                    provider,
                    model,
                    name: metadata.name.clone(),
                    category,
                    usage_count: usage.count,
                    last_used_at: usage.last_used_at,
                    is_current: current_key.as_deref() == Some(key.as_str()),
                    is_default: default_key.as_deref() == Some(key.as_str()),
                });
        }

        if let Some(key) = current_key.as_ref()
            && available_keys.contains(key)
            && let Some(metadata) = catalog.get(key)
            && metadata.provider.as_canonical_str() == provider_filter
        {
            items_by_key
                .entry(key.clone())
                .or_insert_with(|| ModelPickerItem {
                    provider: metadata.provider.as_canonical_str().to_string(),
                    model: metadata.model.as_serialized_str().to_string(),
                    name: metadata.name.clone(),
                    category: ModelPickerCategory::Recent,
                    usage_count: 0,
                    last_used_at: None,
                    is_current: true,
                    is_default: default_key.as_deref() == Some(key.as_str()),
                });
        }

        let mut items = items_by_key.into_values().collect::<Vec<_>>();
        items.sort_by(|left, right| {
            model_category_order(left.category)
                .cmp(&model_category_order(right.category))
                .then_with(|| match left.category {
                    ModelPickerCategory::Recent => right.last_used_at.cmp(&left.last_used_at),
                    ModelPickerCategory::Frequent => right.usage_count.cmp(&left.usage_count),
                    ModelPickerCategory::Available => std::cmp::Ordering::Equal,
                })
                .then_with(|| left.provider.cmp(&right.provider))
                .then_with(|| {
                    model_picker_sort_rank(&left.model).cmp(&model_picker_sort_rank(&right.model))
                })
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.model.cmp(&right.model))
        });
        items
    }

    fn model_picker_sort_rank(model: &str) -> usize {
        let gpt_5_4_codex = ModelId::Gpt5_4Codex.as_serialized_str();
        let gpt_5_4_mini_codex = ModelId::Gpt5_4MiniCodex.as_serialized_str();
        let codex_cli = ModelId::CodexCli.as_serialized_str();
        let gpt_5_codex = ModelId::Gpt5Codex.as_serialized_str();
        let gpt_5_1_codex = ModelId::Gpt5_1Codex.as_serialized_str();
        let gpt_5_2_codex = ModelId::Gpt5_2Codex.as_serialized_str();

        match ModelId::normalize_model_id(model) {
            Some(model) if model == gpt_5_4_codex => 0,
            Some(model) if model == gpt_5_4_mini_codex => 1,
            Some(model) if model == codex_cli => 2,
            Some(model)
                if model == gpt_5_codex || model == gpt_5_1_codex || model == gpt_5_2_codex =>
            {
                20
            }
            _ => 10,
        }
    }

    fn select_default_model_item(
        sessions: &[ChatSessionSummary],
        available: &[ModelMetadataDTO],
        current_model: Option<(&str, &str)>,
    ) -> Option<ModelPickerItem> {
        let providers = build_provider_picker_items(sessions, available, current_model);
        providers.into_iter().find_map(|provider| {
            build_model_picker_items_for_provider(
                sessions,
                available,
                current_model,
                None,
                &provider.provider,
            )
            .into_iter()
            .next()
        })
    }

    fn resolve_model_picker_item(
        available: &[ModelMetadataDTO],
        requested: &str,
    ) -> Option<ModelPickerItem> {
        let requested = requested.trim();
        available.iter().find_map(|metadata| {
            let provider = metadata.provider.as_canonical_str().to_string();
            let model = metadata.model.as_serialized_str().to_string();
            let qualified = model_key(&provider, &model);
            if requested == model || requested == qualified {
                Some(ModelPickerItem {
                    provider,
                    model,
                    name: metadata.name.clone(),
                    category: ModelPickerCategory::Available,
                    usage_count: 0,
                    last_used_at: None,
                    is_current: false,
                    is_default: false,
                })
            } else {
                None
            }
        })
    }

    fn model_category_order(category: ModelPickerCategory) -> u8 {
        match category {
            ModelPickerCategory::Recent => 0,
            ModelPickerCategory::Frequent => 1,
            ModelPickerCategory::Available => 2,
        }
    }

    fn skill_source_order(source: SkillSource) -> u8 {
        match source {
            SkillSource::System => 0,
            SkillSource::User => 1,
            SkillSource::External => 2,
        }
    }

    fn delete_session_error_message(session_id: &str, error: String) -> String {
        format!("Failed to delete session {session_id}: {error}")
    }

    fn start_daemon_error_actions(err: anyhow::Error) -> Vec<ShellAction> {
        vec![ShellAction::Error(format!("Failed to start daemon: {err}"))]
    }

    fn command_display(command: &str, args: &str) -> String {
        if args.is_empty() {
            command.to_string()
        } else {
            format!("{command} {args}")
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{
            build_model_picker_items_for_provider, build_provider_picker_items,
            filter_resume_sessions, preferred_reload_session_id, refreshed_state_is_unchanged,
            select_default_model_item, should_refresh_session_list, start_daemon_error_actions,
        };
        use crate::reducer::ShellAction;
        use crate::state::{AppState, ModelPickerCategory, OverlayState};
        use std::collections::HashSet;
        use types::{ChatSession, ChatSessionSummary, ModelId, ModelMetadataDTO};

        #[test]
        fn start_daemon_error_stays_inside_shell() {
            let actions = start_daemon_error_actions(anyhow::anyhow!("socket denied"));

            assert!(matches!(
                actions.as_slice(),
                [ShellAction::Error(message)]
                    if message.contains("Failed to start daemon") && message.contains("socket denied")
            ));
        }

        #[test]
        fn unchanged_refresh_state_can_skip_render_action() {
            let mut state = AppState::empty();
            state.sessions = vec![session_summary_with_messages("session-1", "Chat", 2)];

            assert!(refreshed_state_is_unchanged(&state, &state.sessions, &[],));
        }

        #[test]
        fn changed_refresh_state_requires_render_action() {
            let mut state = AppState::empty();
            state.sessions = vec![session_summary_with_messages("session-1", "Chat", 2)];
            let refreshed = vec![session_summary_with_messages("session-1", "Chat", 3)];

            assert!(!refreshed_state_is_unchanged(&state, &refreshed, &[],));
        }

        #[test]
        fn filter_resume_sessions_removes_bound_and_empty_sessions() {
            let sessions = vec![
                session_summary_with_messages("session-1", "Regular", 1),
                session_summary_with_messages("session-2", "Bound", 1),
                session_summary_with_messages("session-3", "Empty", 0),
                session_summary_with_messages("session-4", "Regular 2", 1),
            ];
            let bound = HashSet::from(["session-2".to_string()]);

            let visible = filter_resume_sessions(sessions, &bound);

            assert_eq!(visible.len(), 2);
            assert_eq!(visible[0].id, "session-1");
            assert_eq!(visible[1].id, "session-4");
        }

        #[test]
        fn preferred_reload_session_keeps_active_anchor_over_newest_session() {
            let mut state = AppState::empty();
            let session = ChatSession::new("agent-1".to_string(), "model".to_string());
            let session_id = session.id.clone();
            state.set_current_session(session);
            state.push_local_user_message("run a tool".to_string());
            state.apply_stream_frame(types::StreamFrame::ToolCall {
                id: "call-1".to_string(),
                name: "edit".to_string(),
                arguments: serde_json::json!({"file_path":"check.txt"}),
            });

            assert_eq!(
                preferred_reload_session_id(&state, Some("newer-session".to_string())),
                Some(session_id)
            );
        }

        #[test]
        fn preferred_reload_session_uses_current_session_before_listing_newest() {
            let mut state = AppState::empty();
            let session = ChatSession::new("agent-1".to_string(), "model".to_string());
            let session_id = session.id.clone();
            state.set_current_session(session);

            assert_eq!(
                preferred_reload_session_id(&state, Some("newer-session".to_string())),
                Some(session_id)
            );
        }

        #[test]
        fn active_turn_refresh_keeps_hot_path_off_global_session_list() {
            let mut state = AppState::empty();
            state.set_current_session(ChatSession::new("agent-1".to_string(), "model".to_string()));
            state.push_local_user_message("hello".to_string());
            state.begin_stream("turn-1".to_string());

            assert!(!should_refresh_session_list(&state));
        }

        #[test]
        fn idle_refresh_keeps_global_session_list_off_hot_path() {
            let state = AppState::empty();

            assert!(!should_refresh_session_list(&state));
        }

        #[test]
        fn session_picker_refresh_updates_global_session_list() {
            let mut state = AppState::empty();
            state.overlay = Some(OverlayState::SessionPicker { selected: 0 });

            assert!(should_refresh_session_list(&state));
        }

        #[test]
        fn provider_picker_orders_recent_frequent_then_available_providers() {
            let sessions = vec![
                session_summary_with_model("session-1", "Recent", "codex", "gpt-5.4", 100),
                session_summary_with_model(
                    "session-2",
                    "Frequent 1",
                    "minimax-coding-plan",
                    "minimax-coding-plan-m2-5",
                    50,
                ),
                session_summary_with_model(
                    "session-3",
                    "Frequent 2",
                    "minimax-coding-plan",
                    "minimax-coding-plan-m2-5",
                    60,
                ),
            ];
            let available = vec![
                model_metadata(ModelId::MiniMaxM25CodingPlan, "MiniMax M2.5"),
                model_metadata(ModelId::Gpt5_4Codex, "GPT-5.4"),
                model_metadata(ModelId::Gpt5_4MiniCodex, "GPT-5.4 Mini"),
            ];
            let items =
                build_provider_picker_items(&sessions, &available, Some(("codex", "gpt-5.4")));

            assert_eq!(items[0].provider, "codex");
            assert_eq!(items[0].category, ModelPickerCategory::Recent);
            assert!(items[0].is_current);
            assert_eq!(items[1].provider, "minimax-coding-plan");
            assert_eq!(items[1].usage_count, 2);
        }

        #[test]
        fn provider_picker_includes_used_providers_without_current_api_key() {
            let sessions = vec![
                session_summary_with_model("session-1", "Codex", "codex", "gpt-5.4", 100),
                session_summary_with_model(
                    "session-2",
                    "Zai",
                    "zai-coding-plan",
                    "zai-coding-plan-glm-5-1",
                    90,
                ),
            ];
            let available = vec![model_metadata(ModelId::Gpt5, "GPT-5")];
            let items =
                build_provider_picker_items(&sessions, &available, Some(("codex", "gpt-5.4")));
            let providers = items
                .iter()
                .map(|item| item.provider.as_str())
                .collect::<Vec<_>>();

            assert!(providers.contains(&"codex"));
            assert!(providers.contains(&"zai-coding-plan"));
            assert!(providers.contains(&"openai"));
            assert!(
                items
                    .iter()
                    .any(|item| item.provider == "codex" && item.is_current)
            );
        }

        #[test]
        fn model_picker_filters_models_to_selected_provider() {
            let sessions = vec![
                session_summary_with_model("session-1", "Recent", "codex", "gpt-5.4", 100),
                session_summary_with_model(
                    "session-2",
                    "Frequent 1",
                    "minimax-coding-plan",
                    "minimax-coding-plan-m2-5",
                    50,
                ),
                session_summary_with_model(
                    "session-3",
                    "Frequent 2",
                    "minimax-coding-plan",
                    "minimax-coding-plan-m2-5",
                    60,
                ),
            ];
            let available = vec![
                model_metadata(ModelId::MiniMaxM25CodingPlan, "MiniMax M2.5"),
                model_metadata(ModelId::Gpt5_4Codex, "GPT-5.4"),
                model_metadata(ModelId::Gpt5_4MiniCodex, "GPT-5.4 Mini"),
            ];
            let items = build_model_picker_items_for_provider(
                &sessions,
                &available,
                Some(("codex", "gpt-5.4")),
                None,
                "codex",
            );

            assert_eq!(items[0].model, "gpt-5.4");
            assert_eq!(items[0].category, ModelPickerCategory::Recent);
            assert!(items[0].is_current);
            assert_eq!(items[1].model, "gpt-5.4-mini");
            assert_eq!(items[1].category, ModelPickerCategory::Available);
        }

        #[test]
        fn model_picker_marks_configured_default_model() {
            let sessions = vec![session_summary_with_model(
                "session-1",
                "Recent",
                "zai-coding-plan",
                "zai-coding-plan-glm-5-1",
                100,
            )];
            let available = vec![model_metadata(ModelId::Glm5_1CodingPlan, "GLM-5.1")];
            let items = build_model_picker_items_for_provider(
                &sessions,
                &available,
                Some(("zai-coding-plan", "zai-coding-plan-glm-5-1")),
                Some("zai-coding-plan-glm-5-1"),
                "zai-coding-plan",
            );

            assert!(items[0].is_current);
            assert!(items[0].is_default);
        }

        #[test]
        fn model_picker_includes_used_models_without_current_api_key() {
            let sessions = vec![session_summary_with_model(
                "session-1",
                "Codex",
                "codex",
                "gpt-5.4",
                100,
            )];
            let available = vec![model_metadata(ModelId::Gpt5_4Codex, "GPT-5.4")];
            let items =
                build_model_picker_items_for_provider(&sessions, &available, None, None, "codex");

            assert_eq!(items[0].provider, "codex");
            assert_eq!(items[0].model, "gpt-5.4");
            assert_eq!(items[0].name, "GPT-5.4");
            assert_eq!(items[0].category, ModelPickerCategory::Recent);
        }

        #[test]
        fn model_picker_excludes_used_models_without_available_metadata() {
            let sessions = vec![session_summary_with_model(
                "session-1",
                "Old OpenAI",
                "openai",
                "gpt-5-2",
                100,
            )];
            let available = vec![model_metadata(ModelId::Gpt5_4, "GPT-5.4")];
            let items =
                build_model_picker_items_for_provider(&sessions, &available, None, None, "openai");

            assert_eq!(items.len(), 1);
            assert_eq!(items[0].model, "gpt-5-4");
        }

        #[test]
        fn default_model_selection_skips_unavailable_current_model() {
            let sessions = vec![session_summary_with_model(
                "session-1",
                "DeepSeek",
                "deepseek",
                "deepseek-chat",
                100,
            )];
            let available = vec![model_metadata(ModelId::DeepseekChat, "DeepSeek Chat")];

            let item =
                select_default_model_item(&sessions, &available, Some(("openai", "gpt-5.4")))
                    .expect("available fallback model");

            assert_eq!(item.provider, "deepseek");
            assert_eq!(item.model, "deepseek-chat");
        }

        #[test]
        fn default_model_selection_prefers_supported_codex_default() {
            let available = vec![
                model_metadata(ModelId::Gpt5Codex, "GPT-5 Codex"),
                model_metadata(ModelId::Gpt5_1Codex, "GPT-5.1 Codex"),
                model_metadata(ModelId::Gpt5_4MiniCodex, "GPT-5.4 Mini"),
                model_metadata(ModelId::Gpt5_4Codex, "GPT-5.4"),
                model_metadata(ModelId::CodexCli, "GPT-5.3 Codex"),
            ];

            let item = select_default_model_item(&[], &available, Some(("codex", "gpt-5-codex")))
                .expect("codex default model");

            assert_eq!(item.provider, "codex");
            assert_eq!(item.model, "gpt-5.4");
        }

        fn session_summary_with_messages(
            id: &str,
            name: &str,
            message_count: u32,
        ) -> ChatSessionSummary {
            ChatSessionSummary {
                id: id.to_string(),
                name: name.to_string(),
                agent_id: "agent-1".to_string(),
                provider: "provider".to_string(),
                model: "model".to_string(),
                skill_id: None,
                message_count,
                updated_at: 1,
                last_message_preview: Some("preview".to_string()),
                archived_at: None,
            }
        }

        fn session_summary_with_model(
            id: &str,
            name: &str,
            provider: &str,
            model: &str,
            updated_at: i64,
        ) -> ChatSessionSummary {
            ChatSessionSummary {
                id: id.to_string(),
                name: name.to_string(),
                agent_id: "agent-1".to_string(),
                provider: provider.to_string(),
                model: model.to_string(),
                skill_id: None,
                message_count: 1,
                updated_at,
                last_message_preview: Some("preview".to_string()),
                archived_at: None,
            }
        }

        fn model_metadata(model: ModelId, name: &str) -> ModelMetadataDTO {
            ModelMetadataDTO {
                model,
                provider: model.provider(),
                supports_temperature: false,
                name: name.to_string(),
            }
        }
    }
}

mod daemon_client {
    use ::daemon::AppCore;
    use ::daemon::daemon::{
        DaemonConfig, IpcClient, is_daemon_available, start_daemon_with_config, stop_daemon,
    };
    use ::daemon::paths;
    use ::daemon::services::{session::SessionService, skills as skills_service};
    use ::daemon::storage::{load_cli_config, load_global_cli_config, write_cli_config};
    use ::daemon::{DEFAULT_ASSISTANT_NAME, StoredAgent};
    use anyhow::{Result, bail};
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use tokio::time::{Duration, sleep};
    use types::request::WireModelRef;
    use types::{
        ChatSession, ChatSessionSummary, ExecutionContainerKind, ExecutionContainerRef, ModelId,
        ModelMetadataDTO, Provider, RunListQuery, RunSummary, Skill,
    };
    use types::{ChatSessionEvent, StreamFrame};

    use super::event_loop::AppEvent;

    #[derive(Clone)]
    pub struct TuiDaemonClient {
        socket_path: PathBuf,
        core: Arc<AppCore>,
    }

    impl TuiDaemonClient {
        pub async fn new() -> Result<Self> {
            let db_path = paths::ensure_database_path_string()?;
            let core = Arc::new(AppCore::new(&db_path).await?);
            Ok(Self {
                socket_path: paths::socket_path()?,
                core,
            })
        }

        pub async fn daemon_running(&self) -> bool {
            is_daemon_available(&self.socket_path).await
        }

        pub async fn start_daemon(&self) -> Result<()> {
            if self.daemon_running().await {
                return Ok(());
            }

            let report = ::daemon::daemon::recovery::recover().await?;
            let _ = report;
            tokio::task::spawn_blocking(|| start_daemon_with_config(DaemonConfig)).await??;

            for _ in 0..100 {
                if self.daemon_running().await {
                    return Ok(());
                }
                sleep(Duration::from_millis(100)).await;
            }

            bail!("RestFlow daemon did not become ready in time.")
        }

        pub async fn stop_daemon(&self) -> Result<bool> {
            tokio::task::spawn_blocking(stop_daemon).await?
        }

        async fn connect(&self) -> Result<IpcClient> {
            IpcClient::connect(&self.socket_path).await
        }

        pub async fn list_agents(&self) -> Result<Vec<StoredAgent>> {
            self.core.storage.agents.list_agents()
        }

        pub async fn get_agent(&self, id: &str) -> Result<StoredAgent> {
            self.core
                .storage
                .agents
                .get_agent(id.to_string())?
                .ok_or_else(|| anyhow::anyhow!("Agent not found: {id}"))
        }

        pub async fn resolve_default_agent(
            &self,
            explicit: Option<&str>,
        ) -> Result<Option<StoredAgent>> {
            if let Some(id) = explicit {
                return self.get_agent(id).await.map(Some);
            }

            let agents = self.list_agents().await?;
            if agents.is_empty() {
                return Ok(None);
            }

            if let Some(agent) = agents
                .iter()
                .find(|agent| agent.name.eq_ignore_ascii_case(DEFAULT_ASSISTANT_NAME))
                .cloned()
            {
                return Ok(Some(agent));
            }

            if agents.len() == 1 {
                return Ok(agents.into_iter().next());
            }

            bail!(
                "Default agent is ambiguous. Configure '{}' or pass --agent.",
                DEFAULT_ASSISTANT_NAME
            )
        }

        pub async fn resolve_or_create_session(
            &self,
            _agent: &StoredAgent,
            session_override: Option<&str>,
        ) -> Result<Option<ChatSession>> {
            match session_override {
                Some(session_id) => {
                    SessionService::from_storage(&self.core.storage).get_session_view(session_id)
                }
                None => Ok(None),
            }
        }

        pub async fn create_session_for_agent(
            &self,
            agent_id: &str,
            model: Option<&str>,
        ) -> Result<ChatSession> {
            let agent_id = self
                .core
                .storage
                .agents
                .resolve_existing_agent_id(agent_id)?;
            let model = match model {
                Some(model) => normalize_model_input(model)?,
                None if let Some(model) = self.configured_default_model() => {
                    normalize_model_input(&model)?
                }
                None => self
                    .core
                    .storage
                    .agents
                    .get_agent(agent_id.clone())?
                    .and_then(|agent| agent.agent.resolved_model_ref())
                    .map(|model_ref| model_ref.model.as_serialized_str().to_string())
                    .unwrap_or_else(|| ModelId::Gpt5_4.as_serialized_str().to_string()),
            };
            SessionService::from_storage(&self.core.storage)
                .create_workspace_session(agent_id, model, None, None, None)
        }

        pub fn configured_default_model(&self) -> Option<String> {
            load_cli_config().ok().and_then(|config| config.model)
        }

        pub async fn set_default_model(&self, model: &str) -> Result<()> {
            let mut config = load_global_cli_config()?;
            config.model = Some(model.to_string());
            write_cli_config(&config)
        }

        pub async fn list_sessions(&self) -> Result<Vec<ChatSessionSummary>> {
            SessionService::from_storage(&self.core.storage)
                .list_session_summaries(None, None, false)
        }

        pub async fn list_available_models(&self) -> Result<Vec<ModelMetadataDTO>> {
            Ok(available_model_catalog(&self.core))
        }

        pub async fn list_skills(&self) -> Result<Vec<Skill>> {
            skills_service::list_skills(&self.core).await
        }

        pub async fn get_skill(&self, skill_id: &str) -> Result<Option<Skill>> {
            skills_service::get_skill(&self.core, skill_id).await
        }

        pub async fn get_session(&self, session_id: &str) -> Result<ChatSession> {
            SessionService::from_storage(&self.core.storage)
                .get_session_view(session_id)?
                .ok_or_else(|| anyhow::anyhow!(types::session_not_found_message(session_id)))
        }

        pub async fn switch_session_model(
            &self,
            session_id: &str,
            provider: &str,
            model: &str,
        ) -> Result<ChatSession> {
            let _model_ref = WireModelRef {
                provider: provider.to_string(),
                model: model.to_string(),
            };
            let model = normalize_model_input(model)?;
            let session_service = SessionService::from_storage(&self.core.storage);
            let mut session = session_service
                .get_session_view(session_id)?
                .ok_or_else(|| anyhow::anyhow!(types::session_not_found_message(session_id)))?;
            session.set_model_identity_from_raw(&model);
            session_service.save_session_metadata(&session)?;
            Ok(session)
        }

        pub async fn delete_session(&self, session_id: &str) -> Result<bool> {
            SessionService::from_storage(&self.core.storage).delete_session(session_id)
        }

        pub async fn cancel_chat_stream(&self, stream_id: &str) -> Result<bool> {
            Ok(::daemon::daemon::cancel_foreground_chat_stream(&self.core, stream_id).await)
        }

        pub async fn list_runs_for_session(&self, session_id: &str) -> Result<Vec<RunSummary>> {
            let mut client = self.connect().await?;
            client
                .list_runs(RunListQuery {
                    container: ExecutionContainerRef {
                        kind: ExecutionContainerKind::Workspace,
                        id: session_id.to_string(),
                    },
                })
                .await
        }

        pub fn spawn_session_events(
            &self,
            tx: mpsc::UnboundedSender<AppEvent>,
        ) -> tokio::task::JoinHandle<()> {
            let client = self.clone();
            tokio::spawn(async move {
                if !client.daemon_running().await {
                    return;
                }
                let mut ipc = match client.connect().await {
                    Ok(ipc) => ipc,
                    Err(error) => {
                        let _ = tx.send(AppEvent::Error(error.to_string()));
                        return;
                    }
                };

                let result = ipc
                    .subscribe_session_events(|event: ChatSessionEvent| {
                        tx.send(AppEvent::SessionEvent(event))
                            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                        Ok(())
                    })
                    .await;

                if let Err(error) = result {
                    let _ = tx.send(AppEvent::Error(format!("Session stream stopped: {error}")));
                }
            })
        }

        pub fn spawn_chat_stream(
            &self,
            session_id: String,
            input: String,
            stream_id: String,
            tx: mpsc::UnboundedSender<AppEvent>,
        ) -> tokio::task::JoinHandle<()> {
            let client = self.clone();
            let stream_id_for_end = stream_id.clone();
            let workspace_root = std::env::current_dir()
                .ok()
                .map(|path| path.to_string_lossy().into_owned());
            tokio::spawn(async move {
                let stream = ::daemon::daemon::open_foreground_chat_session_stream(
                    client.core.clone(),
                    session_id.clone(),
                    Some(input),
                    stream_id,
                    workspace_root,
                )
                .await;
                let mut rx = match stream {
                    Ok(rx) => rx,
                    Err(error) => {
                        let _ = tx.send(AppEvent::Error(format!("Chat stream failed: {error}")));
                        return;
                    }
                };
                let mut saw_terminal_frame = false;
                while let Some(frame) = rx.recv().await {
                    let terminal =
                        matches!(frame, StreamFrame::Done { .. } | StreamFrame::Error(_));
                    saw_terminal_frame |= terminal;
                    if tx.send(AppEvent::StreamFrame(frame)).is_err() {
                        break;
                    }
                    if terminal {
                        break;
                    }
                }

                if !saw_terminal_frame {
                    let _ = tx.send(AppEvent::ChatStreamEndedWithoutTerminal {
                        stream_id: stream_id_for_end,
                    });
                }
            })
        }

        pub async fn steer_chat_stream(
            &self,
            session_id: String,
            instruction: String,
        ) -> Result<bool> {
            Ok(::daemon::daemon::steer_foreground_chat_stream(
                &self.core,
                &session_id,
                &instruction,
            )
            .await)
        }
    }

    fn normalize_model_input(model: &str) -> Result<String> {
        ModelId::normalize_model_id(model)
            .ok_or_else(|| anyhow::anyhow!("Unsupported model identifier: {}", model))
    }

    fn is_catalog_model(model: ModelId) -> bool {
        !model.is_opencode_cli() && !model.is_gemini_cli() && !is_legacy_openai_model(model)
    }

    fn is_legacy_openai_model(model: ModelId) -> bool {
        matches!(
            model,
            ModelId::Gpt5
                | ModelId::Gpt5Mini
                | ModelId::Gpt5Nano
                | ModelId::Gpt5Pro
                | ModelId::Gpt5_1
                | ModelId::Gpt5_2
        )
    }

    fn provider_has_secret(core: &AppCore, provider: Provider) -> bool {
        provider.api_key_env_candidates().any(|key| {
            core.storage
                .secrets
                .get_non_empty(key)
                .ok()
                .flatten()
                .is_some()
        })
    }

    fn available_model_catalog(core: &AppCore) -> Vec<ModelMetadataDTO> {
        let mut providers = Vec::new();
        for provider in Provider::all().iter().copied() {
            if provider == Provider::Codex || provider_has_secret(core, provider) {
                providers.push(provider);
            }
        }
        providers.sort_by_key(|provider| format!("{provider:?}"));

        let mut models = ModelId::all_with_metadata()
            .into_iter()
            .filter(|metadata| is_catalog_model(metadata.model))
            .filter(|metadata| providers.contains(&metadata.provider))
            .collect::<Vec<_>>();
        models.sort_by(|left, right| {
            format!("{:?}", left.provider)
                .cmp(&format!("{:?}", right.provider))
                .then_with(|| model_sort_rank(left.model).cmp(&model_sort_rank(right.model)))
                .then_with(|| left.name.cmp(&right.name))
        });
        models
    }

    fn model_sort_rank(model: ModelId) -> usize {
        if model == ModelId::Gpt5_4Codex {
            return 0;
        }
        if model == ModelId::Gpt5_4MiniCodex {
            return 1;
        }
        if model == ModelId::CodexCli {
            return 2;
        }
        if model == ModelId::Gpt5Codex
            || model == ModelId::Gpt5_1Codex
            || model == ModelId::Gpt5_2Codex
        {
            return 20;
        }
        10
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn codex_catalog_sort_prefers_supported_default() {
            let mut models = [
                ModelId::Gpt5Codex,
                ModelId::Gpt5_1Codex,
                ModelId::Gpt5_4MiniCodex,
                ModelId::Gpt5_4Codex,
                ModelId::CodexCli,
            ];

            models.sort_by_key(|model| model_sort_rank(*model));

            assert_eq!(models[0], ModelId::Gpt5_4Codex);
            assert_eq!(models[1], ModelId::Gpt5_4MiniCodex);
            assert_eq!(models[2], ModelId::CodexCli);
        }
    }
}

mod event_loop {
    use std::collections::VecDeque;
    use std::thread;
    use std::time::{Duration, Instant};

    use anyhow::Result;
    use crossterm::event::{self, Event};
    use tokio::sync::mpsc;

    use super::controller::ShellController;
    use super::keymap::{Action, map_event};
    use super::reducer::{ShellAction, ShellEffect, reduce};
    use super::shell::ShellRenderer;
    use super::state::AppState;

    use types::{ChatSessionEvent, StreamFrame};

    const MAX_BATCHED_INPUT_EVENTS: usize = 64;
    const RENDER_FRAME_INTERVAL: Duration = Duration::from_millis(16);
    const TYPING_ANIMATION_INTERVAL: Duration = Duration::from_millis(250);

    #[derive(Debug)]
    pub enum AppEvent {
        StreamFrame(StreamFrame),
        SessionEvent(ChatSessionEvent),
        ChatStreamEndedWithoutTerminal { stream_id: String },
        Error(String),
    }

    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    struct ProcessActionsResult {
        should_quit: bool,
        render_request: RenderRequest,
        immediate_render: bool,
    }

    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    struct RenderRequest {
        full: bool,
        viewport: bool,
    }

    impl RenderRequest {
        fn viewport() -> Self {
            Self {
                full: false,
                viewport: true,
            }
        }

        fn full() -> Self {
            Self {
                full: true,
                viewport: false,
            }
        }

        fn merge(&mut self, other: Self) {
            self.full |= other.full;
            self.viewport |= other.viewport;
            if self.full {
                self.viewport = false;
            }
        }
    }

    pub async fn run_event_loop(controller: ShellController, mut state: AppState) -> Result<()> {
        let mut renderer = ShellRenderer::new();
        renderer.purge_screen()?;
        let (tx, mut rx) = mpsc::unbounded_channel();
        let (terminal_tx, mut terminal_rx) = mpsc::unbounded_channel();
        let _input_handle = spawn_input_thread(terminal_tx);
        let mut session_stream_handle = if state.is_startup_mode() {
            None
        } else {
            Some(controller.spawn_session_events(tx.clone()))
        };
        let mut pending_terminal_events = VecDeque::new();
        let mut pending_events = VecDeque::new();
        let mut render_request = RenderRequest::full();

        let mut result = process_actions(
            &controller,
            &mut renderer,
            &mut state,
            VecDeque::from([ShellAction::RefreshTick]),
            tx.clone(),
        )
        .await?;
        if result.should_quit {
            return Ok(());
        }
        render_request.merge(result.render_request);
        if result.immediate_render {
            renderer.sync(&mut state)?;
            render_request = RenderRequest::default();
        }

        if let Some(message) = state.take_pending_initial_message() {
            result = process_actions(
                &controller,
                &mut renderer,
                &mut state,
                VecDeque::from([ShellAction::SubmitText { text: message }]),
                tx.clone(),
            )
            .await?;
            if result.should_quit {
                return Ok(());
            }
            render_request.merge(result.render_request);
            if result.immediate_render {
                renderer.sync(&mut state)?;
                render_request = RenderRequest::default();
            }
        }

        let mut tick = tokio::time::interval(Duration::from_secs(3));
        let mut render_tick = tokio::time::interval(RENDER_FRAME_INTERVAL);
        let mut typing_tick = tokio::time::interval(TYPING_ANIMATION_INTERVAL);
        let mut ctrl_c = Box::pin(tokio::signal::ctrl_c());
        let mut last_active_refresh = Instant::now();
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        render_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        typing_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                biased;

                result = &mut ctrl_c => {
                    if let Err(error) = result {
                        state.status = format!("Failed to listen for Ctrl-C: {error}");
                        render_request.merge(RenderRequest::full());
                        ctrl_c = Box::pin(tokio::signal::ctrl_c());
                        continue;
                    }
                    let result = process_actions(
                        &controller,
                        &mut renderer,
                        &mut state,
                        VecDeque::from([ShellAction::Ui(Action::Quit)]),
                        tx.clone(),
                    )
                    .await?;
                    if result.should_quit {
                        break;
                    }
                    render_request.merge(result.render_request);
                    if result.immediate_render {
                        renderer.sync(&mut state)?;
                        render_request = RenderRequest::default();
                    }
                    ctrl_c = Box::pin(tokio::signal::ctrl_c());
                }

                maybe_event = next_terminal_event(&mut terminal_rx, &mut pending_terminal_events) => {
                    let Some(event) = maybe_event else { break; };
                    let actions = collect_terminal_action_batch(event, &mut terminal_rx, &mut pending_terminal_events);
                    let result = process_actions(&controller, &mut renderer, &mut state, actions, tx.clone()).await?;
                    if result.should_quit {
                        break;
                    }
                    render_request.merge(result.render_request);
                    if result.immediate_render {
                        renderer.sync(&mut state)?;
                        render_request = RenderRequest::default();
                    }
                }
                maybe_event = next_event(&mut rx, &mut pending_events) => {
                    let Some(event) = maybe_event else { break; };
                    let actions = VecDeque::from([app_event_to_action(event)]);
                    let result = process_actions(&controller, &mut renderer, &mut state, actions, tx.clone()).await?;
                    if result.should_quit {
                        break;
                    }
                    render_request.merge(result.render_request);
                    if result.immediate_render {
                        renderer.sync(&mut state)?;
                        render_request = RenderRequest::default();
                    }
                }
                _ = tick.tick() => {
                    last_active_refresh = Instant::now();
                    let result = process_actions(
                        &controller,
                        &mut renderer,
                        &mut state,
                        VecDeque::from([ShellAction::RefreshTick]),
                        tx.clone(),
                    )
                    .await?;
                    if result.should_quit {
                        break;
                    }
                    render_request.merge(result.render_request);
                    if result.immediate_render {
                        renderer.sync(&mut state)?;
                        render_request = RenderRequest::default();
                    }
                }
                _ = typing_tick.tick() => {
                    if state.update_active_typing_indicator() {
                        render_request.merge(RenderRequest::viewport());
                    }
                    if should_refresh_active_from_animation(&state, last_active_refresh) {
                        last_active_refresh = Instant::now();
                        let result = process_actions(
                            &controller,
                            &mut renderer,
                            &mut state,
                            VecDeque::from([ShellAction::RefreshTick]),
                            tx.clone(),
                        )
                        .await?;
                        if result.should_quit {
                            break;
                        }
                        render_request.merge(result.render_request);
                        if result.immediate_render {
                            renderer.sync(&mut state)?;
                            render_request = RenderRequest::default();
                        }
                    }
                }
                _ = render_tick.tick() => {
                    if render_request.full {
                        renderer.sync(&mut state)?;
                        render_request = RenderRequest::default();
                    } else if render_request.viewport {
                        renderer.sync_viewport_only(&mut state)?;
                        render_request = RenderRequest::default();
                    }
                }
            }

            sync_session_subscription(&controller, &state, &tx, &mut session_stream_handle);
        }

        if let Some(handle) = session_stream_handle.take() {
            handle.abort();
        }

        Ok(())
    }

    fn spawn_input_thread(tx: mpsc::UnboundedSender<Event>) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            loop {
                if let Ok(true) = event::poll(Duration::from_millis(100)) {
                    match event::read() {
                        Ok(event) => {
                            if tx.send(event).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        })
    }

    async fn next_terminal_event(
        rx: &mut mpsc::UnboundedReceiver<Event>,
        pending_events: &mut VecDeque<Event>,
    ) -> Option<Event> {
        if let Some(event) = pending_events.pop_front() {
            Some(event)
        } else {
            rx.recv().await
        }
    }

    async fn next_event(
        rx: &mut mpsc::UnboundedReceiver<AppEvent>,
        pending_events: &mut VecDeque<AppEvent>,
    ) -> Option<AppEvent> {
        if let Some(event) = pending_events.pop_front() {
            Some(event)
        } else {
            rx.recv().await
        }
    }

    fn collect_terminal_action_batch(
        first_event: Event,
        rx: &mut mpsc::UnboundedReceiver<Event>,
        pending_events: &mut VecDeque<Event>,
    ) -> VecDeque<ShellAction> {
        let first_action = ShellAction::Ui(map_event(first_event));
        if !is_batchable_input_action(&first_action) {
            return VecDeque::from([first_action]);
        }

        let mut actions = VecDeque::from([first_action]);
        while actions.len() < MAX_BATCHED_INPUT_EVENTS {
            match rx.try_recv() {
                Ok(event) => {
                    let action = ShellAction::Ui(map_event(event.clone()));
                    if is_batchable_input_action(&action) {
                        actions.push_back(action);
                    } else {
                        pending_events.push_back(event);
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        actions
    }

    fn app_event_to_action(event: AppEvent) -> ShellAction {
        match event {
            AppEvent::StreamFrame(frame) => ShellAction::StreamFrame(frame),
            AppEvent::SessionEvent(event) => ShellAction::SessionEvent(event),
            AppEvent::ChatStreamEndedWithoutTerminal { stream_id } => {
                ShellAction::ChatStreamEndedWithoutTerminal { stream_id }
            }
            AppEvent::Error(message) => ShellAction::Error(message),
        }
    }

    fn is_batchable_input_action(action: &ShellAction) -> bool {
        matches!(
            action,
            ShellAction::Ui(
                Action::InputChar(_)
                    | Action::InputBackspace
                    | Action::MoveLeft
                    | Action::MoveRight
                    | Action::Newline
                    | Action::Paste(_)
            )
        )
    }

    async fn process_actions(
        controller: &ShellController,
        renderer: &mut ShellRenderer,
        state: &mut AppState,
        actions: VecDeque<ShellAction>,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<ProcessActionsResult> {
        let mut pending = actions;
        let mut output = ProcessActionsResult::default();

        while let Some(next_action) = pending.pop_front() {
            output
                .render_request
                .merge(render_request_for_action(&next_action));
            output.immediate_render |= action_requires_immediate_render(&next_action);

            let result = reduce(state, next_action);
            if result.should_quit {
                output.should_quit = true;
                return Ok(output);
            }

            pending.extend(result.actions);

            for effect in result.effects {
                if effect_requires_pre_render(&effect) {
                    output.render_request.merge(RenderRequest::full());
                    output.immediate_render = true;
                }
                if matches!(effect, ShellEffect::ClearScreen) {
                    renderer.clear_screen()?;
                    continue;
                }

                if output.immediate_render {
                    renderer.sync(state)?;
                    output.render_request = RenderRequest::default();
                    output.immediate_render = false;
                }

                let followup_actions =
                    match controller.execute_effect(effect, state, tx.clone()).await {
                        Ok(actions) => actions,
                        Err(error) => vec![ShellAction::Error(error.to_string())],
                    };
                pending.extend(followup_actions);
            }
        }

        Ok(output)
    }

    fn render_request_for_action(action: &ShellAction) -> RenderRequest {
        if matches!(
            action,
            ShellAction::Ui(Action::Noop) | ShellAction::RefreshTick
        ) {
            RenderRequest::default()
        } else if is_batchable_input_action(action) {
            RenderRequest::viewport()
        } else {
            RenderRequest::full()
        }
    }

    fn action_requires_immediate_render(action: &ShellAction) -> bool {
        !matches!(
            action,
            ShellAction::RefreshTick
                | ShellAction::Ui(
                    Action::InputChar(_)
                        | Action::InputBackspace
                        | Action::MoveLeft
                        | Action::MoveRight
                        | Action::Newline
                        | Action::Paste(_)
                        | Action::Noop
                )
        )
    }

    fn effect_requires_pre_render(effect: &ShellEffect) -> bool {
        !matches!(
            effect,
            ShellEffect::RefreshState | ShellEffect::ReloadCurrentSession
        )
    }

    fn should_refresh_active_from_animation(state: &AppState, last_refresh: Instant) -> bool {
        (state.is_streaming || state.active_turn.is_some())
            && last_refresh.elapsed() >= Duration::from_secs(1)
    }

    fn sync_session_subscription(
        controller: &ShellController,
        state: &AppState,
        tx: &mpsc::UnboundedSender<AppEvent>,
        slot: &mut Option<tokio::task::JoinHandle<()>>,
    ) {
        match (slot.is_some(), state.is_startup_mode()) {
            (false, false) => {
                *slot = Some(controller.spawn_session_events(tx.clone()));
            }
            (true, true) => {
                if let Some(handle) = slot.take() {
                    handle.abort();
                }
            }
            _ => {}
        }
    }

    #[cfg(test)]
    mod tests {
        use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

        use super::*;

        fn key(code: KeyCode) -> Event {
            Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
        }

        #[test]
        fn collect_terminal_action_batch_drains_contiguous_input_events() {
            let (tx, mut rx) = mpsc::unbounded_channel();
            let mut pending_events = VecDeque::new();
            tx.send(key(KeyCode::Char('b'))).unwrap();
            tx.send(key(KeyCode::Char('c'))).unwrap();
            tx.send(key(KeyCode::Enter)).unwrap();

            let actions = collect_terminal_action_batch(
                key(KeyCode::Char('a')),
                &mut rx,
                &mut pending_events,
            );

            let input = actions
                .into_iter()
                .map(|action| match action {
                    ShellAction::Ui(Action::InputChar(ch)) => ch,
                    other => panic!("unexpected action in input batch: {other:?}"),
                })
                .collect::<String>();
            assert_eq!(input, "abc");
            assert!(matches!(
                pending_events.pop_front(),
                Some(Event::Key(event)) if event.code == KeyCode::Enter
            ));
        }

        #[test]
        fn collect_terminal_action_batch_does_not_batch_submit_first() {
            let (_tx, mut rx) = mpsc::unbounded_channel();
            let mut pending_events = VecDeque::new();

            let actions =
                collect_terminal_action_batch(key(KeyCode::Enter), &mut rx, &mut pending_events);

            assert_eq!(actions.len(), 1);
            assert!(matches!(
                actions.front(),
                Some(ShellAction::Ui(Action::Submit))
            ));
            assert!(pending_events.is_empty());
        }

        #[test]
        fn collect_terminal_action_batch_stops_before_non_batchable_terminal_event() {
            let (tx, mut rx) = mpsc::unbounded_channel();
            let mut pending_events = VecDeque::new();
            tx.send(key(KeyCode::Enter)).unwrap();

            let actions = collect_terminal_action_batch(
                key(KeyCode::Char('a')),
                &mut rx,
                &mut pending_events,
            );

            assert_eq!(actions.len(), 1);
            assert!(matches!(
                actions.front(),
                Some(ShellAction::Ui(Action::InputChar('a')))
            ));
            assert!(matches!(
                pending_events.pop_front(),
                Some(Event::Key(event)) if event.code == KeyCode::Enter
            ));
        }

        #[test]
        fn input_actions_are_dirty_but_not_immediate() {
            let action = ShellAction::Ui(Action::InputChar('x'));
            let request = render_request_for_action(&action);

            assert_eq!(request, RenderRequest::viewport());
            assert!(request.viewport);
            assert!(!request.full);
            assert!(!action_requires_immediate_render(&action));
        }

        #[test]
        fn refresh_tick_is_not_dirty_by_itself() {
            let action = ShellAction::RefreshTick;
            let request = render_request_for_action(&action);

            assert_eq!(request, RenderRequest::default());
            assert!(!action_requires_immediate_render(&action));
        }

        #[test]
        fn animation_tick_can_drive_active_refresh_when_interval_is_due() {
            let mut state = AppState::empty();
            state.push_local_user_message("run live work".to_string());
            let last_refresh = Instant::now() - Duration::from_secs(2);

            assert!(should_refresh_active_from_animation(&state, last_refresh));
        }

        #[test]
        fn animation_tick_does_not_refresh_idle_state() {
            let state = AppState::empty();
            let last_refresh = Instant::now() - Duration::from_secs(2);

            assert!(!should_refresh_active_from_animation(&state, last_refresh));
        }

        #[test]
        fn refresh_effects_do_not_pre_render() {
            assert!(!effect_requires_pre_render(&ShellEffect::RefreshState));
            assert!(!effect_requires_pre_render(
                &ShellEffect::ReloadCurrentSession
            ));
        }

        #[test]
        fn submit_and_resize_actions_render_immediately() {
            assert!(action_requires_immediate_render(&ShellAction::Ui(
                Action::Submit
            )));
            assert!(action_requires_immediate_render(&ShellAction::Ui(
                Action::Resize
            )));
        }
    }
}

mod keymap {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEventKind};

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Action {
        Quit,
        CloseOverlay,
        OpenSessions,
        OpenRuns,
        OpenHelp,
        CycleInputMode,
        Redraw,
        Resize,
        NavUp,
        NavDown,
        MoveLeft,
        MoveRight,
        MoveStart,
        MoveEnd,
        ScrollUp,
        ScrollDown,
        WheelUp,
        WheelDown,
        InputChar(char),
        Paste(String),
        InputBackspace,
        Newline,
        Submit,
        OverlaySelect,
        SetSelectedDefaultModel,
        DeleteSelected,
        Noop,
    }

    pub fn map_event(event: Event) -> Action {
        match event {
            Event::Paste(text) => Action::Paste(text),
            Event::Resize(_, _) => Action::Resize,
            Event::Mouse(event) => match event.kind {
                MouseEventKind::ScrollUp => Action::WheelUp,
                MouseEventKind::ScrollDown => Action::WheelDown,
                _ => Action::Noop,
            },
            Event::Key(KeyEvent {
                code: KeyCode::Char('c'),
                modifiers,
                ..
            }) if modifiers.contains(KeyModifiers::CONTROL)
                || modifiers.contains(KeyModifiers::SUPER) =>
            {
                Action::Quit
            }
            Event::Key(KeyEvent {
                code: KeyCode::Esc, ..
            }) => Action::CloseOverlay,
            Event::Key(KeyEvent {
                code: KeyCode::BackTab,
                ..
            }) => Action::CycleInputMode,
            Event::Key(KeyEvent {
                code: KeyCode::Char('p'),
                modifiers,
                ..
            }) if modifiers.contains(KeyModifiers::CONTROL) => Action::OpenSessions,
            Event::Key(KeyEvent {
                code: KeyCode::Char('d'),
                modifiers,
                ..
            }) if modifiers.is_empty() => Action::DeleteSelected,
            Event::Key(KeyEvent {
                code: KeyCode::Char('r'),
                modifiers,
                ..
            }) if modifiers.contains(KeyModifiers::CONTROL) => Action::OpenRuns,
            Event::Key(KeyEvent {
                code: KeyCode::Char('l'),
                modifiers,
                ..
            }) if modifiers.contains(KeyModifiers::CONTROL) => Action::Redraw,
            Event::Key(KeyEvent {
                code: KeyCode::Char('j'),
                modifiers,
                ..
            }) if modifiers.contains(KeyModifiers::CONTROL) => Action::Newline,
            Event::Key(KeyEvent {
                code: KeyCode::Char('a'),
                modifiers,
                ..
            }) if modifiers.contains(KeyModifiers::CONTROL) => Action::MoveStart,
            Event::Key(KeyEvent {
                code: KeyCode::Char('e'),
                modifiers,
                ..
            }) if modifiers.contains(KeyModifiers::CONTROL) => Action::MoveEnd,
            Event::Key(KeyEvent {
                code: KeyCode::Char('?'),
                modifiers,
                ..
            }) if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT => Action::OpenHelp,
            Event::Key(KeyEvent {
                code: KeyCode::Up, ..
            }) => Action::NavUp,
            Event::Key(KeyEvent {
                code: KeyCode::Down,
                ..
            }) => Action::NavDown,
            Event::Key(KeyEvent {
                code: KeyCode::Left,
                ..
            }) => Action::MoveLeft,
            Event::Key(KeyEvent {
                code: KeyCode::Right,
                ..
            }) => Action::MoveRight,
            Event::Key(KeyEvent {
                code: KeyCode::Home,
                ..
            }) => Action::MoveStart,
            Event::Key(KeyEvent {
                code: KeyCode::End, ..
            }) => Action::MoveEnd,
            Event::Key(KeyEvent {
                code: KeyCode::PageUp,
                ..
            }) => Action::ScrollUp,
            Event::Key(KeyEvent {
                code: KeyCode::PageDown,
                ..
            }) => Action::ScrollDown,
            Event::Key(KeyEvent {
                code: KeyCode::Backspace,
                ..
            }) => Action::InputBackspace,
            Event::Key(KeyEvent {
                code: KeyCode::Enter,
                modifiers,
                ..
            }) if modifiers.contains(KeyModifiers::ALT) => Action::OverlaySelect,
            Event::Key(KeyEvent {
                code: KeyCode::Enter,
                modifiers,
                ..
            }) if modifiers.contains(KeyModifiers::SHIFT) => Action::SetSelectedDefaultModel,
            Event::Key(KeyEvent {
                code: KeyCode::Enter,
                ..
            }) => Action::Submit,
            Event::Key(KeyEvent {
                code: KeyCode::Char(ch),
                modifiers,
                ..
            }) if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT => Action::InputChar(ch),
            _ => Action::Noop,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crossterm::event::{
            Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind,
        };

        #[test]
        fn maps_ctrl_c_to_quit() {
            let event = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
            assert_eq!(map_event(event), Action::Quit);
        }

        #[test]
        fn maps_command_c_to_quit() {
            let event = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::SUPER));
            assert_eq!(map_event(event), Action::Quit);
        }

        #[test]
        fn maps_esc_to_close_overlay_without_quit() {
            let event = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
            assert_eq!(map_event(event), Action::CloseOverlay);
        }

        #[test]
        fn maps_shift_tab_to_cycle_input_mode() {
            let event = Event::Key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
            assert_eq!(map_event(event), Action::CycleInputMode);
        }

        #[test]
        fn maps_shift_enter_to_set_selected_default_model() {
            let event = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
            assert_eq!(map_event(event), Action::SetSelectedDefaultModel);
        }

        #[test]
        fn maps_ctrl_p_to_open_sessions() {
            let event = Event::Key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
            assert_eq!(map_event(event), Action::OpenSessions);
        }

        #[test]
        fn maps_d_to_delete_selected() {
            let event = Event::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
            assert_eq!(map_event(event), Action::DeleteSelected);
        }

        #[test]
        fn maps_ctrl_j_to_newline() {
            let event = Event::Key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL));
            assert_eq!(map_event(event), Action::Newline);
        }

        #[test]
        fn maps_composer_boundary_keys() {
            assert_eq!(
                map_event(Event::Key(KeyEvent::new(
                    KeyCode::Char('a'),
                    KeyModifiers::CONTROL
                ))),
                Action::MoveStart
            );
            assert_eq!(
                map_event(Event::Key(KeyEvent::new(
                    KeyCode::Char('e'),
                    KeyModifiers::CONTROL
                ))),
                Action::MoveEnd
            );
            assert_eq!(
                map_event(Event::Key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE))),
                Action::MoveStart
            );
            assert_eq!(
                map_event(Event::Key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE))),
                Action::MoveEnd
            );
        }

        #[test]
        fn maps_paste_event_to_paste_action() {
            assert_eq!(
                map_event(Event::Paste("hello\nworld".to_string())),
                Action::Paste("hello\nworld".to_string())
            );
        }

        #[test]
        fn maps_resize_event_to_resize_action() {
            assert_eq!(map_event(Event::Resize(120, 40)), Action::Resize);
        }

        #[test]
        fn maps_mouse_wheel_to_scroll_actions() {
            let up = Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollUp,
                column: 0,
                row: 0,
                modifiers: KeyModifiers::NONE,
            });
            let down = Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 0,
                row: 0,
                modifiers: KeyModifiers::NONE,
            });

            assert_eq!(map_event(up), Action::WheelUp);
            assert_eq!(map_event(down), Action::WheelDown);
        }
    }
}

mod reducer {
    use super::composer::ComposerMode;
    use super::keymap::Action;
    use super::slash_command::{SLASH_COMMAND_SPECS, SlashCommand, parse_slash_command};
    use super::state::{
        AppState, ModelPickerItem, PendingSessionState, ProviderPickerItem, SkillManagerSelection,
        SkillPickerItem,
    };
    use ::daemon::StoredAgent;
    use types::{ChatSession, ChatSessionSummary, ModelMetadataDTO, RunSummary};
    use types::{ChatSessionEvent, StreamFrame};

    const MESSAGE_SCROLL_PAGE_ROWS: usize = 8;
    const MESSAGE_SCROLL_WHEEL_ROWS: usize = 1;

    #[derive(Debug)]
    pub enum ShellAction {
        Ui(Action),
        StreamFrame(StreamFrame),
        SessionEvent(ChatSessionEvent),
        ChatStreamEndedWithoutTerminal {
            stream_id: String,
        },
        StateRefreshed {
            sessions: Vec<ChatSessionSummary>,
            runs: Vec<RunSummary>,
        },
        SessionPickerLoaded {
            sessions: Vec<ChatSessionSummary>,
            status: String,
        },
        SessionDeleted {
            session_id: String,
            deleted: bool,
            sessions: Vec<ChatSessionSummary>,
            status: String,
        },
        CurrentSessionReloaded {
            session: Option<Box<ChatSession>>,
            runs: Vec<RunSummary>,
        },
        SessionOpened {
            session: Box<ChatSession>,
            runs: Vec<RunSummary>,
            status: String,
        },
        SessionCreatedForSubmit {
            session: Box<ChatSession>,
            runs: Vec<RunSummary>,
            message: String,
        },
        RunPickerLoaded {
            runs: Vec<RunSummary>,
            status: String,
        },
        SkillPickerLoaded {
            skills: Vec<SkillPickerItem>,
            status: String,
        },
        SkillMentionPickerLoaded {
            skills: Vec<SkillPickerItem>,
            status: String,
        },
        SkillDetailLoaded {
            skill: Box<types::Skill>,
            status: String,
        },
        ProviderPickerLoaded {
            items: Vec<ProviderPickerItem>,
            available_models: Vec<ModelMetadataDTO>,
            sessions: Vec<ChatSessionSummary>,
            status: String,
        },
        ModelPickerLoaded {
            provider: String,
            items: Vec<ModelPickerItem>,
            status: String,
        },
        ModelSwitched {
            session: Box<ChatSession>,
            status: String,
        },
        PendingSessionModelSelected {
            provider: String,
            model: String,
            model_name: String,
            status: String,
        },
        StatusUpdated(String),
        DaemonStarted {
            agent: Option<Box<StoredAgent>>,
            session: Option<Box<ChatSession>>,
            pending_session: Option<PendingSessionState>,
            status: String,
        },
        DaemonStopped {
            status: String,
        },
        CommandPicked {
            text: String,
        },
        NewChatStarted {
            status: String,
        },
        OpenHelpOverlay,
        OpenDaemonPicker,
        SubmitText {
            text: String,
        },
        Quit,
        RefreshTick,
        Error(String),
    }

    #[derive(Debug, Clone)]
    pub enum ShellEffect {
        ClearScreen,
        RefreshState,
        ReloadCurrentSession,
        ActivateOverlaySelection,
        CreateSessionForSubmit {
            message: String,
        },
        SubmitMessage {
            message: String,
            stream_id: String,
        },
        SteerMessage {
            session_id: String,
            instruction: String,
        },
        CancelStream {
            stream_id: String,
        },
        ExecuteSlashCommand(SlashCommand),
        DeleteSession {
            session_id: String,
        },
        ListSkillsForMention,
        ListSessionsInline,
        ListRunsInline,
    }

    #[derive(Debug, Default)]
    pub struct ReducerOutput {
        pub should_quit: bool,
        pub actions: Vec<ShellAction>,
        pub effects: Vec<ShellEffect>,
    }

    pub fn reduce(state: &mut AppState, action: ShellAction) -> ReducerOutput {
        let mut output = ReducerOutput::default();
        match action {
            ShellAction::Ui(action) => reduce_ui(state, action, &mut output),
            ShellAction::StreamFrame(frame) => {
                let should_reload_session = matches!(
                    frame,
                    StreamFrame::ToolResult { .. }
                        | StreamFrame::Done { .. }
                        | StreamFrame::Error(_)
                );
                let could_reload_session = should_reload_active_session(state);
                let applied = state.apply_stream_frame(frame);
                if applied
                    && should_reload_session
                    && (could_reload_session || should_reload_active_session(state))
                {
                    output.effects.push(ShellEffect::ReloadCurrentSession);
                }
            }
            ShellAction::SessionEvent(event) => {
                let refresh_current = state.active_refresh_session_id()
                    == Some(session_id_of(&event))
                    || (state.active_turn_has_tool_call()
                        && (state.is_streaming || state.active_turn.is_some()));
                let is_message_added = matches!(event, ChatSessionEvent::MessageAdded { .. });
                state.apply_session_event(event);
                if !is_message_added {
                    output.effects.push(if refresh_current {
                        ShellEffect::ReloadCurrentSession
                    } else {
                        ShellEffect::RefreshState
                    });
                } else if refresh_current && (state.is_streaming || state.active_turn.is_some()) {
                    output.effects.push(ShellEffect::ReloadCurrentSession);
                }
            }
            ShellAction::ChatStreamEndedWithoutTerminal { stream_id } => {
                if state.suppress_missing_terminal_frame_for_stream(&stream_id)
                    || state.complete_stream_without_terminal_frame(&stream_id)
                {
                    if state.active_refresh_session_id().is_some() {
                        output.effects.push(ShellEffect::ReloadCurrentSession);
                    }
                } else {
                    state.cancel_active_response();
                    let message = "Chat stream ended before a terminal frame.".to_string();
                    state.status = message.clone();
                    state.push_error(message);
                }
            }
            ShellAction::StateRefreshed { sessions, runs } => {
                state.sessions = sessions;
                if state.current_session_id().is_some() {
                    state.set_session_runs(runs);
                } else {
                    state.clear_thread_runs();
                }
            }
            ShellAction::SessionPickerLoaded { sessions, status } => {
                state.sessions = sessions;
                state.open_session_picker();
                state.status = status;
            }
            ShellAction::SessionDeleted {
                session_id,
                deleted,
                sessions,
                status,
            } => {
                state.apply_session_delete_result(&session_id, sessions);
                state.status = status.clone();
                if deleted {
                    state.push_info(status);
                } else {
                    state.push_error(status);
                }
            }
            ShellAction::CurrentSessionReloaded { session, runs } => {
                if let Some(session) = session {
                    state.refresh_current_session(*session);
                    state.set_session_runs(runs);
                } else if state.is_streaming || state.active_turn.is_some() {
                    state.set_session_runs(runs);
                } else {
                    state.clear_current_session("The active session is no longer available.");
                }
            }
            ShellAction::SessionOpened {
                session,
                runs,
                status,
            } => {
                state.set_current_session(*session);
                state.set_session_runs(runs);
                state.clear_overlay();
                state.status = status;
            }
            ShellAction::SessionCreatedForSubmit {
                session,
                runs,
                message,
            } => {
                state.set_current_session(*session);
                state.set_session_runs(runs);
                state.push_local_user_message(message.clone());
                state.start_assistant_typing();
                state.status = "Sending message...".to_string();
                output.effects.push(submit_message_effect(state, message));
            }
            ShellAction::RunPickerLoaded { runs, status } => {
                if state.current_session_id().is_some() {
                    state.set_session_runs(runs);
                } else {
                    state.clear_thread_runs();
                }
                state.open_run_picker();
                state.status = status;
            }
            ShellAction::SkillPickerLoaded { skills, status } => {
                state.skills = skills;
                state.skills_loaded = true;
                state.open_skill_manager();
                state.status = status;
            }
            ShellAction::SkillMentionPickerLoaded { skills, status } => {
                state.skills = skills;
                state.skills_loaded = true;
                if state.composer.current_skill_mention_query().is_some() {
                    state.open_skill_mention_picker();
                    state.sync_skill_mention_picker_to_draft();
                }
                state.status = status;
            }
            ShellAction::SkillDetailLoaded { skill, status } => {
                state.composer.clear();
                state.open_skill_detail(*skill);
                state.status = status;
            }
            ShellAction::ProviderPickerLoaded {
                items,
                available_models,
                sessions,
                status,
            } => {
                state.provider_items = items;
                state.available_models = available_models;
                state.sessions = sessions;
                state.open_provider_picker();
                state.status = status;
            }
            ShellAction::ModelPickerLoaded {
                provider,
                items,
                status,
            } => {
                state.model_items = items;
                state.open_model_picker(provider);
                state.status = status;
            }
            ShellAction::ModelSwitched { session, status } => {
                state.refresh_current_session(*session);
                state.clear_overlay();
                state.status = status;
            }
            ShellAction::PendingSessionModelSelected {
                provider,
                model,
                model_name,
                status,
            } => {
                if state.update_pending_session_model(provider, model, model_name) {
                    state.clear_overlay();
                    state.status = status;
                } else {
                    state.status =
                        "No default agent is available. Start the daemon or send a message first."
                            .to_string();
                }
            }
            ShellAction::StatusUpdated(status) => state.status = status,
            ShellAction::DaemonStarted {
                agent,
                session,
                pending_session,
                status,
            } => {
                state.exit_startup();
                if let Some(agent) = agent.as_ref() {
                    state.set_default_agent(Some(agent.id.clone()), Some(agent.name.clone()));
                } else {
                    state.set_default_agent(None, None);
                }
                if let Some(session) = session {
                    state.set_current_session(*session);
                } else if let Some(pending_session) = pending_session {
                    state.set_pending_session(Some(pending_session));
                } else if let Some(agent) = agent.as_ref() {
                    state.set_pending_session(Some(PendingSessionState::from_agent(agent)));
                }
                state.status = status;
                if let Some(message) = state.take_pending_initial_message()
                    && !message.trim().is_empty()
                    && state.default_agent_id.is_some()
                {
                    output
                        .actions
                        .push(ShellAction::SubmitText { text: message });
                }
            }
            ShellAction::DaemonStopped { status } => {
                let agent_override = state.default_agent_id.clone();
                let session_override = state.current_session_id().map(ToOwned::to_owned);
                state.enter_startup(agent_override, session_override);
                state.status = status.clone();
                state.push_info(status);
            }
            ShellAction::CommandPicked { text } => {
                state.clear_overlay();
                state.composer.replace(text);
                state.status = "Command selected".to_string();
            }
            ShellAction::NewChatStarted { status } => {
                state.start_new_chat();
                state.status = status;
            }
            ShellAction::OpenHelpOverlay => {
                state.composer.clear();
                state.open_help_overlay();
                state.status = "Showing help".to_string();
            }
            ShellAction::OpenDaemonPicker => {
                state.composer.clear();
                state.open_daemon_picker();
                state.status = "Select daemon action".to_string();
            }
            ShellAction::SubmitText { text } => reduce_submit_text(state, text, &mut output),
            ShellAction::Quit => output.should_quit = true,
            ShellAction::RefreshTick => {
                if !state.is_startup_mode() {
                    if should_reload_active_session(state) {
                        output.effects.push(ShellEffect::ReloadCurrentSession);
                    } else {
                        output.effects.push(ShellEffect::RefreshState);
                    }
                }
            }
            ShellAction::Error(message) => {
                if state.is_startup_mode() {
                    state.set_startup_error(message);
                } else {
                    state.cancel_active_response();
                    state.status = message.clone();
                    state.push_error(message);
                }
            }
        }
        output
    }

    fn session_id_of(event: &ChatSessionEvent) -> &str {
        match event {
            ChatSessionEvent::Created { session_id }
            | ChatSessionEvent::Updated { session_id }
            | ChatSessionEvent::MessageAdded { session_id, .. }
            | ChatSessionEvent::Deleted { session_id } => session_id,
        }
    }

    fn should_reload_active_session(state: &AppState) -> bool {
        state.pending_runtime_refresh_session_id().is_some()
            || ((state.active_refresh_session_id().is_some() || state.active_turn_has_tool_call())
                && (state.is_streaming || state.active_turn.is_some()))
    }

    fn reduce_ui(state: &mut AppState, action: Action, output: &mut ReducerOutput) {
        match action {
            Action::Quit => {
                if state.is_streaming || state.active_turn.is_some() {
                    cancel_active_response(state, output);
                } else {
                    output.should_quit = true;
                }
            }
            Action::CloseOverlay => {
                if state.is_streaming || state.active_turn.is_some() {
                    cancel_active_response(state, output);
                } else if state.overlay.is_some() {
                    state.clear_overlay();
                    if matches!(state.composer.mode(), ComposerMode::Command) {
                        state.composer.clear();
                    }
                } else if !state.composer.is_blank() {
                    let was_command_mode = matches!(state.composer.mode(), ComposerMode::Command);
                    state.composer.clear();
                    state.status = if was_command_mode {
                        "Returned to message mode".to_string()
                    } else {
                        "Cleared input".to_string()
                    };
                } else {
                    state.status = "Input already empty. Press Ctrl-C to quit.".to_string();
                }
            }
            Action::OpenSessions => output.effects.push(ShellEffect::ListSessionsInline),
            Action::OpenRuns => output.effects.push(ShellEffect::ListRunsInline),
            Action::OpenHelp => output.actions.push(ShellAction::OpenHelpOverlay),
            Action::CycleInputMode => {
                if state.overlay.is_none() && !response_in_progress(state) {
                    state.cycle_input_mode();
                }
            }
            Action::Resize => output.effects.push(ShellEffect::ClearScreen),
            Action::Redraw => {
                state.status = "Screen redrawn".to_string();
                output.effects.push(ShellEffect::ClearScreen);
            }
            Action::NavUp => {
                if state.overlay.is_some() {
                    state.move_overlay_selection(-1);
                } else if state.composer.is_blank() {
                    state.composer.history_previous();
                }
            }
            Action::NavDown => {
                if state.overlay.is_some() {
                    state.move_overlay_selection(1);
                } else if state.composer.is_navigating_history() {
                    state.composer.history_next();
                }
            }
            Action::MoveLeft => {
                state.composer.move_left();
            }
            Action::MoveRight => {
                state.composer.move_right();
            }
            Action::MoveStart => {
                state.composer.move_start();
            }
            Action::MoveEnd => {
                state.composer.move_end();
            }
            Action::ScrollUp => {
                if state.overlay.is_none() {
                    state.scroll_message_up(MESSAGE_SCROLL_PAGE_ROWS);
                }
            }
            Action::ScrollDown => {
                if state.overlay.is_none() {
                    state.scroll_message_down(MESSAGE_SCROLL_PAGE_ROWS);
                }
            }
            Action::WheelUp => {
                if state.overlay.is_none() {
                    state.scroll_message_up(MESSAGE_SCROLL_WHEEL_ROWS);
                }
            }
            Action::WheelDown => {
                if state.overlay.is_none() {
                    state.scroll_message_down(MESSAGE_SCROLL_WHEEL_ROWS);
                }
            }
            Action::DeleteSelected => {
                if matches!(
                    state.overlay,
                    Some(crate::state::OverlayState::SessionPicker { .. })
                ) {
                    let selected = state.selected_session_summary().cloned();
                    if let Some(session) = selected {
                        if state.is_session_delete_pending(&session.id) {
                            output.effects.push(ShellEffect::DeleteSession {
                                session_id: session.id,
                            });
                        } else {
                            state.mark_session_delete_pending(session.id.clone());
                            state.status =
                                format!("Press d again to delete session {}", session.name);
                        }
                    }
                } else if matches!(
                    state.overlay,
                    Some(crate::state::OverlayState::SkillManager { .. })
                ) {
                    match state.selected_skill_manager_item() {
                        Some(SkillManagerSelection::Skill(skill)) => {
                            state.status = format!(
                                "Skill {} is managed by skrun; edit or remove it from ~/.restflow/skills",
                                skill.id
                            );
                        }
                        None => {}
                    }
                } else if state.overlay.is_none()
                    || matches!(
                        state.overlay,
                        Some(
                            crate::state::OverlayState::CommandPicker { .. }
                                | crate::state::OverlayState::SkillMentionPicker { .. }
                        )
                    )
                {
                    state.composer.insert_char('d');
                    sync_composer_overlay(state, output);
                }
            }
            Action::InputChar(ch) => {
                if state.overlay.is_none()
                    || matches!(
                        state.overlay,
                        Some(
                            crate::state::OverlayState::CommandPicker { .. }
                                | crate::state::OverlayState::SkillMentionPicker { .. }
                        )
                    )
                {
                    state.composer.insert_char(ch);
                    sync_composer_overlay(state, output);
                }
            }
            Action::Paste(text) => {
                if state.overlay.is_none()
                    || matches!(
                        state.overlay,
                        Some(
                            crate::state::OverlayState::CommandPicker { .. }
                                | crate::state::OverlayState::SkillMentionPicker { .. }
                        )
                    )
                {
                    for ch in text.chars() {
                        state.composer.insert_char(ch);
                    }
                    sync_composer_overlay(state, output);
                }
            }
            Action::InputBackspace => {
                if state.overlay.is_none()
                    || matches!(
                        state.overlay,
                        Some(
                            crate::state::OverlayState::CommandPicker { .. }
                                | crate::state::OverlayState::SkillMentionPicker { .. }
                        )
                    )
                {
                    state.composer.backspace();
                    sync_composer_overlay(state, output);
                }
            }
            Action::Newline => {
                if state.overlay.is_none() {
                    state.composer.insert_newline();
                }
            }
            Action::OverlaySelect => {}
            Action::SetSelectedDefaultModel => {
                if matches!(
                    state.overlay,
                    Some(crate::state::OverlayState::ModelPicker { .. })
                ) && let Some(item) = state.selected_model_item()
                {
                    state.status = "Setting default model...".to_string();
                    output.effects.push(ShellEffect::ExecuteSlashCommand(
                        SlashCommand::SetDefaultModel { model: item.model },
                    ));
                }
            }
            Action::Submit => {
                if state.overlay.is_some() {
                    if matches!(
                        state.overlay,
                        Some(crate::state::OverlayState::CommandPicker { .. })
                    ) {
                        let input = state.composer.draft().trim().to_string();
                        if input != "/" && parse_slash_command(&input).is_ok() {
                            state.composer.clear();
                            state.clear_overlay();
                            output.actions.push(ShellAction::SubmitText { text: input });
                        } else {
                            output.effects.push(ShellEffect::ActivateOverlaySelection);
                        }
                    } else if matches!(
                        state.overlay,
                        Some(crate::state::OverlayState::SkillMentionPicker { .. })
                    ) {
                        if let Some(skill) = state.selected_skill_mention_item()
                            && state.composer.replace_current_skill_mention(&skill.id)
                        {
                            state.status = format!("Inserted @{}", skill.id);
                        }
                        state.clear_overlay();
                    } else {
                        if matches!(
                            state.overlay,
                            Some(crate::state::OverlayState::ModelPicker { .. })
                        ) {
                            state.status = "Switching model...".to_string();
                        } else if matches!(
                            state.overlay,
                            Some(crate::state::OverlayState::ProviderPicker { .. })
                        ) {
                            state.status = "Loading models...".to_string();
                        } else if matches!(
                            state.overlay,
                            Some(crate::state::OverlayState::SkillManager { .. })
                        ) {
                            state.status = "Loading skill...".to_string();
                        }
                        output.effects.push(ShellEffect::ActivateOverlaySelection);
                    }
                } else if response_in_progress(state) {
                    steer_active_response(state, output);
                } else {
                    let input = state.composer.take_submission();
                    if !input.trim().is_empty() {
                        state.composer.remember_submission(&input);
                        output.actions.push(ShellAction::SubmitText { text: input });
                    }
                }
            }
            Action::Noop => {}
        }
    }

    fn response_in_progress(state: &AppState) -> bool {
        state.is_streaming || state.active_turn.is_some()
    }

    fn steer_active_response(state: &mut AppState, output: &mut ReducerOutput) {
        if state.composer.draft().trim().is_empty() {
            return;
        }

        let Some(session_id) = state.current_session_id().map(ToOwned::to_owned) else {
            let message =
                "Response is still starting. Press Esc to cancel before sending another message.";
            state.status = message.to_string();
            state.push_info(message.to_string());
            return;
        };

        let instruction = state.composer.take_submission();
        state.composer.remember_submission(&instruction);
        state.queue_active_turn_update(instruction.clone());
        state.status = "Queued update for current response. Press Esc to interrupt.".to_string();
        output.effects.push(ShellEffect::SteerMessage {
            session_id,
            instruction,
        });
    }

    fn cancel_active_response(state: &mut AppState, output: &mut ReducerOutput) {
        let stream_id = state.current_stream_id.clone();
        state.cancel_active_response();
        state.push_info("Canceled current response.");
        if let Some(stream_id) = stream_id {
            state.status = "Canceling response...".to_string();
            output.effects.push(ShellEffect::CancelStream { stream_id });
        } else {
            state.status = "Canceled current response.".to_string();
        }
    }

    fn reduce_submit_text(state: &mut AppState, text: String, output: &mut ReducerOutput) {
        if super::composer::ComposerState::is_command_text(&text) {
            match parse_slash_command(&text) {
                Ok(command) => {
                    state.status = slash_command_pending_status(&command).to_string();
                    output
                        .effects
                        .push(ShellEffect::ExecuteSlashCommand(command));
                }
                Err(error) => {
                    state.status = error.to_string();
                    state.push_error(error.to_string());
                }
            }
        } else if state.current_session_id().is_none() {
            if state.is_startup_mode() {
                let message = "Daemon is offline. Use /daemon to launch it.".to_string();
                state.status = message.clone();
                state.push_error(message);
            } else if state.default_agent_id.is_some() {
                state.push_local_user_message(text.clone());
                state.start_assistant_typing();
                state.status = "Creating session...".to_string();
                output
                    .effects
                    .push(ShellEffect::CreateSessionForSubmit { message: text });
            } else {
                let message =
                    "No active session. Use /resume or configure a default agent.".to_string();
                state.status = message.clone();
                state.push_error(message);
            }
        } else {
            state.push_local_user_message(text.clone());
            state.start_assistant_typing();
            state.status = "Sending message...".to_string();
            output.effects.push(submit_message_effect(state, text));
        }
    }

    fn sync_composer_overlay(state: &mut AppState, output: &mut ReducerOutput) {
        if state.composer.draft().trim_start().starts_with('/') {
            if state.overlay.is_none() {
                state.open_command_picker();
            }
            state.sync_command_picker_to_draft(SLASH_COMMAND_SPECS);
            return;
        }

        if state.composer.current_skill_mention_query().is_some() {
            if !matches!(
                state.overlay,
                Some(crate::state::OverlayState::SkillMentionPicker { .. })
            ) {
                state.open_skill_mention_picker();
            }
            if !state.skills_loaded {
                state.status = "Loading skills...".to_string();
                output.effects.push(ShellEffect::ListSkillsForMention);
            }
            state.sync_skill_mention_picker_to_draft();
            return;
        }

        if matches!(
            state.overlay,
            Some(
                crate::state::OverlayState::CommandPicker { .. }
                    | crate::state::OverlayState::SkillMentionPicker { .. }
            )
        ) {
            state.clear_overlay();
        }
    }

    fn submit_message_effect(state: &mut AppState, message: String) -> ShellEffect {
        let stream_id = uuid::Uuid::new_v4().to_string();
        state.begin_stream(stream_id.clone());
        ShellEffect::SubmitMessage { message, stream_id }
    }

    fn slash_command_pending_status(command: &SlashCommand) -> &'static str {
        match command {
            SlashCommand::NewChat => "Starting new chat...",
            SlashCommand::ListSkills => "Loading skills...",
            SlashCommand::ListModels => "Loading providers...",
            SlashCommand::ListModelsForProvider { .. } => "Loading models...",
            SlashCommand::SwitchModel { .. } => "Switching model...",
            SlashCommand::SetDefaultModel { .. } => "Setting default model...",
            SlashCommand::ListSessions => "Loading sessions...",
            SlashCommand::Quit => "Exiting...",
            _ => "Running command...",
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{
            MESSAGE_SCROLL_PAGE_ROWS, MESSAGE_SCROLL_WHEEL_ROWS, ShellAction, ShellEffect, reduce,
        };
        use crate::keymap::Action;
        use crate::slash_command::SlashCommand;
        use crate::state::{
            AppState, InputMode, ModelPickerCategory, ModelPickerItem, PendingSessionState,
            SkillPickerItem,
        };
        use types::{ChatSession, ChatSessionSummary, Skill, SkillSource};
        use types::{ChatSessionEvent, StreamFrame};

        fn session_summary(id: &str, name: &str) -> ChatSessionSummary {
            ChatSessionSummary {
                id: id.to_string(),
                name: name.to_string(),
                agent_id: "agent-1".to_string(),
                provider: "provider".to_string(),
                model: "model".to_string(),
                skill_id: None,
                message_count: 1,
                updated_at: 1,
                last_message_preview: Some("preview".to_string()),
                archived_at: None,
            }
        }

        #[test]
        fn submit_plain_message_creates_send_effect() {
            let mut state = AppState::empty();
            state.composer.insert_char('h');
            state.composer.insert_char('i');

            let output = reduce(&mut state, ShellAction::Ui(Action::Submit));

            assert!(state.conversation_cells.is_empty());
            assert!(state.runtime_cells.is_empty());
            assert!(state.active_turn.is_none());
            assert!(matches!(
                output.actions.as_slice(),
                [ShellAction::SubmitText { text }] if text == "hi"
            ));
            assert!(output.effects.is_empty());
        }

        #[test]
        fn shift_tab_cycles_input_mode_without_overlay() {
            let mut state = AppState::empty();

            reduce(&mut state, ShellAction::Ui(Action::CycleInputMode));
            assert_eq!(state.input_mode, InputMode::Plan);
            reduce(&mut state, ShellAction::Ui(Action::CycleInputMode));
            assert_eq!(state.input_mode, InputMode::Chat);
        }

        #[test]
        fn submit_while_response_is_running_steers_current_session() {
            let mut state = AppState::empty();
            let session = ChatSession::new("agent-1".to_string(), "model".to_string());
            let session_id = session.id.clone();
            state.set_current_session(session);
            state.begin_stream("turn-1".to_string());
            state.push_local_user_message("first".to_string());
            for ch in "second".chars() {
                state.composer.insert_char(ch);
            }

            let output = reduce(&mut state, ShellAction::Ui(Action::Submit));

            assert!(state.composer.draft().is_empty());
            assert!(output.actions.is_empty());
            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::SteerMessage {
                    session_id: effect_session_id,
                    instruction
                }] if effect_session_id == &session_id && instruction == "second"
            ));
            assert_eq!(
                state.status,
                "Queued update for current response. Press Esc to interrupt."
            );
            let active_turn = state.active_turn.as_ref().expect("active turn");
            assert_eq!(state.runtime_cells[0].cell.body, "first");
            assert_eq!(active_turn.queued_updates, vec!["second"]);
        }

        #[test]
        fn submit_while_response_is_starting_keeps_draft_and_does_not_send() {
            let mut state = AppState::empty();
            state.begin_stream("turn-1".to_string());
            state.push_local_user_message("first".to_string());
            for ch in "second".chars() {
                state.composer.insert_char(ch);
            }

            let output = reduce(&mut state, ShellAction::Ui(Action::Submit));

            assert_eq!(state.composer.draft(), "second");
            assert!(output.actions.is_empty());
            assert!(output.effects.is_empty());
            assert_eq!(
                state.status,
                "Response is still starting. Press Esc to cancel before sending another message."
            );
            let active_turn = state.active_turn.as_ref().expect("active turn");
            assert!(active_turn.cells.is_empty());
            assert_eq!(state.runtime_cells[0].cell.body, "first");
        }

        #[test]
        fn submit_slash_command_creates_command_effect() {
            let mut state = AppState::empty();
            for ch in "/help".chars() {
                state.composer.insert_char(ch);
            }

            let output = reduce(&mut state, ShellAction::Ui(Action::Submit));

            assert!(matches!(
                output.actions.as_slice(),
                [ShellAction::SubmitText { text }] if text == "/help"
            ));
        }

        #[test]
        fn model_slash_command_sets_loading_status_before_effect() {
            let mut state = AppState::empty();

            let output = reduce(
                &mut state,
                ShellAction::SubmitText {
                    text: "/model".to_string(),
                },
            );

            assert_eq!(state.status, "Loading providers...");
            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::ExecuteSlashCommand(SlashCommand::ListModels)]
            ));
        }

        #[test]
        fn skill_slash_command_sets_loading_status_before_effect() {
            let mut state = AppState::empty();

            let output = reduce(
                &mut state,
                ShellAction::SubmitText {
                    text: "/skill".to_string(),
                },
            );

            assert_eq!(state.status, "Loading skills...");
            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::ExecuteSlashCommand(SlashCommand::ListSkills)]
            ));
        }

        #[test]
        fn skill_picker_loaded_opens_manager_overlay_without_history() {
            let mut state = AppState::empty();

            let output = reduce(
                &mut state,
                ShellAction::SkillPickerLoaded {
                    skills: vec![SkillPickerItem {
                        id: "team".to_string(),
                        name: "Team".to_string(),
                        description: Some("Coordinate subagents".to_string()),
                        source: SkillSource::System,
                        read_only: true,
                    }],
                    status: "View skills".to_string(),
                },
            );

            assert!(output.actions.is_empty());
            assert!(output.effects.is_empty());
            assert!(matches!(
                state.overlay,
                Some(crate::state::OverlayState::SkillManager { selected: 0 })
            ));
            assert!(state.conversation_cells.is_empty());
            assert!(state.runtime_cells.is_empty());
        }

        #[test]
        fn run_picker_loaded_opens_transient_overlay_without_history() {
            let mut state = AppState::empty();

            let output = reduce(
                &mut state,
                ShellAction::RunPickerLoaded {
                    runs: Vec::new(),
                    status: "Work picker opened.".to_string(),
                },
            );

            assert!(output.actions.is_empty());
            assert!(output.effects.is_empty());
            assert!(matches!(
                state.overlay,
                Some(crate::state::OverlayState::RunPicker { selected: 0 })
            ));
            assert!(state.conversation_cells.is_empty());
            assert!(state.runtime_cells.is_empty());
        }

        #[test]
        fn skill_detail_loaded_opens_transient_overlay_without_history() {
            let mut state = AppState::empty();
            let mut skill = Skill::new(
                "team".to_string(),
                "Team".to_string(),
                Some("Coordinate subagents".to_string()),
                None,
                "# Team".to_string(),
            );
            skill.source = SkillSource::System;
            skill.read_only = true;

            let output = reduce(
                &mut state,
                ShellAction::SkillDetailLoaded {
                    skill: Box::new(skill),
                    status: "Showing skill team".to_string(),
                },
            );

            assert!(output.actions.is_empty());
            assert!(output.effects.is_empty());
            assert!(matches!(
                state.overlay,
                Some(crate::state::OverlayState::SkillDetail)
            ));
            assert_eq!(
                state.selected_skill.as_ref().map(|skill| skill.id.as_str()),
                Some("team")
            );
            assert!(state.conversation_cells.is_empty());
            assert!(state.runtime_cells.is_empty());
        }

        #[test]
        fn submitting_skill_manager_activates_selected_skill() {
            let mut state = AppState::empty();
            state.skills = vec![SkillPickerItem {
                id: "team".to_string(),
                name: "Team".to_string(),
                description: Some("Coordinate subagents".to_string()),
                source: SkillSource::System,
                read_only: true,
            }];
            state.open_skill_manager();
            state.move_overlay_selection(1);

            let output = reduce(&mut state, ShellAction::Ui(Action::Submit));

            assert_eq!(state.status, "Loading skill...");
            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::ActivateOverlaySelection]
            ));
        }

        #[test]
        fn at_opens_skill_mention_picker_and_loads_skills() {
            let mut state = AppState::empty();

            let output = reduce(&mut state, ShellAction::Ui(Action::InputChar('@')));

            assert!(matches!(
                state.overlay,
                Some(crate::state::OverlayState::SkillMentionPicker { selected: 0 })
            ));
            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::ListSkillsForMention]
            ));
        }

        #[test]
        fn submitting_skill_mention_inserts_selected_skill_id() {
            let mut state = AppState::empty();
            state.skills = vec![SkillPickerItem {
                id: "team".to_string(),
                name: "Team".to_string(),
                description: Some("Coordinate subagents".to_string()),
                source: SkillSource::System,
                read_only: true,
            }];
            state.skills_loaded = true;
            state.composer.replace("use @tea");
            state.open_skill_mention_picker();

            let output = reduce(&mut state, ShellAction::Ui(Action::Submit));

            assert!(output.actions.is_empty());
            assert!(output.effects.is_empty());
            assert_eq!(state.composer.draft(), "use @team ");
            assert!(state.overlay.is_none());
        }

        #[test]
        fn slash_opens_command_picker() {
            let mut state = AppState::empty();

            let output = reduce(&mut state, ShellAction::Ui(Action::InputChar('/')));

            assert!(output.actions.is_empty());
            assert!(output.effects.is_empty());
            assert!(matches!(
                state.overlay,
                Some(crate::state::OverlayState::CommandPicker { selected: 0 })
            ));
        }

        #[test]
        fn command_picker_selection_moves_with_navigation() {
            let mut state = AppState::empty();
            state.composer.insert_char('/');
            state.open_command_picker();

            let output = reduce(&mut state, ShellAction::Ui(Action::NavDown));

            assert!(!output.should_quit);
            assert!(matches!(
                state.overlay,
                Some(crate::state::OverlayState::CommandPicker { selected: 1 })
            ));
        }

        #[test]
        fn page_scroll_updates_history_offset_without_overlay() {
            let mut state = AppState::empty();

            reduce(&mut state, ShellAction::Ui(Action::ScrollUp));
            assert_eq!(state.message_scroll_from_bottom, MESSAGE_SCROLL_PAGE_ROWS);

            reduce(&mut state, ShellAction::Ui(Action::ScrollDown));
            assert_eq!(state.message_scroll_from_bottom, 0);
        }

        #[test]
        fn wheel_scroll_uses_fine_grained_offset() {
            let mut state = AppState::empty();

            reduce(&mut state, ShellAction::Ui(Action::WheelUp));
            assert_eq!(state.message_scroll_from_bottom, MESSAGE_SCROLL_WHEEL_ROWS);

            reduce(&mut state, ShellAction::Ui(Action::WheelDown));
            assert_eq!(state.message_scroll_from_bottom, 0);
        }

        #[test]
        fn page_scroll_is_ignored_while_overlay_is_open() {
            let mut state = AppState::empty();
            state.open_command_picker();

            reduce(&mut state, ShellAction::Ui(Action::ScrollUp));

            assert_eq!(state.message_scroll_from_bottom, 0);
        }

        #[test]
        fn command_picker_tracks_typed_prefix() {
            let mut state = AppState::empty();

            for ch in "/daemon".chars() {
                reduce(&mut state, ShellAction::Ui(Action::InputChar(ch)));
            }

            assert_eq!(state.composer.draft(), "/daemon");
            assert!(matches!(
                state.overlay,
                Some(crate::state::OverlayState::CommandPicker { selected: 0 })
            ));
        }

        #[test]
        fn command_picker_moves_to_resume_when_typed() {
            let mut state = AppState::empty();

            for ch in "/resume".chars() {
                reduce(&mut state, ShellAction::Ui(Action::InputChar(ch)));
            }

            assert_eq!(state.composer.draft(), "/resume");
            assert!(matches!(
                state.overlay,
                Some(crate::state::OverlayState::CommandPicker { selected: 4 })
            ));
        }

        #[test]
        fn command_picker_submit_prefers_typed_alias_over_selected_item() {
            let mut state = AppState::empty();
            for ch in "/session".chars() {
                reduce(&mut state, ShellAction::Ui(Action::InputChar(ch)));
            }

            let output = reduce(&mut state, ShellAction::Ui(Action::Submit));

            assert!(state.overlay.is_none());
            assert!(state.composer.draft().is_empty());
            assert!(matches!(
                output.actions.as_slice(),
                [ShellAction::SubmitText { text }] if text == "/session"
            ));
            assert!(output.effects.is_empty());
        }

        #[test]
        fn command_picked_replaces_composer_draft() {
            let mut state = AppState::empty();
            state.composer.insert_char('/');
            state.open_command_picker();

            let output = reduce(
                &mut state,
                ShellAction::CommandPicked {
                    text: "/model ".to_string(),
                },
            );

            assert!(!output.should_quit);
            assert!(state.overlay.is_none());
            assert_eq!(state.composer.draft(), "/model ");
            assert_eq!(state.status, "Command selected");
        }

        #[test]
        fn open_daemon_picker_clears_command_draft() {
            let mut state = AppState::empty();
            state.composer.insert_char('/');
            state.open_command_picker();

            let output = reduce(&mut state, ShellAction::OpenDaemonPicker);

            assert!(!output.should_quit);
            assert_eq!(state.composer.draft(), "");
            assert!(matches!(
                state.overlay,
                Some(crate::state::OverlayState::DaemonPicker { selected: 0 })
            ));
            assert_eq!(state.status, "Select daemon action");
        }

        #[test]
        fn submitting_model_picker_shows_switching_status_before_effect() {
            let mut state = AppState::empty();
            state.open_model_picker("codex");

            let output = reduce(&mut state, ShellAction::Ui(Action::Submit));

            assert!(!output.should_quit);
            assert!(matches!(
                state.overlay,
                Some(crate::state::OverlayState::ModelPicker { .. })
            ));
            assert_eq!(state.status, "Switching model...");
            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::ActivateOverlaySelection]
            ));
        }

        #[test]
        fn submitting_provider_picker_shows_loading_status_before_effect() {
            let mut state = AppState::empty();
            state.open_provider_picker();

            let output = reduce(&mut state, ShellAction::Ui(Action::Submit));

            assert!(!output.should_quit);
            assert!(matches!(
                state.overlay,
                Some(crate::state::OverlayState::ProviderPicker { .. })
            ));
            assert_eq!(state.status, "Loading models...");
            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::ActivateOverlaySelection]
            ));
        }

        #[test]
        fn delete_selected_session_requires_confirmation() {
            let mut state = AppState::empty();
            state.sessions = vec![session_summary("session-1", "First")];
            state.open_session_picker();

            let output = reduce(&mut state, ShellAction::Ui(Action::DeleteSelected));

            assert!(output.effects.is_empty());
            assert_eq!(state.status, "Press d again to delete session First");
        }

        #[test]
        fn delete_selected_session_second_press_creates_effect() {
            let mut state = AppState::empty();
            state.sessions = vec![session_summary("session-1", "First")];
            state.open_session_picker();
            reduce(&mut state, ShellAction::Ui(Action::DeleteSelected));

            let output = reduce(&mut state, ShellAction::Ui(Action::DeleteSelected));

            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::DeleteSession { session_id }] if session_id == "session-1"
            ));
        }

        #[test]
        fn delete_selected_in_plain_composer_inserts_d() {
            let mut state = AppState::empty();

            let output = reduce(&mut state, ShellAction::Ui(Action::DeleteSelected));

            assert!(output.effects.is_empty());
            assert_eq!(state.composer.draft(), "d");
        }

        #[test]
        fn session_deleted_updates_picker_sessions() {
            let mut state = AppState::empty();
            state.sessions = vec![
                session_summary("session-1", "First"),
                session_summary("session-2", "Second"),
            ];
            state.open_session_picker();

            let output = reduce(
                &mut state,
                ShellAction::SessionDeleted {
                    session_id: "session-1".to_string(),
                    deleted: true,
                    sessions: vec![session_summary("session-2", "Second")],
                    status: "Deleted session session-1".to_string(),
                },
            );

            assert!(!output.should_quit);
            assert_eq!(state.sessions.len(), 1);
            assert_eq!(state.sessions[0].id, "session-2");
            assert_eq!(state.status, "Deleted session session-1");
            assert!(matches!(
                state.overlay,
                Some(crate::state::OverlayState::SessionPicker { selected: 0 })
            ));
        }

        #[test]
        fn invalid_slash_command_pushes_error() {
            let mut state = AppState::empty();
            for ch in "/run nope".chars() {
                state.composer.insert_char(ch);
            }

            let output = reduce(&mut state, ShellAction::Ui(Action::Submit));

            assert!(matches!(
                output.actions.as_slice(),
                [ShellAction::SubmitText { text }] if text == "/run nope"
            ));
            assert!(output.effects.is_empty());
            assert!(state.conversation_cells.is_empty());
            assert!(state.runtime_cells.is_empty());
            assert!(state.active_turn.is_none());
        }

        #[test]
        fn submit_text_routes_slash_command_through_parser() {
            let mut state = AppState::empty();
            let output = reduce(
                &mut state,
                ShellAction::SubmitText {
                    text: "/help".to_string(),
                },
            );

            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::ExecuteSlashCommand(SlashCommand::Help)]
            ));
        }

        #[test]
        fn submit_text_routes_quit_slash_command() {
            let mut state = AppState::empty();
            let output = reduce(
                &mut state,
                ShellAction::SubmitText {
                    text: "/quit".to_string(),
                },
            );

            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::ExecuteSlashCommand(SlashCommand::Quit)]
            ));
            assert!(!output.should_quit);
            assert_eq!(state.status, "Exiting...");
        }

        #[test]
        fn quit_action_exits_shell() {
            let mut state = AppState::empty();
            let output = reduce(&mut state, ShellAction::Quit);

            assert!(output.should_quit);
        }

        #[test]
        fn ctrl_c_cancels_active_stream_before_quitting() {
            let mut state = AppState::empty();
            state.is_streaming = true;
            state.current_stream_id = Some("stream-1".to_string());
            state.apply_stream_frame(StreamFrame::Ack {
                content: String::new(),
            });
            state.apply_stream_frame(StreamFrame::ToolCall {
                id: "call-1".to_string(),
                name: "bash".to_string(),
                arguments: serde_json::json!({"command": "sleep 20"}),
            });

            let output = reduce(&mut state, ShellAction::Ui(Action::Quit));

            assert!(!output.should_quit);
            assert!(!state.is_streaming);
            assert!(state.current_stream_id.is_none());
            assert!(state.active_turn.is_none());
            assert!(state.runtime_cells.iter().any(|entry| {
                entry.cell.title == "Info" && entry.cell.body.contains("Canceled current response")
            }));
            assert_eq!(state.status, "Canceling response...");
            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::CancelStream { stream_id }] if stream_id == "stream-1"
            ));
        }

        #[test]
        fn help_overlay_does_not_append_runtime_info() {
            let mut state = AppState::empty();

            let output = reduce(&mut state, ShellAction::OpenHelpOverlay);

            assert!(output.effects.is_empty());
            assert!(state.conversation_cells.is_empty());
            assert!(state.runtime_cells.is_empty());
            assert!(matches!(
                state.overlay,
                Some(crate::state::OverlayState::Help)
            ));
            assert_eq!(state.status, "Showing help");
        }

        #[test]
        fn submit_daemon_command_routes_to_daemon_picker() {
            let mut state = AppState::empty();
            let output = reduce(
                &mut state,
                ShellAction::SubmitText {
                    text: "/daemon".to_string(),
                },
            );

            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::ExecuteSlashCommand(SlashCommand::Daemon)]
            ));
        }

        #[test]
        fn submit_text_creates_send_effect_for_plain_message() {
            let mut state = AppState::empty();
            let session = ChatSession::new("agent-1".to_string(), "model".to_string());
            state.set_current_session(session);
            let output = reduce(
                &mut state,
                ShellAction::SubmitText {
                    text: "hi".to_string(),
                },
            );

            assert!(state.conversation_cells.is_empty());
            assert_eq!(state.runtime_cells.len(), 1);
            assert_eq!(state.runtime_cells[0].cell.body, "hi");
            let active_turn = state.active_turn.as_ref().expect("active turn");
            let active = active_turn.cells.last().expect("active assistant");
            assert!(active.is_active);
            assert!(
                active
                    .subtitle
                    .as_deref()
                    .is_some_and(|text| text.contains("typing"))
            );
            assert!(state.is_streaming);
            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::SubmitMessage { message, stream_id }] if message == "hi" && !stream_id.is_empty()
            ));
            match output.effects.as_slice() {
                [ShellEffect::SubmitMessage { stream_id, .. }] => {
                    assert_eq!(state.current_stream_id.as_deref(), Some(stream_id.as_str()));
                }
                _ => unreachable!("asserted submit effect above"),
            }
        }

        #[test]
        fn esc_after_submit_cancels_before_start_frame() {
            let mut state = AppState::empty();
            let session = ChatSession::new("agent-1".to_string(), "model".to_string());
            state.set_current_session(session);
            let output = reduce(
                &mut state,
                ShellAction::SubmitText {
                    text: "hi".to_string(),
                },
            );
            let stream_id = match output.effects.as_slice() {
                [ShellEffect::SubmitMessage { stream_id, .. }] => stream_id.clone(),
                _ => panic!("expected submit effect"),
            };

            let output = reduce(&mut state, ShellAction::Ui(Action::CloseOverlay));

            assert_eq!(state.status, "Canceling response...");
            assert!(!state.is_streaming);
            assert!(state.current_stream_id.is_none());
            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::CancelStream { stream_id: canceled }] if canceled == &stream_id
            ));
        }

        #[test]
        fn submit_text_without_session_creates_session_first() {
            let mut state = AppState::empty();
            state.set_default_agent(Some("agent-1".to_string()), Some("Agent".to_string()));

            let output = reduce(
                &mut state,
                ShellAction::SubmitText {
                    text: "hi".to_string(),
                },
            );

            let active_turn = state.active_turn.as_ref().expect("active turn");
            assert_eq!(state.runtime_cells[0].cell.body, "hi");
            assert!(active_turn.cells.last().is_some_and(|cell| cell.is_active));
            assert_eq!(state.status, "Creating session...");
            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::CreateSessionForSubmit { message }] if message == "hi"
            ));
        }

        #[test]
        fn session_created_for_submit_sends_pending_message() {
            let mut state = AppState::empty();
            let session = ChatSession::new("agent-1".to_string(), "model".to_string());
            let session_id = session.id.clone();

            let output = reduce(
                &mut state,
                ShellAction::SessionCreatedForSubmit {
                    session: Box::new(session),
                    runs: Vec::new(),
                    message: "hi".to_string(),
                },
            );

            assert_eq!(state.current_session_id(), Some(session_id.as_str()));
            let active_turn = state.active_turn.as_ref().expect("active turn");
            assert_eq!(state.runtime_cells[0].cell.body, "hi");
            assert!(active_turn.cells.last().is_some_and(|cell| cell.is_active));
            assert_eq!(state.status, "Sending message...");
            assert!(state.is_streaming);
            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::SubmitMessage { message, stream_id }] if message == "hi" && !stream_id.is_empty()
            ));
            match output.effects.as_slice() {
                [ShellEffect::SubmitMessage { stream_id, .. }] => {
                    assert_eq!(state.current_stream_id.as_deref(), Some(stream_id.as_str()));
                }
                _ => unreachable!("asserted submit effect above"),
            }
        }

        #[test]
        fn model_selection_without_session_updates_pending_session() {
            let mut state = AppState::empty();
            state.set_default_agent(Some("agent-1".to_string()), Some("Agent".to_string()));
            state.open_model_picker("codex");

            let output = reduce(
                &mut state,
                ShellAction::PendingSessionModelSelected {
                    provider: "codex".to_string(),
                    model: "gpt-5.4".to_string(),
                    model_name: "GPT-5.4".to_string(),
                    status: "Model selected for new chat.".to_string(),
                },
            );

            assert!(output.effects.is_empty());
            let pending = state.pending_session.as_ref().expect("pending session");
            assert_eq!(pending.agent_id, "agent-1");
            assert_eq!(pending.provider, "codex");
            assert_eq!(pending.model, "gpt-5.4");
            assert_eq!(pending.model_name, "GPT-5.4");
            assert!(state.overlay.is_none());
            assert_eq!(state.status, "Model selected for new chat.");
        }

        #[test]
        fn model_selection_without_default_agent_does_not_claim_success() {
            let mut state = AppState::empty();
            state.open_model_picker("codex");

            let output = reduce(
                &mut state,
                ShellAction::PendingSessionModelSelected {
                    provider: "codex".to_string(),
                    model: "gpt-5.4".to_string(),
                    model_name: "GPT-5.4".to_string(),
                    status: "Model selected for new chat.".to_string(),
                },
            );

            assert!(output.effects.is_empty());
            assert!(state.pending_session.is_none());
            assert!(state.overlay.is_some());
            assert_eq!(
                state.status,
                "No default agent is available. Start the daemon or send a message first."
            );
        }

        #[test]
        fn shift_enter_in_model_picker_sets_global_default_model() {
            let mut state = AppState::empty();
            state.model_items = vec![ModelPickerItem {
                provider: "codex".to_string(),
                model: "gpt-5.4".to_string(),
                name: "GPT-5.4".to_string(),
                category: ModelPickerCategory::Available,
                usage_count: 0,
                last_used_at: None,
                is_current: false,
                is_default: false,
            }];
            state.open_model_picker("codex");

            let output = reduce(&mut state, ShellAction::Ui(Action::SetSelectedDefaultModel));

            assert!(output.actions.is_empty());
            assert_eq!(state.status, "Setting default model...");
            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::ExecuteSlashCommand(SlashCommand::SetDefaultModel { model })]
                    if model == "gpt-5.4"
            ));
        }

        #[test]
        fn session_created_for_submit_clears_pending_session() {
            let mut state = AppState::empty();
            state.set_pending_session(Some(PendingSessionState::new(
                "agent-1".to_string(),
                "Agent".to_string(),
                "gpt-5.4".to_string(),
            )));
            let session = ChatSession::new("agent-1".to_string(), "gpt-5.4".to_string());

            let _ = reduce(
                &mut state,
                ShellAction::SessionCreatedForSubmit {
                    session: Box::new(session),
                    runs: Vec::new(),
                    message: "hi".to_string(),
                },
            );

            assert!(state.pending_session.is_none());
        }

        #[test]
        fn new_chat_started_clears_view_and_sets_pending_session() {
            let mut state = AppState::empty();
            state.set_default_agent(Some("agent-1".to_string()), Some("Agent".to_string()));
            let mut session = ChatSession::new("agent-1".to_string(), "gpt-5.4".to_string());
            session.add_message(types::ChatMessage::user("old"));
            state.set_current_session(session);
            state.push_local_user_message("pending".to_string());
            state.push_info("notice");
            state.open_command_picker();

            let output = reduce(
                &mut state,
                ShellAction::NewChatStarted {
                    status: "Started new chat".to_string(),
                },
            );

            assert!(output.effects.is_empty());
            assert!(state.current_session_id().is_none());
            assert!(state.conversation_cells.is_empty());
            assert!(state.runtime_cells.is_empty());
            assert!(state.active_turn.is_none());
            assert!(state.overlay.is_none());
            assert!(state.composer.draft().is_empty());
            let pending = state.pending_session.as_ref().expect("pending session");
            assert_eq!(pending.agent_id, "agent-1");
            assert_eq!(pending.model, "gpt-5.4");
            assert_eq!(state.status, "Started new chat");
        }

        #[test]
        fn esc_in_command_mode_clears_draft_instead_of_quitting() {
            let mut state = AppState::empty();
            for ch in "/help".chars() {
                state.composer.insert_char(ch);
            }

            let output = reduce(&mut state, ShellAction::Ui(Action::CloseOverlay));

            assert!(!output.should_quit);
            assert!(state.composer.draft().is_empty());
            assert_eq!(state.status, "Returned to message mode");
        }

        #[test]
        fn esc_in_compose_mode_clears_draft_instead_of_quitting() {
            let mut state = AppState::empty();
            for ch in "hello".chars() {
                state.composer.insert_char(ch);
            }

            let output = reduce(&mut state, ShellAction::Ui(Action::CloseOverlay));

            assert!(!output.should_quit);
            assert!(state.composer.draft().is_empty());
            assert_eq!(state.status, "Cleared input");
        }

        #[test]
        fn esc_with_empty_composer_does_not_quit() {
            let mut state = AppState::empty();

            let output = reduce(&mut state, ShellAction::Ui(Action::CloseOverlay));

            assert!(!output.should_quit);
            assert_eq!(state.status, "Input already empty. Press Ctrl-C to quit.");
        }

        #[test]
        fn esc_with_empty_composer_cancels_active_stream() {
            let mut state = AppState::empty();
            state.is_streaming = true;
            state.current_stream_id = Some("stream-1".to_string());
            state.apply_stream_frame(StreamFrame::Ack {
                content: "Partial".to_string(),
            });

            let output = reduce(&mut state, ShellAction::Ui(Action::CloseOverlay));

            assert!(!output.should_quit);
            assert!(!state.is_streaming);
            assert!(state.current_stream_id.is_none());
            assert!(state.active_turn.is_none());
            assert!(
                state
                    .runtime_cells
                    .iter()
                    .any(|entry| entry.cell.body.contains("Partial"))
            );
            assert!(state.runtime_cells.iter().any(|entry| {
                entry.cell.title == "Info" && entry.cell.body.contains("Canceled current response")
            }));
            assert_eq!(state.status, "Canceling response...");
            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::CancelStream { stream_id }] if stream_id == "stream-1"
            ));
        }

        #[test]
        fn canceled_stream_end_without_terminal_frame_does_not_show_error() {
            let mut state = AppState::empty();
            state.is_streaming = true;
            state.current_stream_id = Some("stream-1".to_string());
            state.apply_stream_frame(StreamFrame::Ack {
                content: "Partial".to_string(),
            });

            let cancel_output = reduce(&mut state, ShellAction::Ui(Action::CloseOverlay));
            assert!(matches!(
                cancel_output.effects.as_slice(),
                [ShellEffect::CancelStream { stream_id }] if stream_id == "stream-1"
            ));

            let output = reduce(
                &mut state,
                ShellAction::ChatStreamEndedWithoutTerminal {
                    stream_id: "stream-1".to_string(),
                },
            );

            assert!(!state.runtime_cells.iter().any(|entry| {
                entry.cell.title == "Error"
                    && entry
                        .cell
                        .body
                        .contains("Chat stream ended before a terminal frame")
            }));
            assert_eq!(state.status, "Canceling response...");
            assert!(output.effects.is_empty());
        }

        #[test]
        fn content_stream_end_without_terminal_frame_completes_without_error() {
            let mut state = AppState::empty();
            state.push_local_user_message("hello".to_string());
            state.apply_stream_frame(StreamFrame::Start {
                stream_id: "stream-1".to_string(),
            });
            state.apply_stream_frame(StreamFrame::Data {
                content: "done".to_string(),
            });

            let output = reduce(
                &mut state,
                ShellAction::ChatStreamEndedWithoutTerminal {
                    stream_id: "stream-1".to_string(),
                },
            );

            assert!(!state.is_streaming);
            assert!(state.current_stream_id.is_none());
            assert_eq!(state.status, "Stream finished");
            assert!(state.active_turn.is_none());
            assert!(state.runtime_cells.iter().any(|entry| {
                entry.cell.kind == crate::transcript::TranscriptCellKind::Assistant
                    && entry.cell.body == "done"
            }));
            assert!(!state.runtime_cells.iter().any(|entry| {
                entry.cell.title == "Error"
                    && entry
                        .cell
                        .body
                        .contains("Chat stream ended before a terminal frame")
            }));
            assert!(output.effects.is_empty());
        }

        #[test]
        fn empty_stream_end_without_terminal_frame_still_shows_error() {
            let mut state = AppState::empty();
            state.push_local_user_message("hello".to_string());
            state.apply_stream_frame(StreamFrame::Start {
                stream_id: "stream-1".to_string(),
            });

            reduce(
                &mut state,
                ShellAction::ChatStreamEndedWithoutTerminal {
                    stream_id: "stream-1".to_string(),
                },
            );

            assert!(state.runtime_cells.iter().any(|entry| {
                entry.cell.title == "Error"
                    && entry
                        .cell
                        .body
                        .contains("Chat stream ended before a terminal frame")
            }));
        }

        #[test]
        fn esc_clears_active_turn_even_without_stream_id() {
            let mut state = AppState::empty();
            state.push_local_user_message("run tool".to_string());
            state.apply_stream_frame(StreamFrame::ToolCall {
                id: "call-1".to_string(),
                name: "bash".to_string(),
                arguments: serde_json::json!({"cmd": "sleep 10"}),
            });

            let output = reduce(&mut state, ShellAction::Ui(Action::CloseOverlay));

            assert!(output.effects.is_empty());
            assert!(state.active_turn.is_none());
            assert!(state.runtime_cells.iter().any(|entry| {
                entry.cell.kind == crate::transcript::TranscriptCellKind::Tool
                    && entry.cell.tool_call_id() == Some("call-1")
            }));
            assert_eq!(state.status, "Canceled current response.");
        }

        #[test]
        fn esc_with_draft_cancels_active_stream_before_clearing_composer() {
            let mut state = AppState::empty();
            state.is_streaming = true;
            state.current_stream_id = Some("stream-1".to_string());
            state.composer.replace("draft");
            state.apply_stream_frame(StreamFrame::Ack {
                content: "Partial".to_string(),
            });

            let output = reduce(&mut state, ShellAction::Ui(Action::CloseOverlay));

            assert_eq!(state.composer.draft(), "draft");
            assert_eq!(state.status, "Canceling response...");
            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::CancelStream { stream_id }] if stream_id == "stream-1"
            ));
        }

        #[test]
        fn esc_with_overlay_cancels_active_stream_before_closing_overlay() {
            let mut state = AppState::empty();
            state.is_streaming = true;
            state.current_stream_id = Some("stream-1".to_string());
            state.open_command_picker();
            state.apply_stream_frame(StreamFrame::Ack {
                content: "Partial".to_string(),
            });

            let output = reduce(&mut state, ShellAction::Ui(Action::CloseOverlay));

            assert!(state.overlay.is_some());
            assert_eq!(state.status, "Canceling response...");
            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::CancelStream { stream_id }] if stream_id == "stream-1"
            ));
        }

        #[test]
        fn esc_closes_overlay_and_clears_command_draft() {
            let mut state = AppState::empty();
            state.composer.replace("/daemon");
            state.open_daemon_picker();

            let output = reduce(&mut state, ShellAction::Ui(Action::CloseOverlay));

            assert!(!output.should_quit);
            assert!(state.overlay.is_none());
            assert_eq!(state.composer.draft(), "");
            assert_eq!(state.status, "Connecting to daemon...");
        }

        #[test]
        fn message_added_event_does_not_reload_idle_current_session() {
            let mut state = AppState::empty();
            let session = ChatSession::new("agent-1".to_string(), "model".to_string());
            let session_id = session.id.clone();
            state.set_current_session(session);

            let output = reduce(
                &mut state,
                ShellAction::SessionEvent(ChatSessionEvent::MessageAdded {
                    session_id,
                    source: "ipc".to_string(),
                }),
            );

            assert!(output.effects.is_empty());
        }

        #[test]
        fn message_added_event_reloads_current_session_when_active_turn_is_visible() {
            let mut state = AppState::empty();
            let session = ChatSession::new("agent-1".to_string(), "model".to_string());
            let session_id = session.id.clone();
            state.set_current_session(session);
            state.push_local_user_message("run a team".to_string());
            state.apply_stream_frame(StreamFrame::ToolCall {
                id: "call-1".to_string(),
                name: "spawn_subagent_batch".to_string(),
                arguments: serde_json::json!({"specs":[{"task":"reply"}]}),
            });

            let output = reduce(
                &mut state,
                ShellAction::SessionEvent(ChatSessionEvent::MessageAdded {
                    session_id,
                    source: "ipc".to_string(),
                }),
            );

            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::ReloadCurrentSession]
            ));
        }

        #[test]
        fn message_added_event_reloads_when_visible_tool_turn_lost_session_anchor() {
            let mut state = AppState::empty();
            state.push_local_user_message("run a team".to_string());
            state.apply_stream_frame(StreamFrame::ToolCall {
                id: "call-1".to_string(),
                name: "spawn_subagent_batch".to_string(),
                arguments: serde_json::json!({"specs":[{"task":"reply"}]}),
            });

            let output = reduce(
                &mut state,
                ShellAction::SessionEvent(ChatSessionEvent::MessageAdded {
                    session_id: "session-1".to_string(),
                    source: "ipc".to_string(),
                }),
            );

            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::ReloadCurrentSession]
            ));
        }

        #[test]
        fn stream_tool_result_reloads_when_visible_tool_turn_lost_session_anchor() {
            let mut state = AppState::empty();
            state.push_local_user_message("run a team".to_string());
            reduce(
                &mut state,
                ShellAction::StreamFrame(StreamFrame::ToolCall {
                    id: "call-1".to_string(),
                    name: "spawn_subagent_batch".to_string(),
                    arguments: serde_json::json!({"specs":[{"task":"reply"}]}),
                }),
            );

            let output = reduce(
                &mut state,
                ShellAction::StreamFrame(StreamFrame::ToolResult {
                    id: "call-1".to_string(),
                    result: "completed".to_string(),
                    success: true,
                }),
            );

            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::ReloadCurrentSession]
            ));
        }

        #[test]
        fn stream_done_reloads_current_session_to_reconcile_pending_user() {
            let mut state = AppState::empty();
            let session = ChatSession::new("agent-1".to_string(), "model".to_string());
            state.set_current_session(session);
            state.push_local_user_message("hi".to_string());
            state.start_assistant_typing();

            let output = reduce(
                &mut state,
                ShellAction::StreamFrame(StreamFrame::Done { total_tokens: None }),
            );

            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::ReloadCurrentSession]
            ));
        }

        #[test]
        fn refresh_tick_reloads_current_session_while_active_turn_waits_for_persistence() {
            let mut state = AppState::empty();
            let session = ChatSession::new("agent-1".to_string(), "model".to_string());
            state.set_current_session(session);
            state.push_local_user_message("hi".to_string());
            state.apply_stream_frame(StreamFrame::Ack {
                content: "done".to_string(),
            });
            state.apply_stream_frame(StreamFrame::Done { total_tokens: None });

            assert!(!state.is_streaming);
            assert!(state.active_turn.is_none());
            assert!(state.pending_runtime_refresh_session_id().is_some());
            assert!(!state.runtime_cells.is_empty());

            let output = reduce(&mut state, ShellAction::RefreshTick);

            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::ReloadCurrentSession]
            ));
        }

        #[test]
        fn refresh_tick_uses_active_turn_session_when_thread_session_is_missing() {
            let mut state = AppState::empty();
            let session = ChatSession::new("agent-1".to_string(), "model".to_string());
            state.set_current_session(session);
            state.push_local_user_message("hi".to_string());
            state.apply_stream_frame(StreamFrame::Ack {
                content: "done".to_string(),
            });
            state.apply_stream_frame(StreamFrame::Done { total_tokens: None });
            state.thread.clear_session();

            assert!(state.pending_runtime_refresh_session_id().is_some());
            let output = reduce(&mut state, ShellAction::RefreshTick);

            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::ReloadCurrentSession]
            ));
        }

        #[test]
        fn refresh_tick_reloads_visible_tool_turn_without_session_anchor() {
            let mut state = AppState::empty();
            state.push_local_user_message("run a team".to_string());
            state.apply_stream_frame(StreamFrame::ToolCall {
                id: "call-1".to_string(),
                name: "spawn_subagent_batch".to_string(),
                arguments: serde_json::json!({"specs":[{"task":"reply"}]}),
            });

            let output = reduce(&mut state, ShellAction::RefreshTick);

            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::ReloadCurrentSession]
            ));
        }

        #[test]
        fn active_reload_miss_does_not_clear_visible_turn() {
            let mut state = AppState::empty();
            let session = ChatSession::new("agent-1".to_string(), "model".to_string());
            let session_id = session.id.clone();
            state.set_current_session(session);
            state.push_local_user_message("edit a file".to_string());
            state.apply_stream_frame(StreamFrame::ToolCall {
                id: "call-1".to_string(),
                name: "edit".to_string(),
                arguments: serde_json::json!({"file_path":"check.txt"}),
            });

            let output = reduce(
                &mut state,
                ShellAction::CurrentSessionReloaded {
                    session: None,
                    runs: Vec::new(),
                },
            );

            assert!(output.effects.is_empty());
            assert_eq!(state.current_session_id(), Some(session_id.as_str()));
            assert!(state.active_turn.is_some());
            assert!(!state.conversation_cells.iter().any(|cell| {
                cell.body
                    .contains("The active session is no longer available")
            }));
        }

        #[test]
        fn stream_error_reloads_current_session_to_reconcile_pending_user() {
            let mut state = AppState::empty();
            let session = ChatSession::new("agent-1".to_string(), "model".to_string());
            state.set_current_session(session);
            state.push_local_user_message("hi".to_string());
            state.start_assistant_typing();

            let output = reduce(
                &mut state,
                ShellAction::StreamFrame(StreamFrame::error(500, "failed")),
            );

            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::ReloadCurrentSession]
            ));
        }

        #[test]
        fn startup_submit_triggers_start_daemon_effect() {
            let mut state = AppState::empty();
            state.enter_startup(Some("agent-1".to_string()), Some("session-1".to_string()));

            for ch in "/daemon start".chars() {
                state.composer.insert_char(ch);
            }

            let output = reduce(&mut state, ShellAction::Ui(Action::Submit));

            assert!(matches!(
                output.actions.as_slice(),
                [ShellAction::SubmitText { text }] if text == "/daemon start"
            ));
        }

        #[test]
        fn resize_clears_screen_without_changing_status() {
            let mut state = AppState::empty();
            state.status = "Connected to daemon".to_string();

            let output = reduce(&mut state, ShellAction::Ui(Action::Resize));

            assert_eq!(state.status, "Connected to daemon");
            assert!(matches!(
                output.effects.as_slice(),
                [ShellEffect::ClearScreen]
            ));
        }

        #[test]
        fn plain_text_when_daemon_offline_pushes_start_hint() {
            let mut state = AppState::empty();
            state.enter_startup(None, None);

            let output = reduce(
                &mut state,
                ShellAction::SubmitText {
                    text: "hello".to_string(),
                },
            );

            assert!(output.effects.is_empty());
            assert_eq!(state.runtime_cells.len(), 1);
            assert!(state.runtime_cells[0].cell.body.contains("/daemon"));
        }

        #[test]
        fn daemon_stopped_enters_startup_mode_and_records_notice() {
            let mut state = AppState::empty();
            let session = ChatSession::new("agent-1".to_string(), "model".to_string());
            let session_id = session.id.clone();
            state.set_default_agent(Some("agent-1".to_string()), Some("Agent".to_string()));
            state.set_current_session(session);

            let output = reduce(
                &mut state,
                ShellAction::DaemonStopped {
                    status: "RestFlow daemon stopped".to_string(),
                },
            );

            assert!(!output.should_quit);
            assert!(state.is_startup_mode());
            assert_eq!(state.status, "RestFlow daemon stopped");
            assert_eq!(
                state
                    .startup_state()
                    .and_then(|startup| startup.agent_override.as_deref()),
                Some("agent-1")
            );
            assert_eq!(
                state
                    .startup_state()
                    .and_then(|startup| startup.session_override.as_deref()),
                Some(session_id.as_str())
            );
            assert_eq!(state.runtime_cells.len(), 1);
            assert!(state.runtime_cells[0].cell.body.contains("daemon stopped"));
        }

        #[test]
        fn error_clears_active_typing_cell() {
            let mut state = AppState::empty();
            state.start_assistant_typing();
            assert!(state.active_turn.is_some());

            let output = reduce(
                &mut state,
                ShellAction::Error("Failed to connect to daemon. Is it running?".to_string()),
            );

            assert!(output.effects.is_empty());
            assert!(state.active_turn.is_none());
            assert!(!state.is_streaming);
            assert_eq!(state.status, "Failed to connect to daemon. Is it running?");
            assert!(
                state
                    .runtime_cells
                    .iter()
                    .any(|entry| entry.cell.body.contains("Failed to connect to daemon"))
            );
        }

        #[test]
        fn paste_inserts_text_without_submitting() {
            let mut state = AppState::empty();

            let output = reduce(
                &mut state,
                ShellAction::Ui(Action::Paste("hello\nworld".to_string())),
            );

            assert_eq!(state.composer.draft(), "hello\nworld");
            assert!(output.actions.is_empty());
            assert!(output.effects.is_empty());
        }
    }
}

mod render {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::Style;
    use ratatui::text::Line;
    use ratatui::text::Span;
    use ratatui::widgets::{Block, Borders, Paragraph, Widget, Wrap};

    const COMPOSER_BORDER_HEIGHT: u16 = 2;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) struct ShellBottomViewport {
        pub lines: Vec<Line<'static>>,
        pub cursor_x: u16,
        pub cursor_y: u16,
        pub prompt_top: u16,
    }

    pub(crate) fn render_shell_bottom_viewport(
        width: u16,
        transient_lines: Vec<Line<'static>>,
        prompt_lines: &[Line<'static>],
        prompt_cursor_column: u16,
        prompt_cursor_row: u16,
        footer: &str,
    ) -> ShellBottomViewport {
        let prompt_height = prompt_lines.len() as u16 + COMPOSER_BORDER_HEIGHT;
        let transient_height = transient_lines.len() as u16;
        let height = (transient_height + prompt_height).max(1);
        let area = Rect::new(0, 0, width, height);
        let mut buffer = Buffer::empty(area);

        for (row, line) in transient_lines.into_iter().enumerate() {
            if row as u16 >= transient_height {
                break;
            }
            line.render(Rect::new(0, row as u16, width, 1), &mut buffer);
        }

        let prompt_area = Rect::new(0, transient_height, width, prompt_height);
        let footer_title = truncate_display_width(footer, width.saturating_sub(4));
        let block = Block::default()
            .borders(Borders::ALL)
            .title_bottom(format!(" {footer_title} "));
        Paragraph::new(prompt_lines.to_vec())
            .block(block)
            .wrap(Wrap { trim: false })
            .render(prompt_area, &mut buffer);

        ShellBottomViewport {
            lines: buffer_to_styled_lines(&buffer),
            cursor_x: (1 + prompt_cursor_column).min(width.saturating_sub(2)),
            cursor_y: transient_height + 1 + prompt_cursor_row,
            prompt_top: transient_height,
        }
    }

    fn truncate_display_width(value: &str, width: u16) -> String {
        let width = width as usize;
        let mut out = String::new();
        let mut current = 0usize;
        for ch in value.chars() {
            let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if current + ch_width > width {
                break;
            }
            out.push(ch);
            current += ch_width;
        }
        out
    }

    fn buffer_to_styled_lines(buffer: &Buffer) -> Vec<Line<'static>> {
        let mut lines = Vec::with_capacity(buffer.area.height as usize);
        for y in 0..buffer.area.height {
            let mut spans: Vec<Span<'static>> = Vec::new();
            let mut x = 0;
            let last_content_x = last_content_cell_x(buffer, y);
            if last_content_x.is_none() {
                lines.push(Line::from(""));
                continue;
            }
            while x < buffer.area.width {
                if let Some(cell) = buffer.cell((x, y))
                    && !cell.skip
                {
                    if last_content_x.is_some_and(|last| x > last) {
                        break;
                    }
                    let symbol = cell.symbol();
                    push_symbol(&mut spans, symbol, cell.style());
                    x += unicode_width::UnicodeWidthStr::width(symbol).max(1) as u16;
                    continue;
                }
                x += 1;
            }
            lines.push(Line::from(spans));
        }
        lines
    }

    fn last_content_cell_x(buffer: &Buffer, y: u16) -> Option<u16> {
        let mut last = None;
        for x in 0..buffer.area.width {
            if let Some(cell) = buffer.cell((x, y))
                && !cell.skip
                && !cell.symbol().trim_end_matches(' ').is_empty()
            {
                last = Some(x);
            }
        }
        last
    }

    fn push_symbol(spans: &mut Vec<Span<'static>>, symbol: &str, style: Style) {
        if let Some(last) = spans.last_mut()
            && last.style == style
        {
            last.content.to_mut().push_str(symbol);
            return;
        }
        spans.push(Span::styled(symbol.to_string(), style));
    }

    #[cfg(test)]
    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    #[cfg(test)]
    mod tests {
        use ratatui::style::{Color, Style};
        use ratatui::text::Line;

        use super::line_text;
        use super::render_shell_bottom_viewport;

        #[test]
        fn bottom_viewport_keeps_footer_and_borders() {
            let prompt_lines = vec![Line::from("hello")];
            let rendered =
                render_shell_bottom_viewport(20, Vec::new(), &prompt_lines, 0, 0, "openai · gpt-5");

            assert_eq!(rendered.lines.len(), 3);
            assert!(line_text(&rendered.lines[0]).starts_with('┌'));
            assert!(line_text(&rendered.lines[1]).starts_with('│'));
            assert!(line_text(&rendered.lines[2]).starts_with('└'));
            assert!(line_text(&rendered.lines[2]).contains("openai"));
        }

        #[test]
        fn bottom_viewport_preserves_cjk_without_spacer_cells() {
            let prompt_lines = vec![Line::from("帮我打开浏览器")];
            let rendered =
                render_shell_bottom_viewport(32, Vec::new(), &prompt_lines, 0, 0, "openai · gpt-5");

            assert!(line_text(&rendered.lines[1]).contains("帮我打开浏览器"));
            assert!(!line_text(&rendered.lines[1]).contains("帮 我"));
        }

        #[test]
        fn bottom_viewport_places_cursor_after_transient_lines() {
            let prompt_lines = vec![Line::from("hello")];
            let rendered = render_shell_bottom_viewport(
                24,
                vec![Line::from("Agent · typing…")],
                &prompt_lines,
                2,
                0,
                "model",
            );

            assert_eq!(rendered.cursor_x, 3);
            assert_eq!(rendered.cursor_y, 2);
        }

        #[test]
        fn bottom_viewport_preserves_line_style() {
            let prompt_lines = vec![Line::from("hello")];
            let rendered = render_shell_bottom_viewport(
                24,
                vec![Line::from("Agent").style(Style::default().fg(Color::Yellow))],
                &prompt_lines,
                0,
                0,
                "model",
            );

            assert_eq!(rendered.lines[0].spans[0].style.fg, Some(Color::Yellow));
        }

        #[test]
        fn bottom_viewport_does_not_emit_full_width_blank_rows() {
            let prompt_lines = vec![Line::from("hello")];
            let rendered = render_shell_bottom_viewport(
                24,
                vec![Line::from(""), Line::from("message")],
                &prompt_lines,
                0,
                0,
                "model",
            );

            assert_eq!(line_text(&rendered.lines[0]), "");
            assert_eq!(line_text(&rendered.lines[1]), "message");
        }
    }
}

mod scrollback {
    use std::fmt;
    use std::io::{ErrorKind, Result as IoResult, Write};

    use crossterm::Command;
    use crossterm::cursor::{MoveTo, MoveToColumn};
    use crossterm::queue;
    use crossterm::style::{
        Attribute, Color as CrosstermColor, Colors, Print, SetAttribute, SetBackgroundColor,
        SetColors, SetForegroundColor,
    };
    use crossterm::terminal::{Clear, ClearType};
    use ratatui::style::Modifier;
    use ratatui::text::{Line, Span};
    use unicode_width::UnicodeWidthChar;

    use crate::transcript::TranscriptCell;

    #[derive(Default)]
    pub struct ScrollbackWriter {
        committed_cells: Vec<TranscriptCell>,
        pending_lines: Vec<Line<'static>>,
    }

    impl ScrollbackWriter {
        pub fn reset(&mut self) {
            self.committed_cells.clear();
            self.pending_lines.clear();
        }

        pub fn is_prefix_of(&self, cells: &[TranscriptCell]) -> bool {
            is_cell_prefix(&self.committed_cells, cells)
        }

        pub fn sync_history<F>(&mut self, cells: &[TranscriptCell], width: u16, mut render_lines: F)
        where
            F: FnMut(&[TranscriptCell], u16) -> Vec<Line<'static>>,
        {
            if !self.is_prefix_of(cells) {
                self.reset();
            } else {
                // Clear stale pending lines when content changed but prefix still matches.
                // This prevents ghost lines accumulating when runtime_cells are updated
                // (e.g. tool results replaced, model switched) between sync cycles.
                self.pending_lines.clear();
            }

            let new_cells = &cells[self.committed_cells.len()..];
            let lines = render_lines(new_cells, width);
            self.pending_lines = lines;
            self.committed_cells = cells.to_vec();
        }

        pub fn replace_committed_without_append(&mut self, cells: &[TranscriptCell]) {
            self.pending_lines.clear();
            self.committed_cells = cells.to_vec();
        }

        pub fn discard_pending(&mut self) {
            self.pending_lines.clear();
        }

        pub fn insert_pending(
            &mut self,
            writer: &mut impl Write,
            viewport_top: u16,
            width: u16,
        ) -> IoResult<bool> {
            if self.pending_lines.is_empty() || viewport_top == 0 {
                return Ok(false);
            }
            let lines = std::mem::take(&mut self.pending_lines);
            match insert_history_lines(writer, viewport_top, width, &lines) {
                Ok(()) => Ok(true),
                Err(error) if error.kind() == ErrorKind::Unsupported => {
                    self.reset();
                    Ok(false)
                }
                Err(error) => Err(error),
            }
        }
    }

    pub fn insert_history_lines(
        writer: &mut impl Write,
        viewport_top: u16,
        width: u16,
        lines: &[Line<'static>],
    ) -> IoResult<()> {
        if lines.is_empty() || viewport_top == 0 {
            return Ok(());
        }

        queue!(
            writer,
            SetScrollRegion(1..viewport_top),
            MoveTo(0, viewport_top.saturating_sub(1))
        )?;
        for line in lines {
            queue!(
                writer,
                Print("\r\n"),
                MoveToColumn(0),
                SetForegroundColor(CrosstermColor::Reset),
                SetBackgroundColor(CrosstermColor::Reset),
                SetAttribute(Attribute::Reset),
                Clear(ClearType::CurrentLine),
            )?;
            write_styled_line(writer, &truncate_line_to_width(line, width))?;
        }
        queue!(writer, ResetScrollRegion)?;
        Ok(())
    }

    fn is_cell_prefix(previous: &[TranscriptCell], current: &[TranscriptCell]) -> bool {
        previous.len() <= current.len()
            && previous
                .iter()
                .zip(current.iter())
                .all(|(left, right)| left == right)
    }

    fn truncate_line_to_width(line: &Line<'static>, width: u16) -> Line<'static> {
        let width = width as usize;
        let mut spans = Vec::new();
        let mut current_width = 0usize;

        for span in &line.spans {
            for ch in span.content.chars() {
                let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
                if current_width + ch_width > width {
                    return line_from_spans(spans, line.style, line.alignment);
                }
                push_char_span(&mut spans, ch, span.style);
                current_width += ch_width;
            }
        }

        line_from_spans(spans, line.style, line.alignment)
    }

    fn write_styled_line(writer: &mut impl Write, line: &Line<'static>) -> IoResult<()> {
        for span in &line.spans {
            let style = if line.style == ratatui::style::Style::default() {
                span.style
            } else {
                span.style.patch(line.style)
            };
            queue!(
                writer,
                SetAttribute(Attribute::Reset),
                SetColors(Colors::new(
                    style
                        .fg
                        .map(std::convert::Into::into)
                        .unwrap_or(CrosstermColor::Reset),
                    style
                        .bg
                        .map(std::convert::Into::into)
                        .unwrap_or(CrosstermColor::Reset),
                ))
            )?;
            queue_modifiers(writer, style.add_modifier - style.sub_modifier)?;
            queue!(writer, Print(span.content.clone()))?;
        }
        queue!(
            writer,
            SetForegroundColor(CrosstermColor::Reset),
            SetBackgroundColor(CrosstermColor::Reset),
            SetAttribute(Attribute::Reset),
        )
    }

    fn queue_modifiers(writer: &mut impl Write, modifiers: Modifier) -> IoResult<()> {
        if modifiers.contains(Modifier::BOLD) {
            queue!(writer, SetAttribute(Attribute::Bold))?;
        }
        if modifiers.contains(Modifier::DIM) {
            queue!(writer, SetAttribute(Attribute::Dim))?;
        }
        if modifiers.contains(Modifier::ITALIC) {
            queue!(writer, SetAttribute(Attribute::Italic))?;
        }
        if modifiers.contains(Modifier::UNDERLINED) {
            queue!(writer, SetAttribute(Attribute::Underlined))?;
        }
        if modifiers.contains(Modifier::REVERSED) {
            queue!(writer, SetAttribute(Attribute::Reverse))?;
        }
        if modifiers.contains(Modifier::CROSSED_OUT) {
            queue!(writer, SetAttribute(Attribute::CrossedOut))?;
        }
        if modifiers.contains(Modifier::SLOW_BLINK) {
            queue!(writer, SetAttribute(Attribute::SlowBlink))?;
        }
        if modifiers.contains(Modifier::RAPID_BLINK) {
            queue!(writer, SetAttribute(Attribute::RapidBlink))?;
        }
        Ok(())
    }

    fn line_from_spans(
        spans: Vec<Span<'static>>,
        style: ratatui::style::Style,
        alignment: Option<ratatui::layout::Alignment>,
    ) -> Line<'static> {
        let mut line = Line::from(spans);
        line.style = style;
        line.alignment = alignment;
        line
    }

    fn push_char_span(spans: &mut Vec<Span<'static>>, ch: char, style: ratatui::style::Style) {
        if let Some(last) = spans.last_mut()
            && last.style == style
        {
            last.content.to_mut().push(ch);
            return;
        }
        spans.push(Span::styled(ch.to_string(), style));
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct SetScrollRegion(std::ops::Range<u16>);

    impl Command for SetScrollRegion {
        fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
            write!(f, "\x1b[{};{}r", self.0.start, self.0.end)
        }

        #[cfg(windows)]
        fn execute_winapi(&self) -> std::io::Result<()> {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "SetScrollRegion requires ANSI support",
            ))
        }

        #[cfg(windows)]
        fn is_ansi_code_supported(&self) -> bool {
            true
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct ResetScrollRegion;

    impl Command for ResetScrollRegion {
        fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
            write!(f, "\x1b[r")
        }

        #[cfg(windows)]
        fn execute_winapi(&self) -> std::io::Result<()> {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "ResetScrollRegion requires ANSI support",
            ))
        }

        #[cfg(windows)]
        fn is_ansi_code_supported(&self) -> bool {
            true
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{ScrollbackWriter, insert_history_lines};
        use crate::transcript::{MessageGroup, TranscriptCell, TranscriptCellKind};
        use ratatui::text::Line;
        use std::io::{Error, ErrorKind, Result as IoResult, Write};

        fn user_cell(body: &str) -> TranscriptCell {
            TranscriptCell {
                kind: TranscriptCellKind::User,
                title: "You".to_string(),
                subtitle: None,
                body: body.to_string(),
                group: MessageGroup::Conversation,
                is_active: false,
            }
        }

        #[test]
        fn sync_history_appends_only_new_cells() {
            let mut scrollback = ScrollbackWriter::default();
            let cells = vec![user_cell("one")];

            scrollback.sync_history(&cells, 80, |new_cells, _| {
                new_cells
                    .iter()
                    .map(|cell| Line::from(cell.body.clone()))
                    .collect()
            });
            assert_eq!(scrollback.pending_lines.len(), 1);

            scrollback.sync_history(&cells, 80, |new_cells, _| {
                new_cells
                    .iter()
                    .map(|cell| Line::from(cell.body.clone()))
                    .collect()
            });
            // Re-syncing the same cells produces no new pending lines since
            // the committed cells already cover the full content.
            assert_eq!(scrollback.pending_lines.len(), 0);

            let cells = vec![user_cell("one"), user_cell("two")];
            scrollback.sync_history(&cells, 80, |new_cells, _| {
                new_cells
                    .iter()
                    .map(|cell| Line::from(cell.body.clone()))
                    .collect()
            });
            // Only the new cell ("two") produces pending lines since "one" is already committed.
            assert_eq!(scrollback.pending_lines.len(), 1);
        }

        #[test]
        fn sync_history_resets_when_prefix_changes() {
            let mut scrollback = ScrollbackWriter::default();
            scrollback.sync_history(&[user_cell("one")], 80, |new_cells, _| {
                new_cells
                    .iter()
                    .map(|cell| Line::from(cell.body.clone()))
                    .collect()
            });

            scrollback.sync_history(&[user_cell("other")], 80, |new_cells, _| {
                new_cells
                    .iter()
                    .map(|cell| Line::from(cell.body.clone()))
                    .collect()
            });

            let rendered = scrollback
                .pending_lines
                .iter()
                .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
                .collect::<String>();
            assert!(rendered.contains("other"));
        }

        #[test]
        fn replace_committed_without_append_marks_history_but_leaves_no_pending_lines() {
            let mut scrollback = ScrollbackWriter::default();
            let cells = vec![user_cell("loaded session")];

            scrollback.replace_committed_without_append(&cells);

            assert!(scrollback.pending_lines.is_empty());
            assert!(scrollback.is_prefix_of(&cells));
        }

        #[test]
        fn discard_pending_keeps_committed_history_without_inserting_lines() {
            let mut scrollback = ScrollbackWriter::default();
            let cells = vec![user_cell("loaded session")];

            scrollback.sync_history(&cells, 80, |new_cells, _| {
                new_cells
                    .iter()
                    .map(|cell| Line::from(cell.body.clone()))
                    .collect()
            });
            assert_eq!(scrollback.pending_lines.len(), 1);

            scrollback.discard_pending();

            assert!(scrollback.pending_lines.is_empty());
            assert!(scrollback.is_prefix_of(&cells));
        }

        #[test]
        fn insert_history_uses_region_above_retained_viewport() {
            let mut output = Vec::new();
            insert_history_lines(&mut output, 5, 40, &[Line::from("history line")])
                .expect("history insertion should render");
            let ansi = String::from_utf8(output).expect("history insertion should be utf8");

            assert!(ansi.contains("\u{1b}[1;5r"));
            assert!(ansi.contains("history line"));
            assert!(ansi.contains("\u{1b}[r"));
        }

        #[test]
        fn insert_pending_reports_whether_lines_were_inserted() {
            let mut scrollback = ScrollbackWriter::default();
            let mut output = Vec::new();

            let inserted = scrollback
                .insert_pending(&mut output, 5, 40)
                .expect("empty insert should be valid");
            assert!(!inserted);
            assert!(output.is_empty());

            scrollback.sync_history(&[user_cell("one")], 80, |new_cells, _| {
                new_cells
                    .iter()
                    .map(|cell| Line::from(cell.body.clone()))
                    .collect()
            });
            let inserted = scrollback
                .insert_pending(&mut output, 5, 40)
                .expect("pending insert should be valid");

            assert!(inserted);
            assert!(scrollback.pending_lines.is_empty());
            assert!(String::from_utf8(output).unwrap().contains("one"));
        }

        #[test]
        fn insert_pending_falls_back_when_terminal_insert_is_unsupported() {
            struct UnsupportedWriter;

            impl Write for UnsupportedWriter {
                fn write(&mut self, _buf: &[u8]) -> IoResult<usize> {
                    Err(Error::new(ErrorKind::Unsupported, "unsupported terminal"))
                }

                fn flush(&mut self) -> IoResult<()> {
                    Ok(())
                }
            }

            let mut scrollback = ScrollbackWriter::default();
            scrollback.sync_history(&[user_cell("one")], 80, |new_cells, _| {
                new_cells
                    .iter()
                    .map(|cell| Line::from(cell.body.clone()))
                    .collect()
            });

            let inserted = scrollback
                .insert_pending(&mut UnsupportedWriter, 5, 40)
                .expect("unsupported terminal should fall back to full redraw");

            assert!(!inserted);
            assert!(scrollback.pending_lines.is_empty());
            assert!(scrollback.committed_cells.is_empty());
        }
    }
}

mod visual_timeline {
    use std::collections::HashSet;

    use types::{ChatTurnEventKind, ChatTurnStatus};

    use crate::state::AppState;
    use crate::transcript::{MessageGroup, TranscriptCell, TranscriptCellKind};

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct VisualTimeline {
        scrollback_cells: Vec<TranscriptCell>,
        live_cells: Vec<TranscriptCell>,
    }

    impl VisualTimeline {
        pub fn from_state(state: &AppState) -> Self {
            let scrollback_cells = build_scrollback_cells(state);
            let live_cells = build_live_message_cells(state);
            debug_assert!(
                !has_cross_band_duplicate(&scrollback_cells, &live_cells),
                "a visual cell cannot be owned by both scrollback and live bands"
            );

            Self {
                scrollback_cells,
                live_cells,
            }
        }

        pub fn scrollback_cells(&self) -> &[TranscriptCell] {
            &self.scrollback_cells
        }

        pub fn live_cells(&self) -> &[TranscriptCell] {
            &self.live_cells
        }

        #[cfg(test)]
        pub fn render_cells(&self) -> Vec<TranscriptCell> {
            let mut cells = Vec::with_capacity(
                self.scrollback_cells
                    .len()
                    .saturating_add(self.live_cells.len()),
            );
            cells.extend(self.scrollback_cells.iter().cloned());
            cells.extend(self.live_cells.iter().cloned());
            cells
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    enum VisualKey {
        ToolCall(String),
        SubagentCall(String),
    }

    fn build_scrollback_cells(state: &AppState) -> Vec<TranscriptCell> {
        let mut cells =
            Vec::with_capacity(state.conversation_cells.len() + state.runtime_cells.len());
        let mut runtime = state.runtime_cells.iter().peekable();
        let conversation_limit =
            projected_running_turn_start_index(state).unwrap_or(state.conversation_cells.len());
        for (index, cell) in state
            .conversation_cells
            .iter()
            .take(conversation_limit)
            .enumerate()
        {
            if runtime.peek().is_some_and(|entry| {
                entry.base_cell_index == index && entry.cell.kind != TranscriptCellKind::User
            }) && cell.kind == TranscriptCellKind::User
            {
                cells.push(cell.clone());
                while let Some(entry) = runtime.peek() {
                    if entry.base_cell_index == index {
                        push_runtime_cell_if_visible(state, &mut cells, &entry.cell);
                        runtime.next();
                    } else {
                        break;
                    }
                }
                continue;
            }

            while let Some(entry) = runtime.peek() {
                if entry.base_cell_index == index {
                    push_runtime_cell_if_visible(state, &mut cells, &entry.cell);
                    runtime.next();
                } else {
                    break;
                }
            }

            cells.push(cell.clone());
        }

        for entry in runtime {
            push_runtime_cell_if_visible(state, &mut cells, &entry.cell);
        }
        cells
    }

    fn build_live_message_cells(state: &AppState) -> Vec<TranscriptCell> {
        let active_turn = state.active_turn.as_ref();
        let subagent_activity_cells = state.activity.subagent_live_cells();
        let has_assistant_cell = active_turn.is_some_and(|active_turn| {
            active_turn.cells.iter().any(|cell| {
                cell.kind == TranscriptCellKind::Assistant
                    && (cell.is_active || !cell.body.trim().is_empty())
            })
        });
        let has_runtime_cell = active_turn.is_some_and(|active_turn| {
            active_turn.cells.iter().any(|cell| {
                matches!(
                    cell.kind,
                    TranscriptCellKind::Tool | TranscriptCellKind::Subagent
                )
            })
        });
        let queued_updates_empty = active_turn.is_none_or(|turn| turn.queued_updates.is_empty());
        if !state.is_streaming
            && !has_assistant_cell
            && !has_runtime_cell
            && queued_updates_empty
            && subagent_activity_cells.is_empty()
        {
            return Vec::new();
        }

        let mut cells = Vec::new();
        if let Some(active_turn) = active_turn {
            for active_cell in &active_turn.cells {
                if active_cell.is_active {
                    cells.push(active_cell.clone());
                }
            }
            if let Some(cell) = queued_update_notice_cell(&active_turn.queued_updates) {
                cells.push(cell);
            }
        }
        cells.extend(subagent_activity_cells);
        cells
    }

    fn push_runtime_cell_if_visible(
        state: &AppState,
        cells: &mut Vec<TranscriptCell>,
        cell: &TranscriptCell,
    ) {
        if !should_hide_runtime_cell(state, cell) {
            cells.push(cell.clone());
        }
    }

    fn should_hide_runtime_cell(state: &AppState, cell: &TranscriptCell) -> bool {
        if !state.is_streaming || cell.kind != TranscriptCellKind::Subagent {
            return false;
        }
        cell.tool_call_id()
            .is_some_and(|call_id| state.activity.has_subagent_activity_for(call_id))
    }

    fn visual_key(cell: &TranscriptCell) -> Option<VisualKey> {
        match cell.kind {
            TranscriptCellKind::Tool => cell
                .tool_call_id()
                .map(|call_id| VisualKey::ToolCall(call_id.to_string())),
            TranscriptCellKind::Subagent => cell
                .tool_call_id()
                .map(|call_id| VisualKey::SubagentCall(call_id.to_string())),
            _ => None,
        }
    }

    fn has_cross_band_duplicate(
        scrollback_cells: &[TranscriptCell],
        live_cells: &[TranscriptCell],
    ) -> bool {
        let scrollback_keys = scrollback_cells
            .iter()
            .filter_map(visual_key)
            .collect::<HashSet<_>>();
        live_cells
            .iter()
            .filter_map(visual_key)
            .any(|key| scrollback_keys.contains(&key))
    }

    fn projected_running_turn_start_index(state: &AppState) -> Option<usize> {
        if !state.is_streaming && state.active_turn.is_none() {
            return None;
        }
        let current_user = state
            .active_turn_runtime_cells()?
            .iter()
            .rev()
            .find(|entry| entry.cell.kind == TranscriptCellKind::User)
            .map(|entry| entry.cell.body.trim_end())?;
        let session = state.thread.session.as_ref()?;
        let has_running_projection = session.turns.last().is_some_and(|turn| {
            turn.status == ChatTurnStatus::Running && turn.events.iter().any(|event| {
                matches!(
                    &event.kind,
                    ChatTurnEventKind::UserMessage { content } if content.trim_end() == current_user
                )
            })
        }) || session.messages.last().is_some_and(|message| {
            message.role == types::ChatRole::User && message.content.trim_end() == current_user
        });
        if !has_running_projection {
            return None;
        }
        state.conversation_cells.iter().rposition(|cell| {
            cell.kind == TranscriptCellKind::User && cell.body.trim_end() == current_user
        })
    }

    fn queued_update_notice_cell(queued_updates: &[String]) -> Option<TranscriptCell> {
        if queued_updates.is_empty() {
            return None;
        }
        let body = queued_updates
            .iter()
            .enumerate()
            .map(|(index, update)| {
                let update = update.split_whitespace().collect::<Vec<_>>().join(" ");
                format!("{}. {}", index + 1, update)
            })
            .collect::<Vec<_>>()
            .join("\n");
        Some(TranscriptCell {
            kind: TranscriptCellKind::Notice,
            title: if queued_updates.len() == 1 {
                "Queued update".to_string()
            } else {
                "Queued updates".to_string()
            },
            subtitle: Some("waiting".to_string()),
            body,
            group: MessageGroup::RuntimeNotice,
            is_active: false,
        })
    }

    #[cfg(test)]
    mod tests {
        use super::VisualTimeline;
        use crate::state::{AnchoredRuntimeCell, AppState};
        use crate::transcript::{MessageGroup, TranscriptCell, TranscriptCellKind};
        use types::StreamFrame;

        fn subagent_cell(call_id: &str, body: &str) -> TranscriptCell {
            TranscriptCell {
                kind: TranscriptCellKind::Subagent,
                title: "Subagent".to_string(),
                subtitle: Some(format!("#{call_id}")),
                body: body.to_string(),
                group: MessageGroup::RuntimeNotice,
                is_active: false,
            }
        }

        #[test]
        fn subagent_activity_hides_only_matching_runtime_cell() {
            let mut state = AppState::empty();
            state.is_streaming = true;
            state.runtime_cells.push(AnchoredRuntimeCell {
                base_cell_index: 0,
                cell: subagent_cell("done-call", "done"),
            });
            state.runtime_cells.push(AnchoredRuntimeCell {
                base_cell_index: 0,
                cell: subagent_cell("running-call", "running"),
            });
            state
                .activity
                .record_tool_call("running-call", "spawn_subagent", "running");

            let timeline = VisualTimeline::from_state(&state);
            let scrollback = timeline.scrollback_cells();

            assert!(
                scrollback
                    .iter()
                    .any(|cell| cell.tool_call_id() == Some("done-call"))
            );
            assert!(
                !scrollback
                    .iter()
                    .any(|cell| cell.tool_call_id() == Some("running-call"))
            );
            assert!(
                timeline
                    .live_cells()
                    .iter()
                    .any(|cell| cell.title == "Subagents")
            );
        }

        #[test]
        fn completed_tool_moves_to_scrollback_while_assistant_stays_live() {
            let mut state = AppState::empty();
            state.begin_stream("turn-1".to_string());
            state.push_local_user_message("inspect workspace".to_string());
            state.apply_stream_frame(StreamFrame::ToolCall {
                id: "call-1".to_string(),
                name: "bash".to_string(),
                arguments: serde_json::json!({"command": "pwd"}),
            });

            let running = VisualTimeline::from_state(&state);
            assert!(
                running
                    .live_cells()
                    .iter()
                    .any(|cell| cell.tool_call_id() == Some("call-1"))
            );
            assert!(
                !running
                    .scrollback_cells()
                    .iter()
                    .any(|cell| cell.tool_call_id() == Some("call-1"))
            );

            state.apply_stream_frame(StreamFrame::ToolResult {
                id: "call-1".to_string(),
                result: "{\"stdout\":\"/tmp\"}".to_string(),
                success: true,
            });
            state.apply_stream_frame(StreamFrame::Data {
                content: "Done.".to_string(),
            });

            let completed = VisualTimeline::from_state(&state);
            assert!(
                completed
                    .scrollback_cells()
                    .iter()
                    .any(|cell| cell.tool_call_id() == Some("call-1"))
            );
            assert!(
                !completed
                    .live_cells()
                    .iter()
                    .any(|cell| cell.tool_call_id() == Some("call-1"))
            );
            assert!(completed.live_cells().iter().any(|cell| {
                cell.kind == TranscriptCellKind::Assistant && cell.body.contains("Done.")
            }));
        }
    }
}

mod shell {
    use std::io::{Result as IoResult, Stdout, Write};
    use std::path::Path;

    use crossterm::cursor::MoveTo;
    use crossterm::queue;
    use crossterm::style::{
        Attribute, Color as CrosstermColor, Colors, Print, SetAttribute, SetBackgroundColor,
        SetColors, SetForegroundColor,
    };
    use crossterm::terminal::{self, Clear, ClearType};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use serde_json::Value;
    use types::SkillSource;

    use crate::render::render_shell_bottom_viewport;
    use crate::scrollback::ScrollbackWriter;
    use crate::slash_command::{HELP_TEXT, SLASH_COMMAND_SPECS};
    use crate::state::{AppState, WorkPickerItem, work_run_kind_label};
    use crate::transcript::{MessageGroup, TranscriptCell, TranscriptCellKind};

    const CONTINUATION_PREFIX: &str = "  ";
    const TOOL_SUMMARY_LIMIT: usize = 120;
    const PROMPT_MIN_VISIBLE_ROWS: u16 = 1;
    const PROMPT_MAX_VISIBLE_ROWS: u16 = 6;
    const OVERLAY_MAX_ROWS: u16 = 10;

    fn overlay_capacity_for_state(state: &AppState, available_above_prompt: u16) -> u16 {
        if matches!(
            state.overlay,
            Some(
                crate::state::OverlayState::SessionPicker { .. }
                    | crate::state::OverlayState::RunPicker { .. }
                    | crate::state::OverlayState::SkillManager { .. }
                    | crate::state::OverlayState::SkillDetail
                    | crate::state::OverlayState::ProviderPicker { .. }
                    | crate::state::OverlayState::ModelPicker { .. }
                    | crate::state::OverlayState::Help
            )
        ) {
            available_above_prompt
        } else {
            available_above_prompt.min(OVERLAY_MAX_ROWS)
        }
    }

    pub struct ShellRenderer {
        stdout: Stdout,
        scrollback: ScrollbackWriter,
        last_viewport: Option<ViewportSnapshot>,
        last_terminal_size: Option<(u16, u16)>,
        last_message_line_count: Option<usize>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct ViewportSnapshot {
        top: u16,
        prompt_top: u16,
        lines: Vec<Line<'static>>,
        cursor_x: u16,
        cursor_y: u16,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct PromptSnapshot {
        lines: Vec<Line<'static>>,
        cursor_column: u16,
        cursor_row: u16,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct TerminalViewport {
        size: (u16, u16),
        snapshot: ViewportSnapshot,
    }

    impl ShellRenderer {
        pub fn new() -> Self {
            Self {
                stdout: std::io::stdout(),
                scrollback: ScrollbackWriter::default(),
                last_viewport: None,
                last_terminal_size: None,
                last_message_line_count: None,
            }
        }

        pub fn clear_screen(&mut self) -> IoResult<()> {
            queue_clear_visible(&mut self.stdout)?;
            self.scrollback.reset();
            self.last_viewport = None;
            self.last_terminal_size = None;
            self.last_message_line_count = None;
            self.stdout.flush()
        }

        pub fn purge_screen(&mut self) -> IoResult<()> {
            queue_purge_visible_and_scrollback(&mut self.stdout)?;
            self.scrollback.reset();
            self.last_viewport = None;
            self.last_terminal_size = None;
            self.last_message_line_count = None;
            self.stdout.flush()
        }

        pub fn sync(&mut self, state: &mut AppState) -> IoResult<()> {
            let size = normalize_terminal_size(terminal::size().unwrap_or((80, 24)));
            self.preserve_scrolled_message_anchor(state, size);
            let terminal_viewport = TerminalViewport::build(state, size);
            let viewport = terminal_viewport.snapshot;
            let stable_cells = build_stable_history_cells(state);
            let mut force_full_redraw = false;
            let mut append_stable_history = true;
            if !self.scrollback.is_prefix_of(&stable_cells) {
                self.scrollback
                    .replace_committed_without_append(&stable_cells);
                self.last_viewport = None;
                self.last_message_line_count = None;
                queue_clear_visible(&mut self.stdout)?;
                force_full_redraw = true;
                append_stable_history = false;
            }

            if append_stable_history {
                self.scrollback
                    .sync_history(&stable_cells, size.0, render_history_append_lines);
            }

            let needs_full_redraw = force_full_redraw || self.needs_full_redraw(size, &viewport);
            if needs_full_redraw {
                let clear_from = self
                    .last_viewport
                    .as_ref()
                    .map(|previous| previous.top.min(viewport.top))
                    .unwrap_or(viewport.top);
                self.clear_rows_from(clear_from, size.1, size.0)?;
                if append_stable_history {
                    self.scrollback.discard_pending();
                }
                self.redraw_history_tail(history_redraw_top(&viewport), size.0, &stable_cells)?;
                self.redraw_viewport_full(&viewport, size.0)?;
            } else {
                let protected_top = self.protected_scrollback_top(&viewport);
                let inserted =
                    self.scrollback
                        .insert_pending(&mut self.stdout, protected_top, size.0)?;
                if !inserted {
                    self.redraw_history_tail(protected_top, size.0, &stable_cells)?;
                }
                if should_force_live_viewport_redraw(state) {
                    self.redraw_viewport_full(&viewport, size.0)?;
                } else {
                    match self.last_viewport.clone() {
                        Some(previous) if previous == viewport => {
                            self.restore_cursor(&viewport)?;
                        }
                        Some(previous) => {
                            self.redraw_viewport_diff(&previous, &viewport, size.0)?;
                        }
                        None => {
                            self.redraw_viewport_full(&viewport, size.0)?;
                        }
                    }
                }
            }

            self.last_viewport = Some(viewport);
            self.last_terminal_size = Some(terminal_viewport.size);
            self.stdout.flush()
        }

        pub fn sync_viewport_only(&mut self, state: &mut AppState) -> IoResult<()> {
            let size = normalize_terminal_size(terminal::size().unwrap_or((80, 24)));
            self.preserve_scrolled_message_anchor(state, size);
            let terminal_viewport = TerminalViewport::build(state, size);
            let viewport = terminal_viewport.snapshot;
            let stable_cells = build_stable_history_cells(state);

            if !self.scrollback.is_prefix_of(&stable_cells) {
                return self.sync(state);
            }

            let can_update_viewport_only = self.last_terminal_size == Some(size)
                && self.last_viewport.as_ref().is_some_and(|previous| {
                    previous.top == viewport.top
                        && previous.prompt_top == viewport.prompt_top
                        && previous.lines.len() == viewport.lines.len()
                });
            if !can_update_viewport_only {
                return self.sync(state);
            }

            self.scrollback
                .sync_history(&stable_cells, size.0, render_history_append_lines);
            let protected_top = self.protected_scrollback_top(&viewport);
            let inserted =
                self.scrollback
                    .insert_pending(&mut self.stdout, protected_top, size.0)?;
            if !inserted {
                self.redraw_history_tail(protected_top, size.0, &stable_cells)?;
            }

            if should_force_live_viewport_redraw(state) {
                self.redraw_viewport_full(&viewport, size.0)?;
            } else {
                match self.last_viewport.clone() {
                    Some(previous) if previous == viewport => {
                        self.restore_cursor(&viewport)?;
                    }
                    Some(previous) => {
                        self.redraw_viewport_diff(&previous, &viewport, size.0)?;
                    }
                    None => {
                        self.redraw_viewport_full(&viewport, size.0)?;
                    }
                }
            }

            self.last_viewport = Some(viewport);
            self.last_terminal_size = Some(terminal_viewport.size);
            self.stdout.flush()
        }

        fn needs_full_redraw(&self, size: (u16, u16), viewport: &ViewportSnapshot) -> bool {
            self.last_terminal_size != Some(size)
                || self.last_viewport.as_ref().is_none_or(|previous| {
                    previous.top != viewport.top
                        || previous.prompt_top != viewport.prompt_top
                        || previous.lines.len() != viewport.lines.len()
                })
        }

        fn protected_scrollback_top(&self, viewport: &ViewportSnapshot) -> u16 {
            self.last_viewport
                .as_ref()
                .map(|previous| {
                    viewport
                        .top
                        .min(previous.top)
                        .min(previous.prompt_top)
                        .min(viewport.prompt_top)
                })
                .unwrap_or(viewport.top.min(viewport.prompt_top))
        }

        fn preserve_scrolled_message_anchor(&mut self, state: &mut AppState, size: (u16, u16)) {
            let message_line_count = message_layout_line_count(state, size);
            if let Some(previous_count) = self.last_message_line_count
                && state.message_scroll_from_bottom > 0
            {
                state.message_scroll_from_bottom = preserve_scrolled_offset(
                    previous_count,
                    message_line_count,
                    state.message_scroll_from_bottom,
                );
            }
            self.last_message_line_count = Some(message_line_count);
        }

        fn clear_rows_from(&mut self, start_row: u16, height: u16, width: u16) -> IoResult<()> {
            for row in start_row..height {
                self.write_row(row, &Line::from(""), width)?;
            }
            Ok(())
        }

        fn redraw_history_tail(
            &mut self,
            viewport_top: u16,
            width: u16,
            stable_cells: &[TranscriptCell],
        ) -> IoResult<()> {
            if viewport_top == 0 || stable_cells.is_empty() {
                return Ok(());
            }
            let visible = visible_history_tail_lines(stable_cells, width, viewport_top as usize);
            for row in 0..viewport_top {
                let empty = Line::from("");
                let line = visible.get(row as usize).unwrap_or(&empty);
                self.write_row(row, line, width)?;
            }
            Ok(())
        }

        fn redraw_viewport_full(
            &mut self,
            viewport: &ViewportSnapshot,
            width: u16,
        ) -> IoResult<()> {
            for (offset, line) in viewport.lines.iter().enumerate() {
                self.write_row(viewport.top + offset as u16, line, width)?;
            }
            self.restore_cursor(viewport)
        }

        fn redraw_viewport_diff(
            &mut self,
            previous: &ViewportSnapshot,
            viewport: &ViewportSnapshot,
            width: u16,
        ) -> IoResult<()> {
            for row in changed_row_indices(&previous.lines, &viewport.lines) {
                self.write_row(viewport.top + row as u16, &viewport.lines[row], width)?;
            }
            self.restore_cursor(viewport)
        }

        fn restore_cursor(&mut self, viewport: &ViewportSnapshot) -> IoResult<()> {
            queue!(self.stdout, MoveTo(viewport.cursor_x, viewport.cursor_y))
        }

        fn write_row(&mut self, row: u16, line: &Line<'static>, width: u16) -> IoResult<()> {
            queue!(
                self.stdout,
                MoveTo(0, row),
                SetForegroundColor(CrosstermColor::Reset),
                SetBackgroundColor(CrosstermColor::Reset),
                SetAttribute(Attribute::Reset),
                Clear(ClearType::CurrentLine),
            )?;
            write_styled_line(&mut self.stdout, &truncate_line_to_width(line, width))
        }
    }

    impl TerminalViewport {
        fn build(state: &AppState, size: (u16, u16)) -> Self {
            let (width, height) = size;
            let prompt = build_prompt_snapshot(state, width, height);
            let prompt_height = prompt.lines.len() as u16 + 2;
            let available_above_prompt = height.saturating_sub(prompt_height);
            let overlay_capacity = overlay_capacity_for_state(state, available_above_prompt);
            let overlay_lines =
                build_overlay_lines(state, width, overlay_capacity).unwrap_or_default();
            let overlay_height = overlay_lines.len() as u16;
            let available_above_prompt = available_above_prompt.saturating_sub(overlay_height);
            let spacer_height = u16::from(available_above_prompt > 0);
            let message_height = available_above_prompt.saturating_sub(spacer_height);
            let mut visible_message_lines = build_visible_message_lines(
                state,
                width,
                message_height,
                state.message_scroll_from_bottom,
            );
            if spacer_height > 0 && !visible_message_lines.is_empty() {
                visible_message_lines.push(Line::from(""));
            }
            visible_message_lines.extend(overlay_lines);
            let rendered = render_shell_bottom_viewport(
                width,
                visible_message_lines,
                &prompt.lines,
                prompt.cursor_column,
                prompt.cursor_row,
                &footer_status_line(state),
            );
            let top = height.saturating_sub(rendered.lines.len() as u16);

            Self {
                size,
                snapshot: ViewportSnapshot {
                    top,
                    prompt_top: top + rendered.prompt_top,
                    lines: rendered.lines,
                    cursor_x: rendered.cursor_x.min(width.saturating_sub(1)),
                    cursor_y: (top + rendered.cursor_y).min(height.saturating_sub(1)),
                },
            }
        }
    }

    #[cfg(test)]
    fn build_viewport_snapshot(state: &AppState, size: (u16, u16)) -> ViewportSnapshot {
        TerminalViewport::build(state, size).snapshot
    }

    fn build_prompt_snapshot(state: &AppState, width: u16, height: u16) -> PromptSnapshot {
        let content_width = prompt_content_width(width);
        let max_visible_rows = height
            .saturating_sub(2)
            .clamp(PROMPT_MIN_VISIBLE_ROWS, PROMPT_MAX_VISIBLE_ROWS);
        let show_placeholder = state.composer.is_blank();
        let visible_rows = if show_placeholder {
            1
        } else {
            state
                .composer
                .visible_row_count(content_width)
                .clamp(PROMPT_MIN_VISIBLE_ROWS, max_visible_rows)
        };

        let lines = if show_placeholder {
            vec![placeholder_line(state.input_mode, content_width)]
        } else {
            state
                .composer
                .visible_lines(content_width, visible_rows)
                .into_iter()
                .map(Line::from)
                .collect()
        };

        let (cursor_column, cursor_row) = if show_placeholder {
            (0, 0)
        } else {
            state.composer.cursor_position(content_width, visible_rows)
        };

        PromptSnapshot {
            lines,
            cursor_column,
            cursor_row,
        }
    }

    fn message_layout_line_count(state: &AppState, size: (u16, u16)) -> usize {
        let (width, _) = size;
        build_scrollable_message_lines(state, width).len()
    }

    fn build_stable_history_cells(state: &AppState) -> Vec<TranscriptCell> {
        crate::visual_timeline::VisualTimeline::from_state(state)
            .scrollback_cells()
            .to_vec()
    }

    fn visible_history_tail_lines(
        stable_cells: &[TranscriptCell],
        width: u16,
        height: usize,
    ) -> Vec<Line<'static>> {
        let history_lines = render_history_append_lines(stable_cells, width);
        bottom_anchor_lines(history_lines, height, 0)
    }

    fn history_redraw_top(viewport: &ViewportSnapshot) -> u16 {
        viewport.top.min(viewport.prompt_top)
    }

    fn build_active_message_lines(state: &AppState, width: u16) -> Vec<Line<'static>> {
        build_cell_lines(&build_live_message_cells(state), width)
    }

    fn build_scrollable_message_lines(state: &AppState, width: u16) -> Vec<Line<'static>> {
        build_active_message_lines(state, width)
    }

    fn build_visible_message_lines(
        state: &AppState,
        width: u16,
        max_rows: u16,
        scroll_from_bottom: usize,
    ) -> Vec<Line<'static>> {
        if max_rows == 0 {
            return Vec::new();
        }

        let height = max_rows as usize;
        tail_lines(
            build_scrollable_message_lines(state, width),
            height,
            scroll_from_bottom,
        )
    }

    #[cfg(test)]
    fn build_message_lines(state: &AppState, width: u16, max_rows: u16) -> Vec<Line<'static>> {
        build_visible_message_lines(state, width, max_rows, 0)
    }

    fn build_live_message_cells(state: &AppState) -> Vec<TranscriptCell> {
        crate::visual_timeline::VisualTimeline::from_state(state)
            .live_cells()
            .to_vec()
    }

    fn should_force_live_viewport_redraw(state: &AppState) -> bool {
        state.active_turn.is_some() || state.active_turn_runtime_start.is_some()
    }

    fn build_overlay_lines(
        state: &AppState,
        width: u16,
        max_rows: u16,
    ) -> Option<Vec<Line<'static>>> {
        if let Some(lines) = build_session_picker_lines(state, width, max_rows) {
            return Some(lines);
        }

        if let Some(lines) = build_run_picker_lines(state, width, max_rows) {
            return Some(lines);
        }

        if let Some(lines) = build_skill_mention_picker_lines(state, width, max_rows) {
            return Some(lines);
        }

        if let Some(lines) = build_skill_manager_lines(state, width, max_rows) {
            return Some(lines);
        }

        if let Some(lines) = build_skill_detail_lines(state, width, max_rows) {
            return Some(lines);
        }

        if let Some(lines) = build_provider_picker_lines(state, width, max_rows) {
            return Some(lines);
        }

        if let Some(lines) = build_model_picker_lines(state, width, max_rows) {
            return Some(lines);
        }

        if let Some(lines) = build_daemon_picker_lines(state, width) {
            return Some(lines);
        }

        if let Some(lines) = build_command_picker_lines(state, width, max_rows) {
            return Some(lines);
        }

        if let Some(lines) = build_help_overlay_lines(state, width, max_rows) {
            return Some(lines);
        }

        None
    }

    #[cfg(test)]
    fn build_transient_lines(state: &AppState, width: u16, max_rows: u16) -> Vec<Line<'static>> {
        if max_rows == 0 {
            return Vec::new();
        }

        if let Some(lines) = build_overlay_lines(state, width, max_rows) {
            return lines;
        }

        if state.active_turn.is_none() && state.active_turn_runtime_start.is_none() {
            return Vec::new();
        }
        let pending_lines = Vec::new();
        let live_cells = build_live_message_cells(state);
        let mut active_lines = build_cell_lines(&live_cells, width);
        if live_cells.len() > 1 && active_lines.len() >= max_rows as usize {
            active_lines = live_cells
                .last()
                .map(|cell| build_cell_lines(std::slice::from_ref(cell), width))
                .unwrap_or_default();
        }
        preserve_live_turn_lines(
            pending_lines,
            active_lines,
            max_rows as usize,
            stable_history_has_rendered_lines(state),
        )
    }

    fn build_session_picker_lines(
        state: &AppState,
        width: u16,
        max_rows: u16,
    ) -> Option<Vec<Line<'static>>> {
        let Some(crate::state::OverlayState::SessionPicker { selected }) = state.overlay.as_ref()
        else {
            return None;
        };

        let mut lines = Vec::new();
        lines.push(Line::from(vec![
            Span::styled("Resume session", tool_title_style()),
            Span::styled(
                "  Up/Down select, Enter resume, d delete, Esc close",
                muted_style(),
            ),
        ]));
        if state.sessions.is_empty() {
            lines.push(styled_line("  No sessions to resume yet.", muted_style()));
            return Some(lines);
        }

        let visible_capacity = (max_rows as usize).saturating_sub(1).max(1);
        let rows_per_session = 3usize;
        let visible_sessions = (visible_capacity / rows_per_session).max(1);
        let selected_index = (*selected).min(state.sessions.len().saturating_sub(1));
        let start = selected_index
            .saturating_sub(visible_sessions / 2)
            .min(state.sessions.len().saturating_sub(visible_sessions));
        let end = (start + visible_sessions).min(state.sessions.len());

        for (index, session) in state.sessions[start..end].iter().enumerate() {
            let index = start + index;
            let is_selected = index == selected_index;
            let marker = if is_selected { "› " } else { "  " };
            let title_style = if is_selected {
                tool_title_style()
            } else {
                Style::default().add_modifier(Modifier::BOLD)
            };
            let preview = session
                .last_message_preview
                .as_deref()
                .map(compact_session_preview)
                .filter(|preview| !preview.is_empty())
                .unwrap_or_else(|| "No messages yet".to_string());
            let mut title_spans = vec![
                Span::styled(
                    marker,
                    if is_selected {
                        tool_title_style()
                    } else {
                        muted_style()
                    },
                ),
                Span::styled(session.name.clone(), title_style),
                Span::styled(
                    session_message_count_label(session.message_count),
                    muted_style(),
                ),
            ];
            if is_selected && state.is_session_delete_pending(&session.id) {
                title_spans.push(Span::styled(" · press d again to delete", error_style()));
            }
            let title = Line::from(title_spans);
            lines.extend(wrap_styled_line(title, width));
            let detail = Line::from(vec![
                Span::styled("    Last: ", muted_style()),
                Span::styled(preview, muted_style()),
            ]);
            lines.extend(wrap_styled_line(detail, width));
            let id_line = Line::from(vec![
                Span::styled("    id: ", muted_style()),
                Span::styled(session.id.clone(), muted_style()),
            ]);
            lines.extend(wrap_styled_line(id_line, width));
        }

        if end < state.sessions.len() {
            lines.push(styled_line(
                format!("  ... {} more", state.sessions.len() - end),
                muted_style(),
            ));
        }

        lines.truncate(max_rows as usize);
        Some(lines)
    }

    fn build_run_picker_lines(
        state: &AppState,
        width: u16,
        max_rows: u16,
    ) -> Option<Vec<Line<'static>>> {
        let Some(crate::state::OverlayState::RunPicker { selected }) = state.overlay.as_ref()
        else {
            return None;
        };

        let items = state.work_picker_items();
        let mut lines = vec![Line::from(vec![
            Span::styled("Work", tool_title_style()),
            Span::styled("  Up/Down select, Enter open, Esc close", muted_style()),
        ])];
        if items.is_empty() {
            lines.push(styled_line("  No active runs.", muted_style()));
            return Some(lines);
        }

        let visible_capacity = (max_rows as usize).saturating_sub(1).max(1);
        let rows_per_item = 2usize;
        let visible_items = (visible_capacity / rows_per_item).max(1);
        let selected_index = (*selected).min(items.len().saturating_sub(1));
        let start = selected_index
            .saturating_sub(visible_items / 2)
            .min(items.len().saturating_sub(visible_items));
        let end = (start + visible_items).min(items.len());

        for (index, item) in items[start..end].iter().enumerate() {
            let index = start + index;
            let is_selected = index == selected_index;
            let marker = if is_selected { "› " } else { "  " };
            let title_style = if is_selected {
                tool_title_style()
            } else {
                Style::default().add_modifier(Modifier::BOLD)
            };
            let WorkPickerItem::Run {
                kind,
                title,
                status,
                ..
            } = item;
            lines.extend(wrap_styled_line(
                Line::from(vec![
                    Span::styled(
                        marker,
                        if is_selected {
                            tool_title_style()
                        } else {
                            muted_style()
                        },
                    ),
                    Span::styled(format!("{} · ", work_run_kind_label(*kind)), muted_style()),
                    Span::styled(title.clone(), title_style),
                    Span::styled(format!(" · {status}"), muted_style()),
                ]),
                width,
            ));
        }

        if end < items.len() {
            lines.push(styled_line(
                format!("  ... {} more", items.len() - end),
                muted_style(),
            ));
        }
        lines.truncate(max_rows as usize);
        Some(lines)
    }

    fn build_skill_manager_lines(
        state: &AppState,
        width: u16,
        max_rows: u16,
    ) -> Option<Vec<Line<'static>>> {
        let Some(crate::state::OverlayState::SkillManager { selected }) = state.overlay.as_ref()
        else {
            return None;
        };

        let mut lines = vec![Line::from(vec![
            Span::styled("Skill Manager", tool_title_style()),
            Span::styled("  Up/Down select, Enter details, Esc close", muted_style()),
        ])];
        if state.skills.is_empty() {
            lines.push(styled_line("  No installed skills yet.", muted_style()));
            return Some(lines);
        }

        let visible_capacity = (max_rows as usize).saturating_sub(1).max(1);
        let rows_per_skill = 2usize;
        let visible_skills = (visible_capacity / rows_per_skill).max(1);
        let selected_skill_index = (*selected).min(state.skills.len().saturating_sub(1));
        let start = selected_skill_index
            .saturating_sub(visible_skills / 2)
            .min(state.skills.len().saturating_sub(visible_skills));
        let end = (start + visible_skills).min(state.skills.len());
        let mut previous_source = None;

        for (index, skill) in state.skills[start..end].iter().enumerate() {
            let index = start + index;
            let is_selected = index == selected_skill_index;
            if previous_source != Some(skill.source) {
                previous_source = Some(skill.source);
                push_if_space(
                    &mut lines,
                    max_rows,
                    styled_line(
                        format!("  {}", skill_source_group_label(skill.source)),
                        muted_style(),
                    ),
                );
            }
            let marker = if is_selected { "› " } else { "  " };
            let title_style = if is_selected {
                tool_title_style()
            } else {
                Style::default().add_modifier(Modifier::BOLD)
            };
            let readonly = if skill.read_only { " · read-only" } else { "" };
            let delete_pending = "";
            let title = Line::from(vec![
                Span::styled(
                    marker,
                    if is_selected {
                        tool_title_style()
                    } else {
                        muted_style()
                    },
                ),
                Span::styled(skill.name.clone(), title_style),
                Span::styled(
                    format!(" · {}{readonly}{delete_pending}", skill.id),
                    muted_style(),
                ),
            ]);
            lines.extend(wrap_styled_line(title, width));
            let description = skill
                .description
                .clone()
                .unwrap_or_else(|| "No description".to_string());
            let detail = Line::from(vec![
                Span::styled("    ", muted_style()),
                Span::styled(description, muted_style()),
            ]);
            lines.extend(wrap_styled_line(detail, width));
        }

        if end < state.skills.len() {
            push_if_space(
                &mut lines,
                max_rows,
                styled_line(
                    format!("  ... {} more", state.skills.len() - end),
                    muted_style(),
                ),
            );
        }
        lines.truncate(max_rows as usize);
        Some(lines)
    }

    fn build_skill_mention_picker_lines(
        state: &AppState,
        width: u16,
        max_rows: u16,
    ) -> Option<Vec<Line<'static>>> {
        let Some(crate::state::OverlayState::SkillMentionPicker { selected }) =
            state.overlay.as_ref()
        else {
            return None;
        };
        let matches = state.skill_mention_matches();
        let query = state
            .composer
            .current_skill_mention_query()
            .unwrap_or_default();
        let mut lines = vec![Line::from(vec![
            Span::styled("Skill mentions", tool_title_style()),
            Span::styled("  Up/Down select, Enter insert, Esc close", muted_style()),
        ])];
        if matches.is_empty() {
            let message = if query.is_empty() {
                "  No skills installed."
            } else {
                "  No matching skills."
            };
            lines.push(styled_line(message, muted_style()));
            return Some(lines);
        }

        let visible_capacity = (max_rows as usize).saturating_sub(1).max(1);
        let selected_index = (*selected).min(matches.len().saturating_sub(1));
        let start = selected_index
            .saturating_sub(visible_capacity / 2)
            .min(matches.len().saturating_sub(visible_capacity));
        let end = (start + visible_capacity).min(matches.len());
        for (index, skill) in matches[start..end].iter().enumerate() {
            let index = start + index;
            let is_selected = index == selected_index;
            let title_style = if is_selected {
                tool_title_style()
            } else {
                Style::default().add_modifier(Modifier::BOLD)
            };
            let line = Line::from(vec![
                Span::styled(
                    if is_selected { "› " } else { "  " },
                    if is_selected {
                        tool_title_style()
                    } else {
                        muted_style()
                    },
                ),
                Span::styled(format!("@{}", skill.id), title_style),
                Span::styled(format!("  {}", skill.name), muted_style()),
            ]);
            lines.extend(wrap_styled_line(line, width));
        }
        lines.truncate(max_rows as usize);
        Some(lines)
    }

    fn build_skill_detail_lines(
        state: &AppState,
        width: u16,
        max_rows: u16,
    ) -> Option<Vec<Line<'static>>> {
        if !matches!(state.overlay, Some(crate::state::OverlayState::SkillDetail)) {
            return None;
        }
        let Some(skill) = state.selected_skill.as_ref() else {
            return Some(vec![
                Line::from(vec![
                    Span::styled("Skill", tool_title_style()),
                    Span::styled("  Esc close", muted_style()),
                ]),
                styled_line("  Skill details are unavailable.", muted_style()),
            ]);
        };

        let mut lines = vec![Line::from(vec![
            Span::styled("Skill", tool_title_style()),
            Span::styled("  Esc close", muted_style()),
        ])];
        let read_only = if skill.read_only { " · read-only" } else { "" };
        lines.extend(wrap_styled_line(
            Line::from(vec![
                Span::styled("  ", muted_style()),
                Span::styled(
                    skill.name.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" · {}{read_only}", skill.id), muted_style()),
            ]),
            width,
        ));
        lines.extend(wrap_styled_line(
            Line::from(vec![
                Span::styled("  source: ", muted_style()),
                Span::styled(skill.source.to_string(), muted_style()),
            ]),
            width,
        ));
        if let Some(description) = skill.description.as_ref().filter(|value| !value.is_empty()) {
            lines.extend(wrap_styled_line(
                Line::from(vec![
                    Span::styled("  description: ", muted_style()),
                    Span::styled(description.clone(), muted_style()),
                ]),
                width,
            ));
        }
        if !skill.suggested_tools.is_empty() {
            lines.extend(wrap_styled_line(
                Line::from(vec![
                    Span::styled("  suggested_tools: ", muted_style()),
                    Span::styled(skill.suggested_tools.join(", "), muted_style()),
                ]),
                width,
            ));
        }
        if let Some(source_ref) = skill.source_ref.as_ref().filter(|value| !value.is_empty()) {
            lines.extend(wrap_styled_line(
                Line::from(vec![
                    Span::styled("  source_ref: ", muted_style()),
                    Span::styled(source_ref.clone(), muted_style()),
                ]),
                width,
            ));
        }
        if skill
            .suggested_tools
            .iter()
            .any(|tool| tool == "spawn_subagent_batch")
        {
            lines.extend(wrap_styled_line(
                Line::from(vec![
                    Span::styled("  usage: ", muted_style()),
                    Span::styled(
                        "Use this skill by asking for parallel/team/subagent work.",
                        muted_style(),
                    ),
                ]),
                width,
            ));
        }

        lines.truncate(max_rows as usize);
        Some(lines)
    }

    fn build_model_picker_lines(
        state: &AppState,
        width: u16,
        max_rows: u16,
    ) -> Option<Vec<Line<'static>>> {
        let Some(crate::state::OverlayState::ModelPicker { provider, selected }) =
            state.overlay.as_ref()
        else {
            return None;
        };

        let mut lines = vec![Line::from(vec![
            Span::styled(format!("{provider} models"), tool_title_style()),
            Span::styled(
                "  Up/Down select, Enter switch, Shift+Enter default, Esc close",
                muted_style(),
            ),
        ])];
        if state.model_items.is_empty() {
            lines.push(styled_line("  No available models.", muted_style()));
            return Some(lines);
        }

        let visible_capacity = (max_rows as usize).saturating_sub(1).max(1);
        let selected_index = (*selected).min(state.model_items.len().saturating_sub(1));
        let (start, end) = picker_window_by_rows(
            &state.model_items,
            selected_index,
            visible_capacity,
            2,
            |item| item.category,
        );

        let mut previous_category = None;
        for (index, item) in state.model_items[start..end].iter().enumerate() {
            let index = start + index;
            let is_selected = index == selected_index;
            if previous_category != Some(item.category) {
                previous_category = Some(item.category);
                lines.push(styled_line(
                    format!("  {}", model_category_label(item.category)),
                    muted_style(),
                ));
            }
            let marker = if is_selected { "› " } else { "  " };
            let title_style = if is_selected {
                tool_title_style()
            } else {
                Style::default().add_modifier(Modifier::BOLD)
            };
            let current = if item.is_current { " · current" } else { "" };
            let default = if item.is_default { " · default" } else { "" };
            let usage = if item.usage_count > 0 {
                format!(" · used {}", item.usage_count)
            } else {
                String::new()
            };
            let title = Line::from(vec![
                Span::styled(
                    marker,
                    if is_selected {
                        tool_title_style()
                    } else {
                        muted_style()
                    },
                ),
                Span::styled(format!("{} · {}", item.provider, item.name), title_style),
                Span::styled(format!("{current}{default}{usage}"), muted_style()),
            ]);
            lines.extend(wrap_styled_line(title, width));
            let model_line = Line::from(vec![
                Span::styled("    model: ", muted_style()),
                Span::styled(item.model.clone(), muted_style()),
            ]);
            lines.extend(wrap_styled_line(model_line, width));
        }

        if end < state.model_items.len() {
            push_if_space(
                &mut lines,
                max_rows,
                styled_line(
                    format!("  ... {} more", state.model_items.len() - end),
                    muted_style(),
                ),
            );
        }
        lines.truncate(max_rows as usize);
        Some(lines)
    }

    fn build_provider_picker_lines(
        state: &AppState,
        width: u16,
        max_rows: u16,
    ) -> Option<Vec<Line<'static>>> {
        let Some(crate::state::OverlayState::ProviderPicker { selected }) = state.overlay.as_ref()
        else {
            return None;
        };

        let mut lines = vec![Line::from(vec![
            Span::styled("Providers", tool_title_style()),
            Span::styled("  Up/Down select, Enter models, Esc close", muted_style()),
        ])];
        if state.provider_items.is_empty() {
            lines.push(styled_line("  No available providers.", muted_style()));
            return Some(lines);
        }

        let selected_index = (*selected).min(state.provider_items.len().saturating_sub(1));
        let visible_capacity = (max_rows as usize).saturating_sub(1).max(1);
        let (start, end) = picker_window_by_rows(
            &state.provider_items,
            selected_index,
            visible_capacity,
            1,
            |item| item.category,
        );
        let mut previous_category = None;

        for (index, item) in state.provider_items[start..end].iter().enumerate() {
            let index = start + index;
            let is_selected = index == selected_index;
            if previous_category != Some(item.category) {
                previous_category = Some(item.category);
                lines.push(styled_line(
                    format!("  {}", provider_category_label(item.category)),
                    muted_style(),
                ));
            }
            let marker = if is_selected { "› " } else { "  " };
            let title_style = if is_selected {
                tool_title_style()
            } else {
                Style::default().add_modifier(Modifier::BOLD)
            };
            let current = if item.is_current { " · current" } else { "" };
            let usage = if item.usage_count > 0 {
                format!(" · used {}", item.usage_count)
            } else {
                String::new()
            };
            let line = Line::from(vec![
                Span::styled(
                    marker,
                    if is_selected {
                        tool_title_style()
                    } else {
                        muted_style()
                    },
                ),
                Span::styled(item.label.clone(), title_style),
                Span::styled(format!("{current}{usage}"), muted_style()),
            ]);
            lines.extend(wrap_styled_line(line, width));
        }

        if end < state.provider_items.len() {
            push_if_space(
                &mut lines,
                max_rows,
                styled_line(
                    format!("  ... {} more", state.provider_items.len() - end),
                    muted_style(),
                ),
            );
        }
        lines.truncate(max_rows as usize);
        Some(lines)
    }

    fn push_if_space(lines: &mut Vec<Line<'static>>, max_rows: u16, line: Line<'static>) {
        if lines.len() < max_rows as usize {
            lines.push(line);
        }
    }

    fn picker_window_by_rows<T>(
        items: &[T],
        selected: usize,
        row_capacity: usize,
        rows_per_item: usize,
        category: impl Fn(&T) -> crate::state::ModelPickerCategory + Copy,
    ) -> (usize, usize) {
        if items.is_empty() {
            return (0, 0);
        }

        let selected = selected.min(items.len().saturating_sub(1));
        let mut start = selected;
        let mut end = selected + 1;

        while start > 0
            && picker_window_row_count(&items[start - 1..end], rows_per_item, category)
                <= row_capacity
        {
            start -= 1;
        }

        while end < items.len()
            && picker_window_row_count(&items[start..end + 1], rows_per_item, category)
                <= row_capacity
        {
            end += 1;
        }

        (start, end)
    }

    fn picker_window_row_count<T>(
        items: &[T],
        rows_per_item: usize,
        category: impl Fn(&T) -> crate::state::ModelPickerCategory,
    ) -> usize {
        let mut rows = 0usize;
        let mut previous_category = None;
        for item in items {
            let item_category = category(item);
            if previous_category != Some(item_category) {
                rows += 1;
                previous_category = Some(item_category);
            }
            rows += rows_per_item;
        }
        rows
    }

    fn provider_category_label(category: crate::state::ModelPickerCategory) -> &'static str {
        match category {
            crate::state::ModelPickerCategory::Recent => "Recently used providers",
            crate::state::ModelPickerCategory::Frequent => "Most used providers",
            crate::state::ModelPickerCategory::Available => "Available providers",
        }
    }

    fn model_category_label(category: crate::state::ModelPickerCategory) -> &'static str {
        match category {
            crate::state::ModelPickerCategory::Recent => "Recently used",
            crate::state::ModelPickerCategory::Frequent => "Most used",
            crate::state::ModelPickerCategory::Available => "Available with API key",
        }
    }

    fn skill_source_group_label(source: SkillSource) -> &'static str {
        match source {
            SkillSource::System => "System skills",
            SkillSource::User => "User skills",
            SkillSource::External => "External skills",
        }
    }

    fn build_daemon_picker_lines(state: &AppState, width: u16) -> Option<Vec<Line<'static>>> {
        let Some(crate::state::OverlayState::DaemonPicker { selected }) = state.overlay.as_ref()
        else {
            return None;
        };

        let actions = [
            ("start", "Start the local daemon"),
            ("stop", "Stop the local daemon"),
        ];
        let mut lines = vec![Line::from(vec![
            Span::styled("Daemon", tool_title_style()),
            Span::styled("  Up/Down select, Enter run, Esc close", muted_style()),
        ])];
        for (index, (action, description)) in actions.iter().enumerate() {
            let selected = index == *selected;
            let line = Line::from(vec![
                Span::styled(
                    if selected { "› " } else { "  " },
                    if selected {
                        tool_title_style()
                    } else {
                        muted_style()
                    },
                ),
                Span::styled(
                    format!("/daemon {action}"),
                    if selected {
                        tool_title_style()
                    } else {
                        Style::default().add_modifier(Modifier::BOLD)
                    },
                ),
                Span::styled("  ", muted_style()),
                Span::styled(*description, muted_style()),
            ]);
            lines.extend(wrap_styled_line(line, width));
        }
        Some(lines)
    }

    fn build_command_picker_lines(
        state: &AppState,
        width: u16,
        max_rows: u16,
    ) -> Option<Vec<Line<'static>>> {
        let Some(crate::state::OverlayState::CommandPicker { selected }) = state.overlay.as_ref()
        else {
            return None;
        };

        let mut lines = Vec::with_capacity(SLASH_COMMAND_SPECS.len() + 1);
        lines.push(Line::from(vec![
            Span::styled("Slash commands", tool_title_style()),
            Span::styled("  Enter to run, Esc to clear", muted_style()),
        ]));
        let command_width = SLASH_COMMAND_SPECS
            .iter()
            .map(|spec| command_display(spec.command, spec.args).chars().count())
            .max()
            .unwrap_or(0);

        for (index, spec) in SLASH_COMMAND_SPECS.iter().enumerate() {
            let selected = index == *selected;
            let display = command_display(spec.command, spec.args);
            let padding = " ".repeat(command_width.saturating_sub(display.chars().count()) + 2);
            let line = Line::from(vec![
                Span::styled(
                    if selected { "› " } else { "  " },
                    if selected {
                        tool_title_style()
                    } else {
                        muted_style()
                    },
                ),
                Span::styled(
                    display,
                    if selected {
                        tool_title_style()
                    } else {
                        Style::default().add_modifier(Modifier::BOLD)
                    },
                ),
                Span::raw(padding),
                Span::styled(spec.description, muted_style()),
            ]);
            lines.extend(wrap_styled_line(line, width));
        }

        lines.truncate(max_rows as usize);
        Some(lines)
    }

    fn build_help_overlay_lines(
        state: &AppState,
        width: u16,
        max_rows: u16,
    ) -> Option<Vec<Line<'static>>> {
        if !matches!(state.overlay, Some(crate::state::OverlayState::Help)) {
            return None;
        }

        let mut lines = Vec::new();
        lines.push(Line::from(vec![
            Span::styled("Help", tool_title_style()),
            Span::styled("  Esc close", muted_style()),
        ]));
        for raw_line in HELP_TEXT.lines() {
            let line = if raw_line.is_empty() {
                Line::from("")
            } else {
                Line::from(vec![Span::styled(raw_line.to_string(), muted_style())])
            };
            lines.extend(wrap_styled_line(line, width));
        }

        lines.truncate(max_rows as usize);
        Some(lines)
    }

    fn compact_session_preview(value: &str) -> String {
        value.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn session_message_count_label(message_count: u32) -> String {
        let noun = if message_count == 1 {
            "chat message"
        } else {
            "chat messages"
        };
        format!(" · {message_count} {noun}")
    }

    fn command_display(command: &str, args: &str) -> String {
        if args.is_empty() {
            command.to_string()
        } else {
            format!("{command} {args}")
        }
    }

    fn render_history_append_lines(cells: &[TranscriptCell], width: u16) -> Vec<Line<'static>> {
        let mut lines = build_cell_lines(cells, width);
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        lines
    }

    fn build_cell_lines(cells: &[TranscriptCell], width: u16) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        for cell in cells {
            if !should_render_cell(cell) {
                continue;
            }

            match cell.kind {
                TranscriptCellKind::Tool | TranscriptCellKind::Subagent
                    if cell.group == MessageGroup::ToolActivity =>
                {
                    lines.extend(wrap_display_line(
                        &format_title(cell),
                        width,
                        cell_title_style(cell),
                    ));
                    for line in cell_body_lines(cell) {
                        lines.extend(wrap_prefixed_line(
                            CONTINUATION_PREFIX,
                            &line,
                            width,
                            cell_body_style(cell),
                        ));
                    }
                }
                TranscriptCellKind::Tool | TranscriptCellKind::Subagent => {
                    let title = format_title(cell);
                    let summary = summarize_tool_body(cell.body.as_str());
                    let line = if summary.is_empty() {
                        styled_line(title, cell_title_style(cell))
                    } else {
                        Line::from(vec![
                            Span::styled(title, cell_title_style(cell)),
                            Span::raw(" "),
                            Span::styled(summary, cell_body_style(cell)),
                        ])
                    };
                    lines.extend(wrap_styled_line(line, width));
                }
                _ => {
                    lines.extend(wrap_display_line(
                        &format_title(cell),
                        width,
                        cell_title_style(cell),
                    ));
                    for line in cell_body_lines(cell) {
                        lines.extend(wrap_prefixed_line(
                            CONTINUATION_PREFIX,
                            &line,
                            width,
                            cell_body_style(cell),
                        ));
                    }
                }
            }

            lines.push(Line::from(""));
        }

        if lines.last().is_some_and(line_is_empty) {
            lines.pop();
        }
        lines
    }

    #[cfg(test)]
    fn is_cell_prefix(previous: &[TranscriptCell], current: &[TranscriptCell]) -> bool {
        previous.len() <= current.len()
            && previous
                .iter()
                .zip(current.iter())
                .all(|(left, right)| left == right)
    }

    fn should_render_cell(cell: &TranscriptCell) -> bool {
        match cell.kind {
            TranscriptCellKind::Tool | TranscriptCellKind::Subagent => {
                !summarize_tool_body(cell.body.as_str()).is_empty()
            }
            TranscriptCellKind::Assistant => cell.is_active || !cell.body.trim().is_empty(),
            TranscriptCellKind::User | TranscriptCellKind::System | TranscriptCellKind::Notice => {
                !cell.body.trim().is_empty()
            }
        }
    }

    fn cell_title_style(cell: &TranscriptCell) -> Style {
        match cell.kind {
            TranscriptCellKind::User => Style::default()
                .fg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
            TranscriptCellKind::Assistant => Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            TranscriptCellKind::System | TranscriptCellKind::Notice if cell.title == "Error" => {
                error_style()
            }
            TranscriptCellKind::System | TranscriptCellKind::Notice => muted_style(),
            TranscriptCellKind::Tool => tool_title_style(),
            TranscriptCellKind::Subagent => subagent_title_style(),
        }
    }

    fn cell_body_style(cell: &TranscriptCell) -> Style {
        match cell.kind {
            TranscriptCellKind::User => Style::default(),
            TranscriptCellKind::System | TranscriptCellKind::Notice if cell.title == "Error" => {
                error_style()
            }
            TranscriptCellKind::System | TranscriptCellKind::Notice => muted_style(),
            TranscriptCellKind::Tool => tool_body_style(),
            TranscriptCellKind::Subagent => subagent_body_style(),
            TranscriptCellKind::Assistant => Style::default(),
        }
    }

    fn tool_title_style() -> Style {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    }

    fn subagent_title_style() -> Style {
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD)
    }

    fn subagent_body_style() -> Style {
        Style::default().fg(Color::LightMagenta)
    }

    fn tool_body_style() -> Style {
        Style::default().fg(Color::DarkGray)
    }

    fn muted_style() -> Style {
        Style::default().fg(Color::DarkGray)
    }

    fn error_style() -> Style {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    }

    #[cfg(test)]
    fn stable_history_has_rendered_lines(state: &AppState) -> bool {
        build_stable_history_cells(state)
            .iter()
            .any(should_render_cell)
    }

    fn queue_clear_visible(writer: &mut impl Write) -> IoResult<()> {
        queue!(writer, Clear(ClearType::All), MoveTo(0, 0))
    }

    fn queue_purge_visible_and_scrollback(writer: &mut impl Write) -> IoResult<()> {
        queue!(
            writer,
            Clear(ClearType::All),
            Clear(ClearType::Purge),
            MoveTo(0, 0)
        )
    }

    fn normalize_terminal_size((width, height): (u16, u16)) -> (u16, u16) {
        (width.max(3), height.max(3))
    }

    fn prompt_content_width(total_width: u16) -> u16 {
        total_width.saturating_sub(2).max(1)
    }

    fn format_title(cell: &TranscriptCell) -> String {
        let title = match tool_title_input(cell) {
            Some(input) => format!("{} {}", cell.title, input),
            None => cell.title.clone(),
        };
        match (visible_subtitle(cell.subtitle.as_deref()), cell.is_active) {
            (Some(subtitle), true) => format!("{title} · {subtitle}"),
            (Some(subtitle), false) => format!("{title} {subtitle}"),
            (None, _) => title,
        }
    }

    fn tool_title_input(cell: &TranscriptCell) -> Option<String> {
        if cell.kind != TranscriptCellKind::Tool || cell.group != MessageGroup::ToolActivity {
            return None;
        }
        summarize_tool_input_for_title(cell.body.as_str())
    }

    fn summarize_tool_input_for_title(body: &str) -> Option<String> {
        if let Some(input) = json_after_tool_label(body, "Input:") {
            return summarize_tool_input_json_for_title(&input).or_else(|| {
                let input = compact_json(&input);
                Some(format!("'{}'", escape_single_quoted(&input)))
            });
        }
        text_after_tool_label(body, "Input:")
            .map(compact_tool_text)
            .filter(|input| !input.is_empty())
            .map(|input| format!("'{}'", escape_single_quoted(&input)))
    }

    fn summarize_tool_input_json_for_title(value: &Value) -> Option<String> {
        if let Some(command) = value
            .get("command")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Some(format!(
                "'{}'",
                escape_single_quoted(&compact_command(command))
            ));
        }

        if let Some(query) = value
            .get("query")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Some(format!(
                "'{}'",
                escape_single_quoted(&compact_tool_text(query))
            ));
        }

        summarize_tool_input_json(value)
            .and_then(|summary| summary.strip_prefix("Input: ").map(ToOwned::to_owned))
    }

    fn escape_single_quoted(value: &str) -> String {
        value.replace('\'', "\\'")
    }

    fn visible_subtitle(subtitle: Option<&str>) -> Option<&str> {
        let subtitle = subtitle?.trim();
        if subtitle.is_empty() {
            return None;
        }
        let Some(rest) = subtitle.strip_prefix('#') else {
            return Some(subtitle);
        };
        rest.split_once(" · ")
            .map(|(_, visible)| visible.trim())
            .filter(|visible| !visible.is_empty())
    }

    fn normalize_body_lines(body: &str) -> Vec<String> {
        if body.trim().is_empty() {
            return Vec::new();
        }

        let raw = body.lines().collect::<Vec<_>>();
        let first_non_blank = raw.iter().position(|line| !line.trim().is_empty());
        let last_non_blank = raw.iter().rposition(|line| !line.trim().is_empty());
        let (Some(start), Some(end)) = (first_non_blank, last_non_blank) else {
            return Vec::new();
        };

        let mut lines = Vec::new();
        let mut previous_blank = false;
        for line in &raw[start..=end] {
            if line.trim().is_empty() {
                if !previous_blank {
                    lines.push(String::new());
                    previous_blank = true;
                }
            } else {
                lines.push((*line).to_string());
                previous_blank = false;
            }
        }
        lines
    }

    fn cell_body_lines(cell: &TranscriptCell) -> Vec<String> {
        if cell.kind == TranscriptCellKind::Assistant
            && cell.is_active
            && cell.body.trim().is_empty()
        {
            return vec![String::new()];
        }
        if cell.kind == TranscriptCellKind::Tool
            && cell.group == MessageGroup::ToolActivity
            && has_tool_activity_labels(cell.body.as_str())
        {
            return structured_tool_activity_body_lines(cell.body.as_str());
        }
        if cell.kind == TranscriptCellKind::Subagent && cell.group == MessageGroup::ToolActivity {
            let lines = structured_tool_activity_body_lines(cell.body.as_str());
            if !lines.is_empty() {
                return lines;
            }
        }
        normalize_body_lines(cell.body.as_str())
    }

    fn has_tool_activity_labels(body: &str) -> bool {
        body.contains("Input:") || body.contains("Output:") || body.contains("Error:")
    }

    fn structured_tool_activity_body_lines(body: &str) -> Vec<String> {
        let mut lines = Vec::new();

        append_tool_result_lines(&mut lines, body, "Output:");
        append_tool_result_lines(&mut lines, body, "Error:");
        lines
    }

    fn append_tool_result_lines(lines: &mut Vec<String>, body: &str, label: &str) {
        let display_label = label.trim_end_matches(':');
        match json_after_tool_label(body, label) {
            Some(value) => {
                let summary = match display_label {
                    "Output" => summarize_tool_output_json(&value),
                    "Error" => summarize_tool_error_json(&value),
                    _ => None,
                }
                .unwrap_or_else(|| format!("{display_label}: {}", compact_json(&value)));
                lines.push(visible_tool_result_line(label, &summary));
                append_process_stream_lines(lines, &value, "stdout");
                append_process_stream_lines(lines, &value, "stderr");
            }
            None => {
                if let Some(value) = text_after_tool_label(body, label) {
                    let value = compact_tool_text(value);
                    if label == "Output:" {
                        lines.push(value);
                    } else {
                        lines.push(format!("{display_label}: {value}"));
                    }
                }
            }
        }
    }

    fn visible_tool_result_line(label: &str, summary: &str) -> String {
        if label != "Output:" {
            return summary.to_string();
        }
        summary
            .strip_prefix("Output:")
            .map(str::trim_start)
            .filter(|value| !value.is_empty())
            .unwrap_or(summary)
            .to_string()
    }

    fn append_process_stream_lines(lines: &mut Vec<String>, value: &Value, field: &str) {
        let Some(output) = value
            .get(field)
            .and_then(Value::as_str)
            .filter(|output| !output.trim().is_empty())
        else {
            return;
        };
        lines.push(format!("{field}:"));
        lines.extend(normalize_body_lines(output));
    }

    fn compact_json(value: &Value) -> String {
        serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
    }

    fn summarize_tool_body(body: &str) -> String {
        if let Some(summary) = summarize_structured_tool_body(body) {
            return truncate_tool_summary(&summary);
        }

        let compact = body
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if compact.is_empty() {
            return String::new();
        }

        truncate_tool_summary(&compact)
    }

    fn truncate_tool_summary(value: &str) -> String {
        let mut summary = value.chars().take(TOOL_SUMMARY_LIMIT).collect::<String>();
        if value.chars().count() > TOOL_SUMMARY_LIMIT {
            summary.push('…');
        }
        summary
    }

    fn summarize_structured_tool_body(body: &str) -> Option<String> {
        let mut parts = Vec::new();
        if let Some(input) = json_after_tool_label(body, "Input:")
            && let Some(summary) = summarize_tool_input_json(&input)
        {
            parts.push(summary);
        }
        match json_after_tool_label(body, "Output:") {
            Some(output) => {
                if let Some(summary) = summarize_tool_output_json(&output) {
                    parts.push(summary);
                }
            }
            None => {
                if let Some(output) = text_after_tool_label(body, "Output:") {
                    parts.push(format!("Output: {}", compact_tool_text(output)));
                }
            }
        }
        match json_after_tool_label(body, "Error:") {
            Some(error) => {
                if let Some(summary) = summarize_tool_error_json(&error) {
                    parts.push(summary);
                }
            }
            None => {
                if let Some(error) = text_after_tool_label(body, "Error:") {
                    parts.push(format!("Error: {}", compact_tool_text(error)));
                }
            }
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" "))
        }
    }

    fn json_after_tool_label(body: &str, label: &str) -> Option<Value> {
        let start = body.find(label)? + label.len();
        let rest = body[start..].trim_start();
        let end = ["\nInput:", "\nOutput:", "\nError:"]
            .iter()
            .filter_map(|marker| rest.find(marker))
            .min()
            .unwrap_or(rest.len());
        serde_json::from_str(rest[..end].trim()).ok()
    }

    fn text_after_tool_label<'a>(body: &'a str, label: &str) -> Option<&'a str> {
        let start = body.find(label)? + label.len();
        let rest = body[start..].trim_start();
        let end = ["\nInput:", "\nOutput:", "\nError:"]
            .iter()
            .filter_map(|marker| rest.find(marker))
            .min()
            .unwrap_or(rest.len());
        let value = rest[..end].trim();
        (!value.is_empty()).then_some(value)
    }

    fn compact_tool_text(value: &str) -> String {
        value
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn summarize_tool_input_json(value: &Value) -> Option<String> {
        if let Some(command) = value
            .get("command")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Some(format!("Input: $ {}", compact_command(command)));
        }

        if let Some(pattern) = value
            .get("pattern")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let target = value
                .get("path")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(compact_tool_path)
                .unwrap_or_else(|| "workspace".to_string());
            let mode = value
                .get("output_mode")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            return Some(match mode {
                Some("files_with_matches") => format!("Input: grep files in {target} · {pattern}"),
                Some("count") => format!("Input: grep count in {target} · {pattern}"),
                _ => format!("Input: grep {target} · {pattern}"),
            });
        }

        if let Some(patch) = value
            .get("patch")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            && let Some(summary) = summarize_patch_input(patch)
        {
            return Some(summary);
        }

        let path = value
            .get("file_path")
            .or_else(|| value.get("path"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())?;
        let path = compact_tool_path(path);
        if let Some(edits) = value.get("edits").and_then(Value::as_array) {
            return Some(format!(
                "Input: edit {path} · {} {}",
                edits.len(),
                replacement_label(edits.len())
            ));
        }
        if value.get("old_string").is_some() && value.get("new_string").is_some() {
            return Some(format!("Input: edit {path} · 1 replacement"));
        }
        if let Some(action) = value
            .get("action")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Some(format!("Input: {action} {path}"));
        }
        None
    }

    fn summarize_tool_output_json(value: &Value) -> Option<String> {
        if let Some(summary) = summarize_bash_result_json("Output", value) {
            return Some(summary);
        }

        if let Some(match_count) = value.get("match_count").and_then(Value::as_u64) {
            let label = count_label(match_count, "match", "matches");
            return Some(format!("Output: {match_count} {label}"));
        }
        if let Some(total) = value.get("total").and_then(Value::as_u64) {
            if value.get("files").and_then(Value::as_array).is_some() {
                let label = count_label(total, "matching file", "matching files");
                return Some(format!("Output: {total} {label}"));
            }
            if value.get("counts").and_then(Value::as_array).is_some() {
                let label = count_label(total, "file with matches", "files with matches");
                return Some(format!("Output: {total} {label}"));
            }
        }

        if let Some(results) = value.get("results").and_then(Value::as_array)
            && let Some(summary) = summarize_patch_results(results)
        {
            return Some(summary);
        }

        let has_edit_result =
            value.get("edits_applied").is_some() || value.get("lines_changed").is_some();
        let path = value
            .get("path")
            .or_else(|| value.get("file_path"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())?;
        let path = compact_tool_path(path);
        if !has_edit_result {
            return None;
        }
        let edits = value
            .get("edits_applied")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(1);
        let mut summary = format!(
            "Output: edited {path} · {edits} {}",
            replacement_label(edits)
        );
        if let Some(lines_changed) = value.get("lines_changed").and_then(Value::as_u64) {
            let label = if lines_changed == 1 { "line" } else { "lines" };
            summary.push_str(&format!(" · {lines_changed} {label} changed"));
        }
        Some(summary)
    }

    fn summarize_tool_error_json(value: &Value) -> Option<String> {
        summarize_bash_result_json("Error", value).or_else(|| {
            if value.get("pending_approval").and_then(Value::as_bool) == Some(true) {
                Some("Error: approval required".to_string())
            } else if value.get("blocked").and_then(Value::as_bool) == Some(true) {
                Some("Error: blocked by policy".to_string())
            } else {
                value
                    .get("error")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|error| format!("Error: {}", compact_tool_text(error)))
            }
        })
    }

    fn summarize_bash_result_json(label: &str, value: &Value) -> Option<String> {
        let exit_code = value.get("exit_code").and_then(Value::as_i64)?;
        let mut parts = vec![format!("{label}: exit {exit_code}")];
        if let Some(duration_ms) = value.get("duration_ms").and_then(Value::as_u64) {
            parts.push(format!("{duration_ms}ms"));
        }
        if let Some(stdout_lines) = output_line_count(value.get("stdout").and_then(Value::as_str)) {
            parts.push(format!(
                "{stdout_lines} stdout {}",
                line_label(stdout_lines)
            ));
        }
        if let Some(stderr_lines) = output_line_count(value.get("stderr").and_then(Value::as_str)) {
            parts.push(format!(
                "{stderr_lines} stderr {}",
                line_label(stderr_lines)
            ));
        }
        if value.get("truncated").and_then(Value::as_bool) == Some(true) {
            parts.push("truncated".to_string());
        }
        Some(parts.join(" · "))
    }

    fn output_line_count(value: Option<&str>) -> Option<usize> {
        let count = value?
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count();
        (count > 0).then_some(count)
    }

    fn line_label(count: usize) -> &'static str {
        if count == 1 { "line" } else { "lines" }
    }

    fn count_label(count: u64, singular: &'static str, plural: &'static str) -> &'static str {
        if count == 1 { singular } else { plural }
    }

    fn summarize_patch_input(patch: &str) -> Option<String> {
        let operations = patch
            .lines()
            .filter_map(parse_patch_header)
            .collect::<Vec<_>>();
        if operations.is_empty() {
            return None;
        }
        let files = compact_patch_file_list(operations.iter().map(|(_, path)| path.as_str()));
        Some(format!(
            "Input: patch {} {} · {files}",
            operations.len(),
            file_label(operations.len())
        ))
    }

    fn parse_patch_header(line: &str) -> Option<(String, String)> {
        let trimmed = line.trim();
        let (operation, path) = trimmed
            .strip_prefix("*** Update File: ")
            .map(|path| ("update", path))
            .or_else(|| {
                trimmed
                    .strip_prefix("*** Add File: ")
                    .map(|path| ("add", path))
            })
            .or_else(|| {
                trimmed
                    .strip_prefix("*** Delete File: ")
                    .map(|path| ("delete", path))
            })?;
        let path = path.trim();
        if path.is_empty() {
            return None;
        }
        Some((operation.to_string(), compact_tool_path(path)))
    }

    fn summarize_patch_results(results: &[Value]) -> Option<String> {
        let files = results
            .iter()
            .filter_map(Value::as_str)
            .filter_map(parse_patch_result_file)
            .collect::<Vec<_>>();
        if files.is_empty() {
            return None;
        }
        Some(format!(
            "Output: patched {} {} · {}",
            files.len(),
            file_label(files.len()),
            compact_patch_file_list(files.iter().map(String::as_str))
        ))
    }

    fn parse_patch_result_file(result: &str) -> Option<String> {
        let (_, path) = result.split_once(':')?;
        let path = path.trim();
        if path.is_empty() {
            return None;
        }
        Some(compact_tool_path(path))
    }

    fn compact_patch_file_list<'a>(paths: impl Iterator<Item = &'a str>) -> String {
        let paths = paths.collect::<Vec<_>>();
        let mut listed = paths.iter().take(3).copied().collect::<Vec<_>>().join(", ");
        let remaining = paths.len().saturating_sub(3);
        if remaining > 0 {
            listed.push_str(&format!(" +{remaining} more"));
        }
        listed
    }

    fn file_label(count: usize) -> &'static str {
        if count == 1 { "file" } else { "files" }
    }

    fn compact_command(command: &str) -> String {
        command.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn replacement_label(count: usize) -> &'static str {
        if count == 1 {
            "replacement"
        } else {
            "replacements"
        }
    }

    fn compact_tool_path(path: &str) -> String {
        Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| path.to_string())
    }

    fn footer_status_line(state: &AppState) -> String {
        let base = match state.current_session() {
            Some(session) => {
                let provider = session.provider.trim();
                let model = session.model.trim();
                match (provider, model.is_empty()) {
                    (provider, false) if !provider.is_empty() => format!("{provider} · {model}"),
                    (_, false) => model.to_string(),
                    _ => state.status.clone(),
                }
            }
            None => state
                .pending_session
                .as_ref()
                .map(|pending_session| pending_session.model_label())
                .unwrap_or_else(|| state.status.clone()),
        };
        mode_prefixed_footer(state.input_mode, base)
    }

    fn mode_prefixed_footer(mode: crate::state::InputMode, base: String) -> String {
        match mode {
            crate::state::InputMode::Chat => base,
            crate::state::InputMode::Plan => {
                if base.trim().is_empty() {
                    mode.label().to_string()
                } else {
                    format!("{} · {}", mode.label(), base)
                }
            }
        }
    }

    fn placeholder_line(mode: crate::state::InputMode, inner_width: u16) -> Line<'static> {
        let placeholder = match mode {
            crate::state::InputMode::Chat => "Type your message or use /help",
            crate::state::InputMode::Plan => "Plan mode: describe the plan",
        };
        styled_line(truncate_to_width(placeholder, inner_width), muted_style())
    }

    #[cfg(test)]
    fn preserve_first_line_tail(lines: Vec<Line<'static>>, max_rows: usize) -> Vec<Line<'static>> {
        if max_rows == 0 || lines.len() <= max_rows {
            return lines;
        }
        if max_rows == 1 {
            return lines.into_iter().take(1).collect();
        }

        let mut visible = vec![lines[0].clone()];
        let body_start = lines.len().saturating_sub(max_rows - 1);
        visible.extend_from_slice(&lines[body_start.max(1)..]);
        visible
    }

    #[cfg(test)]
    fn preserve_active_cell_separator(
        lines: Vec<Line<'static>>,
        max_rows: usize,
        prepend_separator: bool,
    ) -> Vec<Line<'static>> {
        if !prepend_separator || max_rows <= 1 || lines.is_empty() {
            return preserve_first_line_tail(lines, max_rows);
        }

        let mut visible = vec![Line::from("")];
        visible.extend(preserve_first_line_tail(lines, max_rows - 1));
        visible
    }

    #[cfg(test)]
    fn preserve_live_turn_lines(
        pending_lines: Vec<Line<'static>>,
        active_lines: Vec<Line<'static>>,
        max_rows: usize,
        prepend_separator: bool,
    ) -> Vec<Line<'static>> {
        if max_rows == 0 {
            return Vec::new();
        }

        let mut visible = Vec::new();
        let mut remaining = max_rows;
        if prepend_separator && remaining > 1 {
            visible.push(Line::from(""));
            remaining -= 1;
        }

        if active_lines.len() >= remaining {
            visible.extend(preserve_first_line_tail(active_lines, remaining));
            return visible;
        }

        let pending_capacity = remaining.saturating_sub(active_lines.len());
        if pending_capacity > 0 {
            let start = pending_lines.len().saturating_sub(pending_capacity);
            visible.extend_from_slice(&pending_lines[start..]);
        }
        visible.extend(active_lines);
        visible
    }

    fn wrap_display_line(value: &str, width: u16, style: Style) -> Vec<Line<'static>> {
        wrap_styled_line(styled_line(value.to_string(), style), width)
    }

    fn wrap_prefixed_line(
        prefix: &str,
        value: &str,
        width: u16,
        style: Style,
    ) -> Vec<Line<'static>> {
        let prefix_width = display_width(prefix) as usize;
        let content_width = (width as usize).saturating_sub(prefix_width).max(1);
        wrap_styled_line(styled_line(value.to_string(), style), content_width as u16)
            .into_iter()
            .enumerate()
            .map(|(index, line)| prefix_styled_line(if index == 0 { prefix } else { "  " }, line))
            .collect()
    }

    fn wrap_styled_line(line: Line<'static>, width: u16) -> Vec<Line<'static>> {
        let width = width.max(1) as usize;
        let line_style = line.style;
        let line_alignment = line.alignment;
        let mut lines = Vec::new();
        let mut current = Vec::new();
        let mut current_width = 0usize;

        for span in line.spans {
            let span_style = span.style;
            for ch in span.content.chars() {
                let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                if current_width > 0 && ch_width > 0 && current_width + ch_width > width {
                    lines.push(line_from_spans(
                        std::mem::take(&mut current),
                        line_style,
                        line_alignment,
                    ));
                    current_width = 0;
                }
                push_char_span(&mut current, ch, span_style);
                current_width += ch_width;
            }
        }

        lines.push(line_from_spans(current, line_style, line_alignment));
        lines
    }

    fn bottom_anchor_lines(
        lines: Vec<Line<'static>>,
        height: usize,
        scroll_from_bottom: usize,
    ) -> Vec<Line<'static>> {
        if height == 0 {
            return Vec::new();
        }
        let total = lines.len();
        let scroll_from_bottom = clamp_history_scroll(total, height, scroll_from_bottom);
        let end = total.saturating_sub(scroll_from_bottom);
        let start = end.saturating_sub(height);
        let mut visible = lines[start..end].to_vec();
        if visible.len() < height {
            let mut padding = vec![Line::from(""); height - visible.len()];
            padding.append(&mut visible);
            return padding;
        }
        visible
    }

    fn tail_lines(
        lines: Vec<Line<'static>>,
        height: usize,
        scroll_from_bottom: usize,
    ) -> Vec<Line<'static>> {
        if height == 0 || lines.is_empty() {
            return Vec::new();
        }
        let total = lines.len();
        let scroll_from_bottom = clamp_history_scroll(total, height, scroll_from_bottom);
        let end = total.saturating_sub(scroll_from_bottom);
        let start = end.saturating_sub(height);
        lines[start..end].to_vec()
    }

    fn clamp_history_scroll(total_lines: usize, viewport_height: usize, requested: usize) -> usize {
        requested.min(total_lines.saturating_sub(viewport_height))
    }

    fn preserve_scrolled_offset(
        previous_line_count: usize,
        current_line_count: usize,
        current_scroll_from_bottom: usize,
    ) -> usize {
        if current_line_count > previous_line_count {
            current_scroll_from_bottom.saturating_add(current_line_count - previous_line_count)
        } else {
            current_scroll_from_bottom.saturating_sub(previous_line_count - current_line_count)
        }
    }

    fn changed_row_indices(previous: &[Line<'static>], current: &[Line<'static>]) -> Vec<usize> {
        let max_len = previous.len().max(current.len());
        let mut rows = Vec::new();
        for index in 0..max_len {
            if previous.get(index) != current.get(index) {
                rows.push(index);
            }
        }
        rows
    }

    #[cfg(test)]
    fn protected_append_top(
        previous: Option<&ViewportSnapshot>,
        current: &ViewportSnapshot,
    ) -> u16 {
        previous
            .map(|previous| {
                current
                    .top
                    .min(previous.top)
                    .min(previous.prompt_top)
                    .min(current.prompt_top)
            })
            .unwrap_or(current.top.min(current.prompt_top))
    }

    #[cfg(test)]
    fn visible_history_fill_count(
        history_line_count: usize,
        viewport_top: u16,
        incoming_line_count: usize,
    ) -> usize {
        (viewport_top as usize)
            .saturating_sub(history_line_count)
            .min(incoming_line_count)
    }

    fn styled_line(value: impl Into<String>, style: Style) -> Line<'static> {
        Line::from(Span::styled(value.into(), style))
    }

    fn line_from_spans(
        spans: Vec<Span<'static>>,
        style: Style,
        alignment: Option<ratatui::layout::Alignment>,
    ) -> Line<'static> {
        let mut line = Line::from(spans);
        line.style = style;
        line.alignment = alignment;
        line
    }

    fn push_char_span(spans: &mut Vec<Span<'static>>, ch: char, style: Style) {
        if let Some(last) = spans.last_mut()
            && last.style == style
        {
            last.content.to_mut().push(ch);
            return;
        }
        spans.push(Span::styled(ch.to_string(), style));
    }

    fn prefix_styled_line(prefix: &str, line: Line<'static>) -> Line<'static> {
        let mut spans = Vec::with_capacity(line.spans.len() + 1);
        spans.push(Span::raw(prefix.to_string()));
        spans.extend(line.spans);
        line_from_spans(spans, line.style, line.alignment)
    }

    fn line_is_empty(line: &Line<'_>) -> bool {
        line.spans
            .iter()
            .all(|span| span.content.as_ref().is_empty())
    }

    #[cfg(test)]
    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    fn truncate_line_to_width(line: &Line<'static>, width: u16) -> Line<'static> {
        let width = width as usize;
        let mut spans = Vec::new();
        let mut current_width = 0usize;

        for span in &line.spans {
            for ch in span.content.chars() {
                let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                if current_width + ch_width > width {
                    return line_from_spans(spans, line.style, line.alignment);
                }
                push_char_span(&mut spans, ch, span.style);
                current_width += ch_width;
            }
        }

        line_from_spans(spans, line.style, line.alignment)
    }

    fn write_styled_line(writer: &mut impl Write, line: &Line<'static>) -> IoResult<()> {
        for span in &line.spans {
            let style = if line.style == Style::default() {
                span.style
            } else {
                span.style.patch(line.style)
            };
            queue!(
                writer,
                SetAttribute(Attribute::Reset),
                SetColors(Colors::new(
                    style
                        .fg
                        .map(std::convert::Into::into)
                        .unwrap_or(CrosstermColor::Reset),
                    style
                        .bg
                        .map(std::convert::Into::into)
                        .unwrap_or(CrosstermColor::Reset),
                ))
            )?;
            queue_modifiers(writer, style.add_modifier - style.sub_modifier)?;
            queue!(writer, Print(span.content.clone()))?;
        }
        queue!(
            writer,
            SetForegroundColor(CrosstermColor::Reset),
            SetBackgroundColor(CrosstermColor::Reset),
            SetAttribute(Attribute::Reset),
        )
    }

    fn queue_modifiers(writer: &mut impl Write, modifiers: Modifier) -> IoResult<()> {
        if modifiers.contains(Modifier::BOLD) {
            queue!(writer, SetAttribute(Attribute::Bold))?;
        }
        if modifiers.contains(Modifier::DIM) {
            queue!(writer, SetAttribute(Attribute::Dim))?;
        }
        if modifiers.contains(Modifier::ITALIC) {
            queue!(writer, SetAttribute(Attribute::Italic))?;
        }
        if modifiers.contains(Modifier::UNDERLINED) {
            queue!(writer, SetAttribute(Attribute::Underlined))?;
        }
        if modifiers.contains(Modifier::REVERSED) {
            queue!(writer, SetAttribute(Attribute::Reverse))?;
        }
        if modifiers.contains(Modifier::CROSSED_OUT) {
            queue!(writer, SetAttribute(Attribute::CrossedOut))?;
        }
        if modifiers.contains(Modifier::SLOW_BLINK) {
            queue!(writer, SetAttribute(Attribute::SlowBlink))?;
        }
        if modifiers.contains(Modifier::RAPID_BLINK) {
            queue!(writer, SetAttribute(Attribute::RapidBlink))?;
        }
        Ok(())
    }

    fn truncate_to_width(value: &str, width: u16) -> String {
        let width = width as usize;
        let mut out = String::new();
        let mut current = 0usize;
        for ch in value.chars() {
            let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if current + ch_width > width {
                break;
            }
            out.push(ch);
            current += ch_width;
        }
        out
    }

    fn display_width(value: &str) -> u16 {
        unicode_width::UnicodeWidthStr::width(value) as u16
    }

    #[cfg(test)]
    mod tests {
        use super::{
            ViewportSnapshot, bottom_anchor_lines, build_stable_history_cells,
            build_transient_lines, build_viewport_snapshot, cell_title_style, changed_row_indices,
            clamp_history_scroll, compact_session_preview, footer_status_line, format_title,
            history_redraw_top, is_cell_prefix, line_text, normalize_body_lines,
            preserve_active_cell_separator, preserve_first_line_tail, preserve_scrolled_offset,
            protected_append_top, queue_clear_visible, queue_purge_visible_and_scrollback,
            render_history_append_lines, session_message_count_label,
            should_force_live_viewport_redraw, summarize_tool_body, visible_history_fill_count,
            visible_history_tail_lines, write_styled_line,
        };
        use crossterm::queue;
        use crossterm::style::{Attribute, SetAttribute};
        use ratatui::style::{Color, Modifier, Style};
        use ratatui::text::Line;
        use ratatui::text::Span;

        use crate::render::render_shell_bottom_viewport;
        use crate::slash_command::SLASH_COMMAND_SPECS;
        use crate::state::{
            AnchoredRuntimeCell, AppState, ModelPickerCategory, ModelPickerItem,
            PendingSessionState, ProviderPickerItem, SkillPickerItem,
        };
        use crate::transcript::{MessageGroup, TranscriptCell, TranscriptCellKind};
        use types::StreamFrame;
        use types::{
            ChatMessage, ChatSession, ChatSessionSummary, ChatTurnEventKind, Skill, SkillSource,
        };

        fn line_texts(lines: &[Line<'static>]) -> Vec<String> {
            lines.iter().map(line_text).collect()
        }

        #[test]
        fn tool_summary_compacts_and_truncates_multiline_content() {
            let summary = summarize_tool_body(" \n {\"ok\": true}\n\nsecond line");
            assert_eq!(summary, "{\"ok\": true} second line");
        }

        #[test]
        fn tool_summary_formats_edit_input_json() {
            let summary = summarize_tool_body(&format!(
                "Input: {}",
                serde_json::json!({
                    "file_path": "README.md",
                    "old_string": "status=pending",
                    "new_string": "status=done"
                })
            ));

            assert_eq!(summary, "Input: edit README.md · 1 replacement");
        }

        #[test]
        fn tool_summary_formats_file_read_without_claiming_edit() {
            let summary = summarize_tool_body(&format!(
                "Input: {}\nOutput: {}",
                serde_json::json!({
                    "action": "read",
                    "path": "README.md"
                }),
                serde_json::json!({
                    "path": "/private/tmp/work/README.md",
                    "content": "# Title\n"
                })
            ));

            assert_eq!(summary, "Input: read README.md");
            assert!(!summary.contains("edited"));
        }

        #[test]
        fn tool_summary_formats_successful_bash_json() {
            let summary = summarize_tool_body(&format!(
                "Input: {}\nOutput: {}",
                serde_json::json!({
                    "command": "cargo   test  -p tui"
                }),
                serde_json::json!({
                    "exit_code": 0,
                    "stdout": "ok\n",
                    "stderr": "",
                    "truncated": false,
                    "duration_ms": 42
                })
            ));

            assert_eq!(
                summary,
                "Input: $ cargo test -p tui Output: exit 0 · 42ms · 1 stdout line"
            );
        }

        #[test]
        fn tool_summary_formats_failed_bash_json() {
            let summary = summarize_tool_body(&format!(
                "Input: {}\nError: {}",
                serde_json::json!({
                    "command": "cargo test"
                }),
                serde_json::json!({
                    "exit_code": 101,
                    "stdout": "",
                    "stderr": "failed\nsecond\n",
                    "truncated": true,
                    "duration_ms": 900
                })
            ));

            assert_eq!(
                summary,
                "Input: $ cargo test Error: exit 101 · 900ms · 2 stderr lines · truncated"
            );
        }

        #[test]
        fn tool_summary_formats_structured_reviewer_error_without_raw_json() {
            let summary = summarize_tool_body(&format!(
                "Input: {}\nError: {}",
                serde_json::json!({
                    "command": "sleep 120"
                }),
                serde_json::json!({
                    "error": "Operation denied by reviewer: long sleep is not allowed.",
                    "reason": "duplicate internal explanation",
                    "review_denied": true
                })
            ));

            assert_eq!(
                summary,
                "Input: $ sleep 120 Error: Operation denied by reviewer: long sleep is not allowed."
            );
            assert!(!summary.contains("review_denied"));
            assert!(!summary.contains("duplicate internal explanation"));
        }

        #[test]
        fn tool_summary_formats_multiedit_input_and_output_json() {
            let summary = summarize_tool_body(&format!(
                "Input: {}\nOutput: {}",
                serde_json::json!({
                    "file_path": "README.md",
                    "edits": [
                        {"old_string": "a", "new_string": "b"},
                        {"old_string": "c", "new_string": "d"}
                    ]
                }),
                serde_json::json!({
                    "message": "2 edits applied to README.md (0 lines changed)",
                    "path": "README.md",
                    "edits_applied": 2,
                    "lines_changed": 0
                })
            ));

            assert_eq!(
                summary,
                "Input: edit README.md · 2 replacements Output: edited README.md · 2 replacements · 0 lines changed"
            );
        }

        #[test]
        fn tool_summary_formats_grep_content_json() {
            let summary = summarize_tool_body(&format!(
                "Input: {}\nOutput: {}",
                serde_json::json!({
                    "pattern": "status=pending",
                    "path": "README.md",
                    "output_mode": "content"
                }),
                serde_json::json!({
                    "output": "1:status=pending",
                    "match_count": 1
                })
            ));

            assert_eq!(
                summary,
                "Input: grep README.md · status=pending Output: 1 match"
            );
        }

        #[test]
        fn tool_summary_formats_grep_files_json() {
            let summary = summarize_tool_body(&format!(
                "Input: {}\nOutput: {}",
                serde_json::json!({
                    "pattern": "status=",
                    "path": "/private/tmp/work",
                    "output_mode": "files_with_matches"
                }),
                serde_json::json!({
                    "files": ["/private/tmp/work/README.md", "/private/tmp/work/TODO.md"],
                    "total": 2
                })
            ));

            assert_eq!(
                summary,
                "Input: grep files in work · status= Output: 2 matching files"
            );
        }

        #[test]
        fn tool_summary_formats_grep_count_json() {
            let summary = summarize_tool_body(&format!(
                "Input: {}\nOutput: {}",
                serde_json::json!({
                    "pattern": "TODO",
                    "output_mode": "count"
                }),
                serde_json::json!({
                    "counts": [
                        {"file": "/private/tmp/work/README.md", "count": 3}
                    ],
                    "total": 1
                })
            ));

            assert_eq!(
                summary,
                "Input: grep count in workspace · TODO Output: 1 file with matches"
            );
        }

        #[test]
        fn tool_summary_formats_patch_input_and_output_json() {
            let summary = summarize_tool_body(&format!(
                "Input: {}\nOutput: {}",
                serde_json::json!({
                    "patch": "*** Update File: README.md\n-old\n+new\n*** Add File: notes/TODO.md\n+todo"
                }),
                serde_json::json!({
                    "results": [
                        "Updated: /private/tmp/work/README.md",
                        "Created: /private/tmp/work/notes/TODO.md"
                    ]
                })
            ));

            assert_eq!(
                summary,
                "Input: patch 2 files · README.md, TODO.md Output: patched 2 files · README.md, TODO.md"
            );
        }

        #[test]
        fn tool_summary_formats_patch_plain_text_error() {
            let summary = summarize_tool_body(&format!(
                "Input: {}\nError: {}",
                serde_json::json!({
                    "patch": "*** Update File: README.md\n-status=pending\n+status=patch_checked"
                }),
                "File README.md has not been read. Read it before patching."
            ));

            assert_eq!(
                summary,
                "Input: patch 1 file · README.md Error: File README.md has not been read. Read it before patching."
            );
        }

        #[test]
        fn tool_summary_formats_large_patch_without_raw_patch_text() {
            let summary = summarize_tool_body(&format!(
                "Input: {}",
                serde_json::json!({
                    "patch": "*** Update File: a.txt\n-a\n+b\n*** Update File: b.txt\n-a\n+b\n*** Delete File: c.txt\n*** Add File: d.txt\n+d"
                })
            ));

            assert_eq!(
                summary,
                "Input: patch 4 files · a.txt, b.txt, c.txt +1 more"
            );
            assert!(!summary.contains("*** Update File"));
        }

        #[test]
        fn footer_uses_pending_session_model_when_no_persisted_session() {
            let mut state = AppState::empty();
            state.set_pending_session(Some(PendingSessionState::new(
                "agent-1".to_string(),
                "Agent".to_string(),
                "gpt-5.4".to_string(),
            )));

            assert_eq!(footer_status_line(&state), "codex · gpt-5.4");
        }

        #[test]
        fn active_titles_include_typing_subtitle_inline() {
            let cell = TranscriptCell {
                kind: TranscriptCellKind::Assistant,
                title: "Default Assistant".to_string(),
                subtitle: Some("typing…".to_string()),
                body: "hello".to_string(),
                group: MessageGroup::Conversation,
                is_active: true,
            };
            assert_eq!(format_title(&cell), "Default Assistant · typing…");
        }

        #[test]
        fn tool_titles_hide_internal_call_ids() {
            let inactive = TranscriptCell {
                kind: TranscriptCellKind::Tool,
                title: "Tool · bash".to_string(),
                subtitle: Some("#call_08b6e0db075f424386babde3".to_string()),
                body: "Output: ok".to_string(),
                group: MessageGroup::ToolActivity,
                is_active: false,
            };
            assert_eq!(format_title(&inactive), "Tool · bash");

            let active = TranscriptCell {
                subtitle: Some("#call_08b6e0db075f424386babde3 · running... 6s".to_string()),
                is_active: true,
                ..inactive
            };
            assert_eq!(format_title(&active), "Tool · bash · running... 6s");
        }

        #[test]
        fn tool_titles_render_input_after_tool_name() {
            let cell = TranscriptCell {
                kind: TranscriptCellKind::Tool,
                title: "Tool · bash".to_string(),
                subtitle: Some("#call-1 · running... 2s".to_string()),
                body: format!(
                    "Input: {}",
                    serde_json::json!({"command": "ls   -la  /tmp"})
                ),
                group: MessageGroup::ToolActivity,
                is_active: true,
            };

            assert_eq!(
                format_title(&cell),
                "Tool · bash 'ls -la /tmp' · running... 2s"
            );
        }

        #[test]
        fn transcript_lines_style_user_assistant_and_tool_titles() {
            let lines = super::build_cell_lines(
                &[
                    TranscriptCell {
                        kind: TranscriptCellKind::User,
                        title: "You".to_string(),
                        subtitle: None,
                        body: "hello".to_string(),
                        group: MessageGroup::Conversation,
                        is_active: false,
                    },
                    TranscriptCell {
                        kind: TranscriptCellKind::Assistant,
                        title: "Agent".to_string(),
                        subtitle: None,
                        body: "hi".to_string(),
                        group: MessageGroup::Conversation,
                        is_active: false,
                    },
                    TranscriptCell {
                        kind: TranscriptCellKind::Tool,
                        title: "Tool · bash".to_string(),
                        subtitle: None,
                        body: "ok".to_string(),
                        group: MessageGroup::ToolActivity,
                        is_active: false,
                    },
                ],
                80,
            );

            assert_eq!(lines[0].spans[0].style.fg, Some(Color::LightBlue));
            assert!(
                lines[0].spans[0]
                    .style
                    .add_modifier
                    .contains(Modifier::BOLD)
            );
            assert_eq!(lines[1].spans[1].style.fg, None);
            assert!(
                !lines[1].spans[1]
                    .style
                    .add_modifier
                    .contains(Modifier::BOLD)
            );
            assert_eq!(lines[3].spans[0].style.fg, Some(Color::Yellow));
            assert!(
                lines[3].spans[0]
                    .style
                    .add_modifier
                    .contains(Modifier::BOLD)
            );
            assert_eq!(lines[4].spans[1].style.fg, None);
            assert!(
                !lines[4].spans[1]
                    .style
                    .add_modifier
                    .contains(Modifier::BOLD)
            );
            assert_eq!(lines[6].spans[0].style.fg, Some(Color::Cyan));
            assert!(
                lines[6].spans[0]
                    .style
                    .add_modifier
                    .contains(Modifier::BOLD)
            );
        }

        #[test]
        fn active_assistant_title_uses_active_style() {
            let cell = TranscriptCell {
                kind: TranscriptCellKind::Assistant,
                title: "Agent".to_string(),
                subtitle: Some("typing…".to_string()),
                body: "hello".to_string(),
                group: MessageGroup::Conversation,
                is_active: true,
            };

            let style = cell_title_style(&cell);

            assert_eq!(style.fg, Some(Color::Yellow));
            assert!(style.add_modifier.contains(Modifier::BOLD));
        }

        #[test]
        fn write_styled_line_emits_ansi_style_sequences() {
            let line = Line::from(Span::styled(
                "Styled",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ));
            let mut output = Vec::new();

            write_styled_line(&mut output, &line).expect("styled line should render");

            let mut expected_bold = Vec::new();
            queue!(expected_bold, SetAttribute(Attribute::Bold))
                .expect("expected bold sequence should render");
            let rendered = String::from_utf8_lossy(&output);
            assert!(rendered.contains("\u{1b}["));
            assert!(
                output
                    .windows(expected_bold.len())
                    .any(|window| window == expected_bold)
            );
            assert!(rendered.contains("Styled"));
        }

        #[test]
        fn bottom_viewport_renderer_keeps_footer_and_borders() {
            let prompt_lines = vec![Line::from("hello")];
            let rendered =
                render_shell_bottom_viewport(20, Vec::new(), &prompt_lines, 0, 0, "openai · gpt-5");

            assert_eq!(rendered.lines.len(), 3);
            assert!(line_text(&rendered.lines[0]).starts_with('┌'));
            assert!(line_text(&rendered.lines[1]).starts_with('│'));
            assert!(line_text(&rendered.lines[2]).starts_with('└'));
            assert!(line_text(&rendered.lines[2]).contains("openai"));
        }

        #[test]
        fn bottom_viewport_renderer_preserves_cjk_without_spacer_cells() {
            let prompt_lines = vec![Line::from("帮我打开浏览器")];
            let rendered =
                render_shell_bottom_viewport(32, Vec::new(), &prompt_lines, 0, 0, "openai · gpt-5");

            assert!(line_text(&rendered.lines[1]).contains("帮我打开浏览器"));
            assert!(!line_text(&rendered.lines[1]).contains("帮 我"));
        }

        #[test]
        fn clear_visible_homes_cursor_before_redraw() {
            let mut output = Vec::new();
            queue_clear_visible(&mut output).expect("clear sequence should render");
            let ansi = String::from_utf8(output).expect("clear sequence should be utf8");
            assert!(ansi.contains("\u{1b}[2J"));
            assert!(ansi.contains("\u{1b}[1;1H") || ansi.contains("\u{1b}[H"));
        }

        #[test]
        fn purge_visible_and_scrollback_emits_scrollback_clear() {
            let mut output = Vec::new();
            queue_purge_visible_and_scrollback(&mut output).expect("purge sequence should render");
            let ansi = String::from_utf8(output).expect("purge sequence should be utf8");
            assert!(ansi.contains("\u{1b}[2J"));
            assert!(ansi.contains("\u{1b}[3J"));
            assert!(ansi.contains("\u{1b}[1;1H") || ansi.contains("\u{1b}[H"));
        }

        #[test]
        fn stable_history_tail_keeps_user_message_visible() {
            let cells = vec![
                TranscriptCell {
                    kind: TranscriptCellKind::User,
                    title: "You".to_string(),
                    subtitle: None,
                    body: "132".to_string(),
                    group: MessageGroup::Conversation,
                    is_active: false,
                },
                TranscriptCell {
                    kind: TranscriptCellKind::Assistant,
                    title: "Agent".to_string(),
                    subtitle: None,
                    body: "assistant reply".to_string(),
                    group: MessageGroup::Conversation,
                    is_active: false,
                },
            ];

            let rendered = line_texts(&visible_history_tail_lines(&cells, 80, 8));

            assert!(rendered.iter().any(|line| line.contains("You")));
            assert!(rendered.iter().any(|line| line.contains("132")));
            assert!(rendered.iter().any(|line| line.contains("assistant reply")));
        }

        #[test]
        fn first_turn_short_stable_history_stays_bottom_anchored() {
            let cells = vec![
                TranscriptCell {
                    kind: TranscriptCellKind::User,
                    title: "You".to_string(),
                    subtitle: None,
                    body: "hello".to_string(),
                    group: MessageGroup::Conversation,
                    is_active: false,
                },
                TranscriptCell {
                    kind: TranscriptCellKind::Assistant,
                    title: "Agent".to_string(),
                    subtitle: None,
                    body: "OK".to_string(),
                    group: MessageGroup::Conversation,
                    is_active: false,
                },
            ];

            let rendered = line_texts(&visible_history_tail_lines(&cells, 80, 8));

            assert_eq!(rendered.len(), 8);
            assert!(rendered[..2].iter().all(|line| line.is_empty()));
            assert_eq!(rendered[2], "You");
            assert!(rendered.iter().any(|line| line.contains("OK")));
            assert_eq!(rendered.last().map(String::as_str), Some(""));
        }

        #[test]
        fn first_turn_keeps_user_in_stable_history_while_assistant_is_live() {
            let mut live_state = AppState::empty();
            live_state.push_local_user_message("first message stability check".to_string());
            live_state.start_assistant_typing();
            live_state.apply_stream_frame(StreamFrame::Ack {
                content: "FIRST_STABLE_OK".to_string(),
            });

            let live_viewport = build_viewport_snapshot(&live_state, (60, 18));
            let live_lines = line_texts(&live_viewport.lines);
            assert!(!live_lines.iter().any(|line| line == "You"));
            assert!(
                live_lines
                    .iter()
                    .any(|line| line.contains("FIRST_STABLE_OK"))
            );

            let stable_lines = line_texts(&render_history_append_lines(
                &build_stable_history_cells(&live_state),
                60,
            ));
            assert!(stable_lines.iter().any(|line| line == "You"));
            assert!(
                stable_lines
                    .iter()
                    .any(|line| line.contains("first message stability check"))
            );
        }

        #[test]
        fn first_turn_stable_history_uses_normal_tail_when_overflowing() {
            let cells = vec![
                TranscriptCell {
                    kind: TranscriptCellKind::User,
                    title: "You".to_string(),
                    subtitle: None,
                    body: "run lots of output".to_string(),
                    group: MessageGroup::Conversation,
                    is_active: false,
                },
                TranscriptCell {
                    kind: TranscriptCellKind::Tool,
                    title: "Tool · bash".to_string(),
                    subtitle: Some("#call-1".to_string()),
                    body: format!(
                        "Input: {{\"command\":\"ls\"}}\nOutput: {}",
                        (1..=30)
                            .map(|index| format!("line {index}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    ),
                    group: MessageGroup::ToolActivity,
                    is_active: false,
                },
                TranscriptCell {
                    kind: TranscriptCellKind::Assistant,
                    title: "Agent".to_string(),
                    subtitle: None,
                    body: "done".to_string(),
                    group: MessageGroup::Conversation,
                    is_active: false,
                },
            ];

            let rendered = line_texts(&visible_history_tail_lines(&cells, 80, 12));

            assert!(!rendered.iter().any(|line| line == "  ..."));
            assert!(rendered.iter().any(|line| line.contains("line 30")));
            assert!(rendered.iter().any(|line| line.contains("done")));
        }

        #[test]
        fn viewport_stays_anchored_to_bottom() {
            let state = AppState::empty();
            let viewport = build_viewport_snapshot(&state, (40, 10));
            assert_eq!(viewport.top, 7);
            assert_eq!(viewport.cursor_y, 8);
            assert_eq!(viewport.lines.len(), 3);
        }

        #[test]
        fn latest_message_stays_above_composer_rows() {
            let mut state = AppState::empty();
            state.apply_stream_frame(StreamFrame::Ack {
                content: "first\nlatest visible message".to_string(),
            });

            let viewport = build_viewport_snapshot(&state, (60, 10));
            let rendered = line_texts(&viewport.lines);
            let composer_top = rendered
                .iter()
                .position(|line| line.starts_with('┌'))
                .expect("composer top border");
            let latest_row = rendered
                .iter()
                .position(|line| line.contains("latest visible message"))
                .expect("latest message row");

            assert!(latest_row < composer_top);
            assert_eq!(rendered[composer_top - 1], "");
            assert!(
                !rendered[composer_top..]
                    .iter()
                    .any(|line| line.contains("latest visible message"))
            );
        }

        #[test]
        fn overlay_renders_between_message_and_composer() {
            let mut state = AppState::empty();
            state.apply_stream_frame(StreamFrame::Ack {
                content: "live still visible".to_string(),
            });
            state.open_command_picker();

            let viewport = build_viewport_snapshot(&state, (80, 16));
            let rendered = line_texts(&viewport.lines);
            let live_row = rendered
                .iter()
                .position(|line| line.contains("live still visible"))
                .expect("live row");
            let overlay_row = rendered
                .iter()
                .position(|line| line.contains("Slash commands"))
                .expect("overlay row");
            let composer_top = rendered
                .iter()
                .position(|line| line.starts_with('┌'))
                .expect("composer top border");

            assert!(live_row < overlay_row);
            assert!(overlay_row < composer_top);
        }

        #[test]
        fn protected_append_top_uses_current_top_without_previous_viewport() {
            let current = ViewportSnapshot {
                top: 12,
                prompt_top: 12,
                lines: vec![Line::from(""); 3],
                cursor_x: 0,
                cursor_y: 13,
            };

            assert_eq!(protected_append_top(None, &current), 12);
        }

        #[test]
        fn protected_append_top_preserves_previous_streaming_viewport() {
            let previous = ViewportSnapshot {
                top: 8,
                prompt_top: 14,
                lines: vec![Line::from(""); 8],
                cursor_x: 0,
                cursor_y: 15,
            };
            let current = ViewportSnapshot {
                top: 13,
                prompt_top: 13,
                lines: vec![Line::from(""); 3],
                cursor_x: 0,
                cursor_y: 14,
            };

            assert_eq!(protected_append_top(Some(&previous), &current), 8);
        }

        #[test]
        fn protected_append_top_preserves_current_expanded_viewport() {
            let previous = ViewportSnapshot {
                top: 13,
                prompt_top: 13,
                lines: vec![Line::from(""); 3],
                cursor_x: 0,
                cursor_y: 14,
            };
            let current = ViewportSnapshot {
                top: 8,
                prompt_top: 14,
                lines: vec![Line::from(""); 8],
                cursor_x: 0,
                cursor_y: 15,
            };

            assert_eq!(protected_append_top(Some(&previous), &current), 8);
        }

        #[test]
        fn protected_append_top_preserves_previous_prompt_rows_when_prompt_shrinks() {
            let previous = ViewportSnapshot {
                top: 20,
                prompt_top: 20,
                lines: vec![Line::from(""); 4],
                cursor_x: 0,
                cursor_y: 22,
            };
            let current = ViewportSnapshot {
                top: 21,
                prompt_top: 21,
                lines: vec![Line::from(""); 3],
                cursor_x: 0,
                cursor_y: 22,
            };

            assert_eq!(protected_append_top(Some(&previous), &current), 20);
        }

        #[test]
        fn full_redraw_history_top_ignores_previous_full_screen_overlay() {
            let current = ViewportSnapshot {
                top: 33,
                prompt_top: 33,
                lines: vec![Line::from(""); 3],
                cursor_x: 0,
                cursor_y: 34,
            };

            assert_eq!(history_redraw_top(&current), 33);
        }

        #[test]
        fn visible_history_fill_count_fills_padding_before_scrolling() {
            assert_eq!(visible_history_fill_count(0, 10, 4), 4);
            assert_eq!(visible_history_fill_count(7, 10, 5), 3);
            assert_eq!(visible_history_fill_count(10, 10, 5), 0);
            assert_eq!(visible_history_fill_count(12, 10, 5), 0);
        }

        #[test]
        fn stable_history_uses_persisted_conversation_order() {
            let mut state = AppState::empty();
            state.conversation_cells.push(TranscriptCell {
                kind: TranscriptCellKind::User,
                title: "You".to_string(),
                subtitle: None,
                body: "hello".to_string(),
                group: MessageGroup::Conversation,
                is_active: false,
            });
            state.conversation_cells.push(TranscriptCell {
                kind: TranscriptCellKind::Assistant,
                title: "Agent".to_string(),
                subtitle: None,
                body: "hi".to_string(),
                group: MessageGroup::Conversation,
                is_active: false,
            });

            let cells = build_stable_history_cells(&state);
            assert_eq!(cells[0].kind, TranscriptCellKind::User);
            assert_eq!(cells[1].kind, TranscriptCellKind::Assistant);
        }

        #[test]
        fn stable_history_includes_runtime_tool_and_notice_cells() {
            let mut state = AppState::empty();
            state.runtime_cells.push(AnchoredRuntimeCell {
                base_cell_index: 0,
                cell: TranscriptCell {
                    kind: TranscriptCellKind::Tool,
                    title: "Tool · switch_model".to_string(),
                    subtitle: None,
                    body: "{\"ok\":true}".to_string(),
                    group: MessageGroup::ToolActivity,
                    is_active: false,
                },
            });
            state.runtime_cells.push(AnchoredRuntimeCell {
                base_cell_index: 0,
                cell: TranscriptCell {
                    kind: TranscriptCellKind::Notice,
                    title: "Info".to_string(),
                    subtitle: None,
                    body: "Listed sessions".to_string(),
                    group: MessageGroup::RuntimeNotice,
                    is_active: false,
                },
            });

            let cells = build_stable_history_cells(&state);

            assert_eq!(cells.len(), 2);
            assert_eq!(cells[0].kind, TranscriptCellKind::Tool);
            assert_eq!(cells[1].kind, TranscriptCellKind::Notice);
        }

        #[test]
        fn stable_history_keeps_non_streaming_team_runtime_summary_cells() {
            let mut state = AppState::empty();
            state.runtime_cells.push(AnchoredRuntimeCell {
                base_cell_index: 0,
                cell: TranscriptCell {
                    kind: TranscriptCellKind::Subagent,
                    title: "Subagent".to_string(),
                    subtitle: Some("#call-team".to_string()),
                    body: "Starting 1 subagent".to_string(),
                    group: MessageGroup::ToolActivity,
                    is_active: false,
                },
            });

            let cells = build_stable_history_cells(&state);

            assert_eq!(cells.len(), 1);
            assert_eq!(cells[0].kind, TranscriptCellKind::Subagent);
            assert!(cells[0].body.contains("Starting 1 subagent"));
        }

        #[test]
        fn live_tool_cells_render_as_separate_chat_entries() {
            let mut state = AppState::empty();
            state.push_local_user_message("check disk".to_string());
            state.apply_stream_frame(StreamFrame::ToolCall {
                id: "call-df".to_string(),
                name: "bash".to_string(),
                arguments: serde_json::json!({"command":"df -h | grep -i samsung"}),
            });
            state.apply_stream_frame(StreamFrame::ToolCall {
                id: "call-ls".to_string(),
                name: "bash".to_string(),
                arguments: serde_json::json!({"command":"ls"}),
            });

            let lines = line_texts(&super::build_message_lines(&state, 100, 12));
            let first_tool_index = lines
                .iter()
                .position(|line| line.contains("Tool · bash 'df -h"))
                .expect("first tool title");
            let second_tool_index = lines
                .iter()
                .position(|line| line.contains("Tool · bash 'ls'"))
                .expect("second tool title");

            assert!(first_tool_index < second_tool_index);
            assert_eq!(
                lines
                    .iter()
                    .filter(|line| line.starts_with("Tool · bash"))
                    .count(),
                2
            );
            assert!(!lines.iter().any(|line| line.contains("#call-")));
            assert!(!lines.iter().any(|line| line.contains("Input:")));
            assert!(!lines.iter().any(|line| line.contains("Tool activity")));
        }

        #[test]
        fn stable_history_keeps_runtime_cells_between_user_and_final_assistant() {
            let mut state = AppState::empty();
            state.conversation_cells.push(TranscriptCell {
                kind: TranscriptCellKind::User,
                title: "You".to_string(),
                subtitle: None,
                body: "run a tool".to_string(),
                group: MessageGroup::Conversation,
                is_active: false,
            });
            state.runtime_cells.push(AnchoredRuntimeCell {
                base_cell_index: 0,
                cell: TranscriptCell {
                    kind: TranscriptCellKind::Tool,
                    title: "Tool · bash".to_string(),
                    subtitle: None,
                    body: "ok".to_string(),
                    group: MessageGroup::ToolActivity,
                    is_active: false,
                },
            });
            state.conversation_cells.push(TranscriptCell {
                kind: TranscriptCellKind::Assistant,
                title: "Agent".to_string(),
                subtitle: None,
                body: "done".to_string(),
                group: MessageGroup::Conversation,
                is_active: false,
            });

            let cells = build_stable_history_cells(&state);

            assert_eq!(
                cells.iter().map(|cell| cell.kind).collect::<Vec<_>>(),
                vec![
                    TranscriptCellKind::User,
                    TranscriptCellKind::Tool,
                    TranscriptCellKind::Assistant,
                ]
            );
        }

        #[test]
        fn stable_history_keeps_runtime_cells_after_persisted_user() {
            let mut state = AppState::empty();
            state.conversation_cells.push(TranscriptCell {
                kind: TranscriptCellKind::User,
                title: "You".to_string(),
                subtitle: None,
                body: "run a tool".to_string(),
                group: MessageGroup::Conversation,
                is_active: false,
            });
            state.conversation_cells.push(TranscriptCell {
                kind: TranscriptCellKind::Assistant,
                title: "Agent".to_string(),
                subtitle: None,
                body: "done".to_string(),
                group: MessageGroup::Conversation,
                is_active: false,
            });
            state.runtime_cells.push(AnchoredRuntimeCell {
                base_cell_index: 0,
                cell: TranscriptCell {
                    kind: TranscriptCellKind::Tool,
                    title: "Tool · bash".to_string(),
                    subtitle: None,
                    body: "ok".to_string(),
                    group: MessageGroup::ToolActivity,
                    is_active: false,
                },
            });

            let cells = build_stable_history_cells(&state);

            assert_eq!(
                cells.iter().map(|cell| cell.kind).collect::<Vec<_>>(),
                vec![
                    TranscriptCellKind::User,
                    TranscriptCellKind::Tool,
                    TranscriptCellKind::Assistant,
                ]
            );
        }

        #[test]
        fn stable_history_keeps_session_running_projection_without_local_tool() {
            let mut session = ChatSession::new("agent-1".to_string(), "model".to_string());
            session.record_turn_user_message("turn-1", "previous");
            session.complete_turn_with_assistant_message("turn-1", "done");
            session.record_turn_user_message("turn-2", "current");
            session.record_turn_event(
                "turn-2",
                ChatTurnEventKind::ToolCall {
                    call_id: "call-1".to_string(),
                    name: "file".to_string(),
                    arguments: "{}".to_string(),
                },
            );
            let mut state = AppState::empty();
            state.set_current_session(session);
            state.push_local_user_message("current".to_string());

            let cells = build_stable_history_cells(&state);

            assert_eq!(
                cells
                    .iter()
                    .map(|cell| cell.body.as_str())
                    .collect::<Vec<_>>(),
                vec!["previous", "done", "current", "Input: {}"]
            );
        }

        #[test]
        fn stable_history_excludes_pending_legacy_user_when_active_turn_is_visible() {
            let mut session = ChatSession::new("agent-1".to_string(), "model".to_string());
            session.messages.push(ChatMessage::user("current"));
            let mut state = AppState::empty();
            state.set_current_session(session);
            state.push_local_user_message("current".to_string());
            state.start_assistant_typing();

            let cells = build_stable_history_cells(&state);

            assert_eq!(
                cells
                    .iter()
                    .map(|cell| cell.body.as_str())
                    .collect::<Vec<_>>(),
                vec!["current"]
            );
        }

        #[test]
        fn stable_history_keeps_previous_same_text_when_active_turn_is_not_persisted() {
            let mut session = ChatSession::new("agent-1".to_string(), "model".to_string());
            session.record_turn_user_message("turn-1", "repeat");
            session.complete_turn_with_assistant_message("turn-1", "done");
            let mut state = AppState::empty();
            state.set_current_session(session);
            state.push_local_user_message("repeat".to_string());

            let cells = build_stable_history_cells(&state);

            assert_eq!(
                cells
                    .iter()
                    .map(|cell| cell.body.as_str())
                    .collect::<Vec<_>>(),
                vec!["repeat", "done"]
            );
        }

        #[test]
        fn transient_view_only_contains_active_assistant() {
            let mut state = AppState::empty();
            state.runtime_cells.push(AnchoredRuntimeCell {
                base_cell_index: 0,
                cell: TranscriptCell {
                    kind: TranscriptCellKind::Notice,
                    title: "Info".to_string(),
                    subtitle: None,
                    body: "This should be committed history".to_string(),
                    group: MessageGroup::RuntimeNotice,
                    is_active: false,
                },
            });
            state.apply_stream_frame(StreamFrame::Ack {
                content: "streaming".to_string(),
            });

            let lines = build_transient_lines(&state, 80, 8);

            let rendered = line_texts(&lines);
            assert!(rendered.iter().any(|line| line.contains("typing")));
            assert!(
                !rendered
                    .iter()
                    .any(|line| line.contains("committed history"))
            );
        }

        #[test]
        fn slash_command_dropdown_lists_all_commands_when_composer_starts_with_slash() {
            let mut state = AppState::empty();
            state.composer.insert_char('/');
            state.open_command_picker();

            let lines = build_transient_lines(&state, 100, 32);
            let rendered = line_texts(&lines);

            assert_eq!(rendered.len(), SLASH_COMMAND_SPECS.len() + 1);
            assert!(rendered[0].contains("Slash commands"));
            assert!(rendered.iter().any(|line| line.contains("/daemon")));
            assert!(!rendered.iter().any(|line| line.contains("/start")));
            assert!(!rendered.iter().any(|line| line.contains("/stop")));
            assert!(rendered.iter().any(|line| line.contains("/resume")));
            assert!(rendered.iter().any(|line| line.contains("/skill")));
            assert!(!rendered.iter().any(|line| line.contains("/task")));
            assert!(
                !rendered
                    .iter()
                    .any(|line| line.contains("/session open <session_id>"))
            );
            assert!(rendered.iter().any(|line| line.contains("/runs")));
            assert!(
                !rendered
                    .iter()
                    .any(|line| line.contains("/run open <run_id>"))
            );
            assert!(
                !rendered
                    .iter()
                    .any(|line| line.contains("/reject <approval_id> [reason]"))
            );
        }

        #[test]
        fn slash_command_dropdown_uses_command_title_style() {
            let mut state = AppState::empty();
            state.composer.insert_char('/');
            state.open_command_picker();

            let lines = build_transient_lines(&state, 100, 32);

            assert_eq!(lines[1].spans[0].style.fg, Some(Color::Cyan));
            assert!(lines[1].spans[0].content.contains("/daemon"));
            assert!(
                lines[1].spans[0]
                    .style
                    .add_modifier
                    .contains(Modifier::BOLD)
            );
        }

        #[test]
        fn help_overlay_renders_as_transient_content() {
            let mut state = AppState::empty();
            state.open_help_overlay();

            let lines = build_transient_lines(&state, 100, 32);
            let rendered = line_texts(&lines);

            assert!(rendered[0].contains("Help"));
            assert!(
                rendered
                    .iter()
                    .any(|line| line.contains("RestFlow terminal shell"))
            );
            assert!(rendered.iter().any(|line| line.contains("/daemon")));
            assert!(rendered.iter().any(|line| line.contains("/skill")));
            assert!(!rendered.iter().any(|line| line.starts_with("Info")));
        }

        #[test]
        fn slash_command_dropdown_is_not_shown_for_plain_input() {
            let mut state = AppState::empty();
            state.composer.insert_char('h');
            state.composer.insert_char('i');

            let lines = build_transient_lines(&state, 100, 32);

            assert!(lines.is_empty());
        }

        #[test]
        fn resume_picker_lists_sessions_with_last_message_preview() {
            let mut state = AppState::empty();
            state.sessions = vec![
                ChatSessionSummary {
                    id: "session-1".to_string(),
                    name: "First chat".to_string(),
                    agent_id: "agent-1".to_string(),
                    provider: "provider".to_string(),
                    model: "model".to_string(),
                    skill_id: None,
                    message_count: 2,
                    updated_at: 1,
                    last_message_preview: Some("hello\nworld".to_string()),
                    archived_at: None,
                },
                ChatSessionSummary {
                    id: "session-2".to_string(),
                    name: "Second chat".to_string(),
                    agent_id: "agent-1".to_string(),
                    provider: "provider".to_string(),
                    model: "model".to_string(),
                    skill_id: None,
                    message_count: 0,
                    updated_at: 2,
                    last_message_preview: None,
                    archived_at: None,
                },
            ];
            state.open_session_picker();

            let lines = build_transient_lines(&state, 120, 16);
            let rendered = line_texts(&lines);

            assert!(rendered[0].contains("Resume session"));
            assert!(rendered.iter().any(|line| line.contains("First chat")));
            assert!(
                rendered
                    .iter()
                    .any(|line| line.contains("Last: hello world"))
            );
            assert!(rendered.iter().any(|line| line.contains("2 chat messages")));
            assert!(rendered.iter().any(|line| line.contains("0 chat messages")));
            assert!(rendered.iter().any(|line| line.contains("id: session-1")));
            assert!(rendered.iter().any(|line| line.contains("No messages yet")));
        }

        #[test]
        fn completed_tool_only_turn_moves_to_stable_history() {
            let mut state = AppState::empty();
            state.push_local_user_message("coordinate team".to_string());
            state.start_assistant_typing();
            state.apply_stream_frame(StreamFrame::ToolCall {
                id: "call-team".to_string(),
                name: "spawn_subagent_batch".to_string(),
                arguments: serde_json::json!({"specs":[{"task":"reply ok"}]}),
            });
            state.apply_stream_frame(StreamFrame::ToolResult {
                id: "call-team".to_string(),
                success: true,
                result: serde_json::json!({"status":"completed"}).to_string(),
            });
            state.apply_stream_frame(StreamFrame::Done { total_tokens: None });

            let text = line_texts(&render_history_append_lines(
                &build_stable_history_cells(&state),
                100,
            ))
            .join("\n");

            assert!(text.contains("coordinate team"));
            assert!(text.contains("completed"));
            assert!(!text.contains("call-team"));
            assert!(line_texts(&super::build_message_lines(&state, 100, 12)).is_empty());
        }

        #[test]
        fn skill_manager_lists_grouped_skills() {
            let mut state = AppState::empty();
            state.skills = vec![
                SkillPickerItem {
                    id: "team".to_string(),
                    name: "Team".to_string(),
                    description: Some("Coordinate parallel subagents".to_string()),
                    source: SkillSource::System,
                    read_only: true,
                },
                SkillPickerItem {
                    id: "regex-finder".to_string(),
                    name: "Regex Finder".to_string(),
                    description: Some("Find text with regex".to_string()),
                    source: SkillSource::External,
                    read_only: false,
                },
            ];
            state.open_skill_manager();

            let lines = build_transient_lines(&state, 100, 10);
            let text = line_texts(&lines).join("\n");

            assert!(text.contains("Skill Manager"));
            assert!(text.contains("System skills"));
            assert!(text.contains("External skills"));
            assert!(text.contains("Team · team · read-only"));
            assert!(text.contains("Regex Finder"));
        }

        #[test]
        fn skill_mention_picker_lists_matching_skills() {
            let mut state = AppState::empty();
            state.skills = vec![SkillPickerItem {
                id: "team".to_string(),
                name: "Team".to_string(),
                description: Some("Coordinate parallel subagents".to_string()),
                source: SkillSource::System,
                read_only: true,
            }];
            state.composer.replace("use @tea");
            state.open_skill_mention_picker();

            let lines = build_transient_lines(&state, 100, 10);
            let text = line_texts(&lines).join("\n");

            assert!(text.contains("Skill mentions"));
            assert!(text.contains("@team"));
        }

        #[test]
        fn skill_detail_renders_metadata_and_subagent_usage_hint() {
            let mut state = AppState::empty();
            let mut skill = Skill::new(
                "team".to_string(),
                "Team".to_string(),
                Some("Coordinate short-lived parallel subagents".to_string()),
                Some(vec!["system".to_string(), "team".to_string()]),
                "# Team".to_string(),
            );
            skill.source = SkillSource::External;
            skill.read_only = true;
            skill.source_ref = Some("skrun:team@0.1.0".to_string());
            skill.suggested_tools = vec!["spawn_subagent_batch".to_string()];
            state.open_skill_detail(skill);

            let lines = build_transient_lines(&state, 120, 10);
            let text = line_texts(&lines).join("\n");

            assert!(text.contains("Skill"));
            assert!(text.contains("Team · team · read-only"));
            assert!(text.contains("source: external"));
            assert!(text.contains("suggested_tools: spawn_subagent_batch"));
            assert!(text.contains("source_ref: skrun:team@0.1.0"));
            assert!(text.contains("Use this skill by asking for parallel/team/subagent work."));
        }

        #[test]
        fn provider_picker_lists_grouped_providers() {
            let mut state = AppState::empty();
            state.provider_items = vec![
                ProviderPickerItem {
                    provider: "codex".to_string(),
                    label: "codex".to_string(),
                    category: ModelPickerCategory::Recent,
                    usage_count: 2,
                    last_used_at: Some(10),
                    is_current: true,
                },
                ProviderPickerItem {
                    provider: "openai".to_string(),
                    label: "openai".to_string(),
                    category: ModelPickerCategory::Available,
                    usage_count: 0,
                    last_used_at: None,
                    is_current: false,
                },
            ];
            state.open_provider_picker();

            let lines = build_transient_lines(&state, 80, 8);
            let text = line_texts(&lines).join("\n");
            assert!(text.contains("Providers"));
            assert!(text.contains("Recently used providers"));
            assert!(text.contains("codex"));
            assert!(text.contains("current"));
        }

        #[test]
        fn provider_picker_scrolls_to_selected_provider() {
            let mut state = AppState::empty();
            state.provider_items = (0..10)
                .map(|index| ProviderPickerItem {
                    provider: format!("provider-{index}"),
                    label: format!("provider-{index}"),
                    category: ModelPickerCategory::Available,
                    usage_count: 0,
                    last_used_at: None,
                    is_current: false,
                })
                .collect();
            state.open_provider_picker();
            for _ in 0..9 {
                state.move_overlay_selection(1);
            }

            let lines = build_transient_lines(&state, 80, 8);
            let text = line_texts(&lines).join("\n");
            assert!(text.contains("provider-9"));
            assert!(!text.contains("provider-0"));
        }

        #[test]
        fn model_picker_lists_provider_models() {
            let mut state = AppState::empty();
            state.model_items = vec![
                ModelPickerItem {
                    provider: "codex".to_string(),
                    model: "gpt-5.4".to_string(),
                    name: "GPT-5.4".to_string(),
                    category: ModelPickerCategory::Recent,
                    usage_count: 2,
                    last_used_at: Some(10),
                    is_current: true,
                    is_default: true,
                },
                ModelPickerItem {
                    provider: "openai".to_string(),
                    model: "gpt-5".to_string(),
                    name: "GPT-5".to_string(),
                    category: ModelPickerCategory::Available,
                    usage_count: 0,
                    last_used_at: None,
                    is_current: false,
                    is_default: false,
                },
            ];
            state.open_model_picker("codex");

            let lines = build_transient_lines(&state, 80, 8);
            let text = line_texts(&lines).join("\n");
            assert!(text.contains("codex models"));
            assert!(text.contains("Recently used"));
            assert!(text.contains("codex · GPT-5.4"));
            assert!(text.contains("current"));
            assert!(text.contains("default"));
            assert!(text.contains("model: gpt-5.4"));
        }

        #[test]
        fn model_picker_scrolls_to_selected_model() {
            let mut state = AppState::empty();
            state.model_items = (0..8)
                .map(|index| ModelPickerItem {
                    provider: "codex".to_string(),
                    model: format!("model-{index}"),
                    name: format!("Model {index}"),
                    category: ModelPickerCategory::Available,
                    usage_count: 0,
                    last_used_at: None,
                    is_current: false,
                    is_default: false,
                })
                .collect();
            state.open_model_picker("codex");
            for _ in 0..7 {
                state.move_overlay_selection(1);
            }

            let lines = build_transient_lines(&state, 80, 8);
            let text = line_texts(&lines).join("\n");
            assert!(text.contains("Model 7"));
            assert!(text.contains("model: model-7"));
            assert!(!text.contains("Model 0"));
        }

        #[test]
        fn model_picker_uses_full_available_viewport_height() {
            let mut state = AppState::empty();
            state.model_items = (0..14)
                .map(|index| ModelPickerItem {
                    provider: "codex".to_string(),
                    model: format!("model-{index}"),
                    name: format!("Model {index}"),
                    category: ModelPickerCategory::Available,
                    usage_count: 0,
                    last_used_at: None,
                    is_current: false,
                    is_default: false,
                })
                .collect();
            state.open_model_picker("codex");

            let snapshot = build_viewport_snapshot(&state, (120, 34));
            let rendered = line_texts(&snapshot.lines);
            let visible_model_rows = rendered
                .iter()
                .filter(|line| line.contains("Model "))
                .count();

            assert!(visible_model_rows > 4);
            assert!(rendered.iter().any(|line| line.contains("Model 6")));
        }

        #[test]
        fn resume_picker_scrolls_to_selected_session() {
            let mut state = AppState::empty();
            state.sessions = (0..8)
                .map(|index| ChatSessionSummary {
                    id: format!("session-{index}"),
                    name: format!("Session {index}"),
                    agent_id: "agent-1".to_string(),
                    provider: "provider".to_string(),
                    model: "model".to_string(),
                    skill_id: None,
                    message_count: index,
                    updated_at: index as i64,
                    last_message_preview: Some(format!("preview {index}")),
                    archived_at: None,
                })
                .collect();
            state.open_session_picker();
            for _ in 0..6 {
                state.move_overlay_selection(1);
            }

            let lines = build_transient_lines(&state, 120, 7);
            let rendered = line_texts(&lines);

            assert!(rendered.iter().any(|line| line.contains("› Session 6")));
            assert!(!rendered.iter().any(|line| line.contains("Session 0")));
            assert!(rendered.iter().any(|line| line.contains("id: session-6")));
        }

        #[test]
        fn resume_picker_scrolls_before_selected_reaches_bottom() {
            let mut state = AppState::empty();
            state.sessions = (0..8)
                .map(|index| ChatSessionSummary {
                    id: format!("session-{index}"),
                    name: format!("Session {index}"),
                    agent_id: "agent-1".to_string(),
                    provider: "provider".to_string(),
                    model: "model".to_string(),
                    skill_id: None,
                    message_count: index,
                    updated_at: index as i64,
                    last_message_preview: Some(format!("preview {index}")),
                    archived_at: None,
                })
                .collect();
            state.open_session_picker();
            for _ in 0..3 {
                state.move_overlay_selection(1);
            }

            let lines = build_transient_lines(&state, 120, 16);
            let rendered = line_texts(&lines);

            assert!(rendered.iter().any(|line| line.contains("› Session 3")));
            assert!(!rendered.iter().any(|line| line.contains("Session 0")));
            assert!(rendered.iter().any(|line| line.contains("Session 5")));
        }

        #[test]
        fn resume_picker_uses_full_available_viewport_height() {
            let mut state = AppState::empty();
            state.sessions = (0..12)
                .map(|index| ChatSessionSummary {
                    id: format!("session-{index}"),
                    name: format!("Session {index}"),
                    agent_id: "agent-1".to_string(),
                    provider: "provider".to_string(),
                    model: "model".to_string(),
                    skill_id: None,
                    message_count: index,
                    updated_at: index as i64,
                    last_message_preview: Some(format!("preview {index}")),
                    archived_at: None,
                })
                .collect();
            state.open_session_picker();

            let snapshot = build_viewport_snapshot(&state, (120, 30));
            let rendered = line_texts(&snapshot.lines);
            let visible_session_rows = rendered
                .iter()
                .filter(|line| line.contains("Session "))
                .count();

            assert!(
                visible_session_rows > 3,
                "resume picker should not be capped to the compact overlay height"
            );
            assert!(rendered.iter().any(|line| line.contains("Session 6")));
        }

        #[test]
        fn work_picker_uses_full_height_without_showing_run_id() {
            let mut state = AppState::empty();
            state.thread.runs = (0..12)
                .map(|index| types::RunSummary {
                    id: format!("work-{index}"),
                    kind: types::RunKind::WorkspaceRun,
                    container_id: "session-1".to_string(),
                    root_run_id: Some(format!("run-internal-{index}")),
                    title: format!("Work {index}"),
                    subtitle: None,
                    status: types::RunStatus::Running,
                    updated_at: index as i64,
                    started_at: Some(index as i64),
                    ended_at: None,
                    session_id: Some("session-1".to_string()),
                    run_id: Some(format!("run-internal-{index}")),
                    parent_run_id: None,
                    agent_id: Some("agent-1".to_string()),
                    effective_model: None,
                    provider: None,
                    event_count: 0,
                })
                .collect();
            state.open_run_picker();

            let snapshot = build_viewport_snapshot(&state, (120, 30));
            let rendered = line_texts(&snapshot.lines);
            let text = rendered.join("\n");
            let visible_work_rows = rendered
                .iter()
                .filter(|line| line.contains("Work "))
                .count();

            assert!(visible_work_rows > 5);
            assert!(text.contains("Work 6"));
            assert!(!text.contains("run-internal"));
            assert!(!text.contains("run:"));
        }

        #[test]
        fn daemon_picker_lists_start_and_stop_actions() {
            let mut state = AppState::empty();
            state.open_daemon_picker();

            let lines = build_transient_lines(&state, 100, 8);
            let rendered = line_texts(&lines);

            assert!(rendered[0].contains("Daemon"));
            assert!(rendered.iter().any(|line| line.contains("/daemon start")));
            assert!(rendered.iter().any(|line| line.contains("/daemon stop")));
            assert!(rendered[1].contains('›'));
        }

        #[test]
        fn compact_session_preview_collapses_whitespace() {
            assert_eq!(compact_session_preview("hello\n  world"), "hello world");
        }

        #[test]
        fn session_message_count_label_names_chat_messages() {
            assert_eq!(session_message_count_label(0), " · 0 chat messages");
            assert_eq!(session_message_count_label(1), " · 1 chat message");
            assert_eq!(session_message_count_label(2), " · 2 chat messages");
        }

        #[test]
        fn live_viewport_forces_full_redraw_during_active_turn() {
            let mut state = AppState::empty();
            assert!(!should_force_live_viewport_redraw(&state));

            state.push_local_user_message("hello".to_string());

            assert!(should_force_live_viewport_redraw(&state));
        }

        #[test]
        fn transient_view_separates_active_assistant_from_prior_history() {
            let mut state = AppState::empty();
            state.push_local_user_message("hello".to_string());
            state.apply_stream_frame(StreamFrame::Ack {
                content: "streaming".to_string(),
            });

            let lines = build_transient_lines(&state, 80, 8);
            let rendered = line_texts(&lines);

            assert!(rendered.iter().any(|line| line.contains("Agent")));
            assert!(rendered.iter().any(|line| line.contains("typing")));
        }

        #[test]
        fn transient_view_prioritizes_title_when_separator_would_exhaust_space() {
            let mut state = AppState::empty();
            state.push_local_user_message("hello".to_string());
            state.apply_stream_frame(StreamFrame::Ack {
                content: "streaming".to_string(),
            });

            let lines = build_transient_lines(&state, 80, 1);
            let rendered = line_texts(&lines);

            assert_eq!(lines.len(), 1);
            assert!(rendered[0].contains("Agent"));
            assert!(rendered[0].contains("typing"));
        }

        #[test]
        fn transient_view_preserves_active_assistant_title_when_tail_clipped() {
            let mut state = AppState::empty();
            state.apply_stream_frame(StreamFrame::Ack {
                content: "line one\nline two\nline three\nline four".to_string(),
            });

            let lines = build_transient_lines(&state, 80, 3);
            let rendered = line_texts(&lines);

            assert_eq!(lines.len(), 3);
            assert!(rendered[0].contains("Agent"));
            assert!(rendered[0].contains("typing"));
            assert!(!rendered.iter().any(|line| line.contains("line one")));
            assert!(rendered.iter().any(|line| line.contains("line four")));
        }

        #[test]
        fn message_viewport_shows_pending_user_before_stream_start() {
            let mut state = AppState::empty();
            state.push_local_user_message("first message".to_string());
            state.start_assistant_typing();

            let lines = line_texts(&super::build_message_lines(&state, 80, 8));

            assert!(!lines.iter().any(|line| line.contains("You")));
            assert!(!lines.iter().any(|line| line.contains("first message")));
            assert!(lines.iter().any(|line| line.contains("Agent")));
            assert!(lines.iter().any(|line| line.contains("typing")));

            let stable = line_texts(&super::render_history_append_lines(
                &super::build_stable_history_cells(&state),
                80,
            ));
            assert!(stable.iter().any(|line| line.contains("You")));
            assert!(stable.iter().any(|line| line.contains("first message")));
        }

        #[test]
        fn first_live_message_keeps_row_when_assistant_text_starts() {
            let mut state = AppState::empty();
            state.push_local_user_message("first message stability check".to_string());
            state.start_assistant_typing();

            let before = line_texts(&build_viewport_snapshot(&state, (60, 18)).lines);
            state.apply_stream_frame(StreamFrame::Ack {
                content: "FIRST_STABLE_OK".to_string(),
            });
            let after = line_texts(&build_viewport_snapshot(&state, (60, 18)).lines);

            assert_eq!(
                before.iter().position(|line| line.contains("Agent")),
                after.iter().position(|line| line.contains("Agent"))
            );
            assert!(after.iter().any(|line| line.contains("FIRST_STABLE_OK")));
        }

        #[test]
        fn active_user_message_is_stable_not_pinned_to_live_viewport() {
            let mut state = AppState::empty();
            state.push_local_user_message("do not pin me".to_string());
            state.apply_stream_frame(StreamFrame::Ack {
                content: "working".to_string(),
            });

            let live = line_texts(&super::build_message_lines(&state, 80, 8));
            assert!(!live.iter().any(|line| line.contains("do not pin me")));
            assert!(live.iter().any(|line| line.contains("working")));

            let stable = line_texts(&super::render_history_append_lines(
                &super::build_stable_history_cells(&state),
                80,
            ));
            assert!(stable.iter().any(|line| line.contains("do not pin me")));
        }

        #[test]
        fn live_turn_renders_tool_as_a_separate_cell() {
            let mut state = AppState::empty();
            state.apply_stream_frame(StreamFrame::Ack {
                content: "Checking first".to_string(),
            });
            state.apply_stream_frame(StreamFrame::ToolCall {
                id: "call-1".to_string(),
                name: "web_search".to_string(),
                arguments: serde_json::json!({"query": "离骚全文 屈原"}),
            });
            state.apply_stream_frame(StreamFrame::ToolResult {
                id: "call-1".to_string(),
                result: "{\"ok\":true}".to_string(),
                success: true,
            });
            state.apply_stream_frame(StreamFrame::Data {
                content: "Final answer".to_string(),
            });

            let lines = super::build_message_lines(&state, 100, 12);
            let rendered = line_texts(&lines);
            let stable = line_texts(&super::render_history_append_lines(
                &super::build_stable_history_cells(&state),
                100,
            ));

            assert!(rendered.iter().any(|line| line.contains("Agent")));
            assert!(
                !rendered
                    .iter()
                    .any(|line| line == "Tool · web_search '离骚全文 屈原'")
            );
            assert!(
                stable
                    .iter()
                    .any(|line| line == "Tool · web_search '离骚全文 屈原'")
            );
            assert!(!stable.iter().any(|line| line.contains("#call-1")));
            assert!(!stable.iter().any(|line| line.contains("Input:")));
            assert!(rendered.iter().any(|line| line.contains("Final answer")));
        }

        #[test]
        fn live_turn_renders_queued_updates_inside_message_panel() {
            let mut state = AppState::empty();
            state.begin_stream("turn-1".to_string());
            state.push_local_user_message("first".to_string());
            state.queue_active_turn_update("please use the shorter answer".to_string());

            let lines = super::build_message_lines(&state, 100, 12);
            let rendered = line_texts(&lines);

            assert!(!rendered.iter().any(|line| line == "You"));
            assert!(
                rendered
                    .iter()
                    .any(|line| line.contains("Queued update waiting"))
            );
            assert!(
                rendered
                    .iter()
                    .any(|line| line.contains("1. please use the shorter answer"))
            );
            let stable = line_texts(&super::render_history_append_lines(
                &super::build_stable_history_cells(&state),
                100,
            ));
            assert!(stable.iter().any(|line| line == "You"));
            assert!(stable.iter().any(|line| line.contains("first")));
        }

        #[test]
        fn live_tool_activity_expands_bash_stdout_lines() {
            let mut state = AppState::empty();
            state.push_local_user_message("run output command".to_string());
            state.apply_stream_frame(StreamFrame::ToolCall {
                id: "call-1".to_string(),
                name: "bash".to_string(),
                arguments: serde_json::json!({"command": "printf 'LONG_TOOL_1\\nLONG_TOOL_2\\n'"}),
            });
            state.apply_stream_frame(StreamFrame::ToolResult {
                id: "call-1".to_string(),
                result: serde_json::json!({
                    "duration_ms": 3,
                    "exit_code": 0,
                    "stderr": "",
                    "stdout": "LONG_TOOL_1\nLONG_TOOL_2\n",
                    "truncated": false
                })
                .to_string(),
                success: true,
            });

            let rendered = line_texts(&super::render_history_append_lines(
                &super::build_stable_history_cells(&state),
                100,
            ));

            assert!(
                rendered
                    .iter()
                    .any(|line| line.contains("Tool · bash 'printf"))
            );
            assert!(!rendered.iter().any(|line| line.contains("Input:")));
            assert!(rendered.iter().any(|line| line.contains("exit 0")));
            assert!(!rendered.iter().any(|line| line.contains("Output: exit 0")));
            assert!(rendered.iter().any(|line| line.trim() == "stdout:"));
            assert!(rendered.iter().any(|line| line.trim() == "LONG_TOOL_1"));
            assert!(rendered.iter().any(|line| line.trim() == "LONG_TOOL_2"));
            assert!(
                !rendered
                    .iter()
                    .any(|line| line.contains("Output:") && line.contains("\\nLONG_TOOL_2"))
            );
        }

        #[test]
        fn failed_subagent_wait_does_not_leave_live_summary() {
            let mut state = AppState::empty();
            state.begin_stream("turn-1".to_string());
            state.push_local_user_message("wait for subagents".to_string());
            state.apply_stream_frame(StreamFrame::ToolCall {
                id: "wait-call".to_string(),
                name: "wait_subagents".to_string(),
                arguments: serde_json::json!({
                    "task_ids": ["child-1"],
                    "timeout_secs": 1
                }),
            });
            state.apply_stream_frame(StreamFrame::ToolResult {
                id: "wait-call".to_string(),
                success: false,
                result: "Tool error: Tool wait_subagents timed out".to_string(),
            });

            let rendered = line_texts(&super::render_history_append_lines(
                &super::build_stable_history_cells(&state),
                100,
            ));

            assert!(rendered.iter().any(|line| line.contains("Tool error")));
            assert!(
                !rendered
                    .iter()
                    .any(|line| line.contains("Subagents updated"))
            );
        }

        #[test]
        fn live_turn_can_fill_the_full_message_viewport() {
            let mut state = AppState::empty();
            state.apply_stream_frame(StreamFrame::Ack {
                content: (1..=30)
                    .map(|index| format!("line {index}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            });

            let viewport = build_viewport_snapshot(&state, (80, 24));
            let rendered = line_texts(&viewport.lines);

            assert_eq!(viewport.top, 0);
            assert_eq!(viewport.lines.len(), 24);
            assert!(rendered.iter().any(|line| line.contains("line 30")));
            assert!(rendered.iter().any(|line| line.starts_with('┌')));
        }

        #[test]
        fn bottom_live_viewport_uses_normal_scrollback_when_turn_overflows() {
            let mut state = AppState::empty();
            state.push_local_user_message("run lots of output".to_string());
            state.apply_stream_frame(StreamFrame::Ack {
                content: (1..=30)
                    .map(|index| format!("line {index}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            });

            let rendered = line_texts(&super::build_message_lines(&state, 80, 8));

            assert!(!rendered.iter().any(|line| line == "  ..."));
            assert!(rendered.iter().any(|line| line.contains("line 30")));

            let scrolled = line_texts(&super::build_visible_message_lines(
                &state,
                80,
                8,
                usize::MAX,
            ));
            assert!(!scrolled.iter().any(|line| line == "You"));

            let stable = line_texts(&super::render_history_append_lines(
                &super::build_stable_history_cells(&state),
                80,
            ));
            assert!(stable.iter().any(|line| line == "You"));
            assert!(
                stable
                    .iter()
                    .any(|line| line.contains("run lots of output"))
            );
        }

        #[test]
        fn message_viewport_scrolls_only_within_live_turn() {
            let mut state = AppState::empty();
            state.conversation_cells.push(TranscriptCell {
                kind: TranscriptCellKind::Assistant,
                title: "Agent".to_string(),
                subtitle: None,
                body: (1..=20)
                    .map(|index| format!("stable {index}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                group: MessageGroup::Conversation,
                is_active: false,
            });
            state.apply_stream_frame(StreamFrame::Ack {
                content: (1..=20)
                    .map(|index| format!("live {index}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            });

            let bottom = line_texts(&build_viewport_snapshot(&state, (80, 12)).lines);
            assert!(bottom.iter().any(|line| line.contains("live 20")));
            assert!(!bottom.iter().any(|line| line.contains("stable")));

            state.message_scroll_from_bottom = 5;
            let scrolled = line_texts(&build_viewport_snapshot(&state, (80, 12)).lines);
            assert!(!scrolled.iter().any(|line| line.contains("stable")));
            assert!(!scrolled.iter().any(|line| line.contains("live 20")));
            assert!(scrolled.iter().any(|line| line.contains("live 15")));

            state.message_scroll_from_bottom = 16;
            let scrolled_to_top = line_texts(&build_viewport_snapshot(&state, (80, 12)).lines);
            assert!(!scrolled_to_top.iter().any(|line| line.contains("stable")));
            assert!(scrolled_to_top.iter().any(|line| line.contains("live 1")));
        }

        #[test]
        fn preserve_first_line_tail_keeps_header_and_latest_body() {
            let lines = preserve_first_line_tail(
                vec![
                    Line::from("Header"),
                    Line::from("old"),
                    Line::from("middle"),
                    Line::from("new"),
                ],
                3,
            );

            assert_eq!(line_texts(&lines), vec!["Header", "middle", "new"]);
        }

        #[test]
        fn preserve_active_cell_separator_keeps_header_after_separator() {
            let lines = preserve_active_cell_separator(
                vec![
                    Line::from("Header"),
                    Line::from("old"),
                    Line::from("middle"),
                    Line::from("new"),
                ],
                3,
                true,
            );

            assert_eq!(line_texts(&lines), vec!["", "Header", "new"]);
        }

        #[test]
        fn transcript_view_filters_empty_user_cells() {
            let lines = super::build_cell_lines(
                &[
                    TranscriptCell {
                        kind: TranscriptCellKind::User,
                        title: "You".to_string(),
                        subtitle: None,
                        body: String::new(),
                        group: MessageGroup::Conversation,
                        is_active: false,
                    },
                    TranscriptCell {
                        kind: TranscriptCellKind::User,
                        title: "You".to_string(),
                        subtitle: None,
                        body: "hello".to_string(),
                        group: MessageGroup::Conversation,
                        is_active: false,
                    },
                ],
                40,
            );
            let rendered = line_texts(&lines);
            assert_eq!(rendered[0], "You");
            assert_eq!(rendered[1], "  hello");
            assert_eq!(lines.len(), 2);
        }

        #[test]
        fn transcript_wraps_long_body_lines() {
            let lines = super::build_cell_lines(
                &[TranscriptCell {
                    kind: TranscriptCellKind::Assistant,
                    title: "Agent".to_string(),
                    subtitle: None,
                    body: "abcdefghijklmno".to_string(),
                    group: MessageGroup::Conversation,
                    is_active: false,
                }],
                8,
            );

            assert_eq!(
                line_texts(&lines),
                vec!["Agent", "  abcdef", "  ghijkl", "  mno"]
            );
        }

        #[test]
        fn normalize_body_lines_trims_edges_and_compacts_blank_runs() {
            let lines = normalize_body_lines("\n\nhello\n\n\nworld\n\n");
            assert_eq!(lines, vec!["hello", "", "world"]);
        }

        #[test]
        fn append_lines_keep_bottom_spacer_when_history_exists() {
            let cells = vec![TranscriptCell {
                kind: TranscriptCellKind::User,
                title: "You".to_string(),
                subtitle: None,
                body: "hello".to_string(),
                group: MessageGroup::Conversation,
                is_active: false,
            }];
            let lines = render_history_append_lines(&cells, 40);
            let rendered = line_texts(&lines);
            assert_eq!(rendered[0], "You");
            assert_eq!(rendered.last().map(String::as_str), Some(""));
        }

        #[test]
        fn prefix_check_requires_identical_leading_cells() {
            let user = TranscriptCell {
                kind: TranscriptCellKind::User,
                title: "You".to_string(),
                subtitle: None,
                body: "hello".to_string(),
                group: MessageGroup::Conversation,
                is_active: false,
            };
            let assistant = TranscriptCell {
                kind: TranscriptCellKind::Assistant,
                title: "Agent".to_string(),
                subtitle: None,
                body: "hi".to_string(),
                group: MessageGroup::Conversation,
                is_active: false,
            };
            assert!(is_cell_prefix(
                std::slice::from_ref(&user),
                &[user.clone(), assistant.clone()]
            ));
            assert!(!is_cell_prefix(
                std::slice::from_ref(&assistant),
                &[user, assistant.clone()]
            ));
        }

        #[test]
        fn changed_row_indices_only_returns_modified_rows() {
            let previous = vec![Line::from("a"), Line::from("b"), Line::from("c")];
            let current = vec![Line::from("a"), Line::from("x"), Line::from("c")];
            assert_eq!(changed_row_indices(&previous, &current), vec![1]);
        }

        #[test]
        fn bottom_anchor_lines_pads_from_top() {
            let visible = bottom_anchor_lines(vec![Line::from("one"), Line::from("two")], 4, 0);
            assert_eq!(line_texts(&visible), vec!["", "", "one", "two"]);
        }

        #[test]
        fn bottom_anchor_lines_supports_scrollback_offset() {
            let visible = bottom_anchor_lines(
                vec![
                    Line::from("one"),
                    Line::from("two"),
                    Line::from("three"),
                    Line::from("four"),
                    Line::from("five"),
                ],
                2,
                1,
            );

            assert_eq!(line_texts(&visible), vec!["three", "four"]);
        }

        #[test]
        fn clamp_history_scroll_prevents_empty_overscroll() {
            assert_eq!(clamp_history_scroll(5, 2, 99), 3);
            assert_eq!(clamp_history_scroll(2, 5, 99), 0);
        }

        #[test]
        fn preserve_scrolled_offset_keeps_visible_anchor_when_content_changes() {
            assert_eq!(preserve_scrolled_offset(10, 13, 4), 7);
            assert_eq!(preserve_scrolled_offset(13, 10, 7), 4);
            assert_eq!(preserve_scrolled_offset(13, 10, 1), 0);
        }
    }
}

mod slash_command {
    use anyhow::{Result, bail};

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum SlashCommand {
        Daemon,
        NewChat,
        Quit,
        Start,
        Stop,
        Help,
        ListSessions,
        ListSkills,
        ListModels,
        ListModelsForProvider { provider: String },
        ListRuns,
        SwitchModel { model: String },
        SetDefaultModel { model: String },
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SlashCommandSpec {
        pub command: &'static str,
        pub args: &'static str,
        pub description: &'static str,
    }

    pub const SLASH_COMMAND_SPECS: &[SlashCommandSpec] = &[
        SlashCommandSpec {
            command: "/daemon",
            args: "",
            description: "Control the local daemon",
        },
        SlashCommandSpec {
            command: "/new",
            args: "",
            description: "Start a new chat",
        },
        SlashCommandSpec {
            command: "/quit",
            args: "",
            description: "Exit RestFlow",
        },
        SlashCommandSpec {
            command: "/help",
            args: "",
            description: "Show slash command help",
        },
        SlashCommandSpec {
            command: "/resume",
            args: "",
            description: "Resume a previous session",
        },
        SlashCommandSpec {
            command: "/skill",
            args: "",
            description: "View skills",
        },
        SlashCommandSpec {
            command: "/model",
            args: "",
            description: "Switch model or set the default model",
        },
        SlashCommandSpec {
            command: "/runs",
            args: "",
            description: "Show work, runs, and subagents",
        },
    ];

    pub const HELP_TEXT: &str = "RestFlow terminal shell\n\n\
Use /daemon when the daemon is offline.\n\
\
Enter sends the current draft.\n\
Ctrl-J inserts a newline.\n\
Ctrl-P resumes a previous session.\n\
Ctrl-L clears and redraws the screen.\n\
Ctrl-C exits.\n\n\
Slash commands:\n\
/daemon\n\
/new\n\
/quit\n\
/help\n\
/resume\n\
/skill\n\
/model\n\
/model default <model>\n\
	/runs";

    pub fn parse_slash_command(raw: &str) -> Result<SlashCommand> {
        let mut parts = raw.split_whitespace();
        let command = parts.next().unwrap_or_default();
        match command {
            "/daemon" => match parts.next().unwrap_or_default() {
                "" => Ok(SlashCommand::Daemon),
                "start" => Ok(SlashCommand::Start),
                "stop" => Ok(SlashCommand::Stop),
                _ => bail!("Usage: /daemon [start|stop]"),
            },
            "/new" | "/clear" => Ok(SlashCommand::NewChat),
            "/quit" | "/exit" => Ok(SlashCommand::Quit),
            "/start" => Ok(SlashCommand::Start),
            "/stop" => Ok(SlashCommand::Stop),
            "/help" => Ok(SlashCommand::Help),
            "/resume" | "/session" | "/sessions" => Ok(SlashCommand::ListSessions),
            "/skill" => Ok(SlashCommand::ListSkills),
            "/model" => {
                let first = parts.next().unwrap_or_default();
                if first.is_empty() {
                    return Ok(SlashCommand::ListModels);
                }
                if matches!(first, "default" | "global") {
                    let second = parts.next().unwrap_or_default();
                    if second.is_empty() {
                        bail!("Usage: /model default <model> or /model default <provider> <model>");
                    }
                    let third = parts.next().unwrap_or_default();
                    let model = if third.is_empty() {
                        second.to_string()
                    } else {
                        format!("{second}:{third}")
                    };
                    return Ok(SlashCommand::SetDefaultModel { model });
                }
                let second = parts.next().unwrap_or_default();
                if second.is_empty() {
                    return Ok(SlashCommand::ListModelsForProvider {
                        provider: first.to_string(),
                    });
                }
                Ok(SlashCommand::SwitchModel {
                    model: format!("{first}:{second}"),
                })
            }
            "/runs" => Ok(SlashCommand::ListRuns),
            _ => bail!("Unknown command: {command}"),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{SLASH_COMMAND_SPECS, SlashCommand, parse_slash_command};

        #[test]
        fn parses_list_runs_command() {
            assert_eq!(
                parse_slash_command("/runs").expect("parse"),
                SlashCommand::ListRuns
            );
        }

        #[test]
        fn parses_session_listing_commands() {
            assert_eq!(
                parse_slash_command("/sessions").expect("parse"),
                SlashCommand::ListSessions
            );
            assert_eq!(
                parse_slash_command("/session").expect("parse"),
                SlashCommand::ListSessions
            );
        }

        #[test]
        fn parses_model_commands() {
            assert_eq!(
                parse_slash_command("/model").expect("parse"),
                SlashCommand::ListModels
            );
            assert_eq!(
                parse_slash_command("/model gpt-5.4").expect("parse"),
                SlashCommand::ListModelsForProvider {
                    provider: "gpt-5.4".to_string(),
                }
            );
            assert_eq!(
                parse_slash_command("/model codex").expect("parse"),
                SlashCommand::ListModelsForProvider {
                    provider: "codex".to_string(),
                }
            );
            assert_eq!(
                parse_slash_command("/model codex gpt-5.4").expect("parse"),
                SlashCommand::SwitchModel {
                    model: "codex:gpt-5.4".to_string(),
                }
            );
            assert_eq!(
                parse_slash_command("/model default gpt-5.4").expect("parse"),
                SlashCommand::SetDefaultModel {
                    model: "gpt-5.4".to_string(),
                }
            );
            assert_eq!(
                parse_slash_command("/model global codex gpt-5.4").expect("parse"),
                SlashCommand::SetDefaultModel {
                    model: "codex:gpt-5.4".to_string(),
                }
            );
        }

        #[test]
        fn parses_start_command() {
            assert_eq!(
                parse_slash_command("/start").expect("parse"),
                SlashCommand::Start
            );
            assert_eq!(
                parse_slash_command("/daemon start").expect("parse"),
                SlashCommand::Start
            );
        }

        #[test]
        fn parses_stop_command() {
            assert_eq!(
                parse_slash_command("/stop").expect("parse"),
                SlashCommand::Stop
            );
            assert_eq!(
                parse_slash_command("/daemon stop").expect("parse"),
                SlashCommand::Stop
            );
        }

        #[test]
        fn parses_daemon_menu_command() {
            assert_eq!(
                parse_slash_command("/daemon").expect("parse"),
                SlashCommand::Daemon
            );
        }

        #[test]
        fn parses_help_command() {
            assert_eq!(
                parse_slash_command("/help").expect("parse"),
                SlashCommand::Help
            );
        }

        #[test]
        fn parses_quit_aliases() {
            assert_eq!(
                parse_slash_command("/quit").expect("parse"),
                SlashCommand::Quit
            );
            assert_eq!(
                parse_slash_command("/exit").expect("parse"),
                SlashCommand::Quit
            );
        }

        #[test]
        fn rejects_team_as_slash_command() {
            let error = parse_slash_command("/team").expect_err("team is a skill mention");
            assert!(error.to_string().contains("Unknown command: /team"));
        }

        #[test]
        fn parses_new_chat_aliases() {
            assert_eq!(
                parse_slash_command("/new").expect("parse"),
                SlashCommand::NewChat
            );
            assert_eq!(
                parse_slash_command("/clear").expect("parse"),
                SlashCommand::NewChat
            );
        }

        #[test]
        fn command_specs_include_all_supported_entrypoints() {
            let specs = SLASH_COMMAND_SPECS
                .iter()
                .map(|spec| (spec.command, spec.args))
                .collect::<Vec<_>>();

            assert!(specs.contains(&("/daemon", "")));
            assert!(specs.contains(&("/new", "")));
            assert!(specs.contains(&("/quit", "")));
            assert!(!specs.contains(&("/clear", "")));
            assert!(!specs.contains(&("/exit", "")));
            assert!(!specs.contains(&("/start", "")));
            assert!(!specs.contains(&("/stop", "")));
            assert!(specs.contains(&("/help", "")));
            assert!(specs.contains(&("/resume", "")));
            assert!(specs.contains(&("/skill", "")));
            assert!(specs.contains(&("/model", "")));
            assert!(!specs.contains(&("/task", "")));
            assert!(!specs.contains(&("/team", "")));
            assert!(!specs.contains(&("/session", "open <session_id>")));
            assert!(specs.contains(&("/runs", "")));
            assert!(!specs.contains(&("/run", "open <run_id>")));
            assert!(!specs.contains(&("/task", "pause <id>")));
            assert!(!specs.contains(&("/task", "resume <id>")));
            assert!(!specs.contains(&("/task", "stop <id>")));
            assert!(!specs.contains(&("/approve", "<approval_id>")));
            assert!(!specs.contains(&("/reject", "<approval_id> [reason]")));
        }
    }
}

mod state {
    use std::collections::{HashMap, HashSet};

    use ::daemon::StoredAgent;
    use chrono::Utc;
    use types::{
        ChatRole, ChatSession, ChatSessionSummary, ChatTurnEventKind, ChatTurnStatus, ModelId,
        ModelMetadataDTO, Provider, RunKind, RunSummary, Skill, SkillSource,
    };
    use types::{ChatSessionEvent, StreamFrame};

    use super::activity::ActivityState;
    use super::composer::ComposerState;
    use super::transcript::{
        ShellMessage, TranscriptCell, TranscriptCellKind, cell_from_message,
        message_from_session_event, message_from_stream_frame, messages_from_session,
        transcript_cells,
    };

    #[derive(Debug, Clone)]
    pub enum WorkPickerItem {
        Run {
            kind: RunKind,
            title: String,
            status: String,
        },
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
        pub is_default: bool,
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

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub enum InputMode {
        #[default]
        Chat,
        Plan,
    }

    impl InputMode {
        pub fn next(self) -> Self {
            match self {
                Self::Chat => Self::Plan,
                Self::Plan => Self::Chat,
            }
        }

        pub fn label(self) -> &'static str {
            match self {
                Self::Chat => "Chat",
                Self::Plan => "Plan",
            }
        }
    }

    #[derive(Debug, Clone, Default)]
    pub struct SessionThreadState {
        pub session: Option<ChatSession>,
        pub runs: Vec<RunSummary>,
    }

    impl SessionThreadState {
        pub fn session_id(&self) -> Option<&str> {
            self.session.as_ref().map(|session| session.id.as_str())
        }

        pub fn set_session(&mut self, session: ChatSession) {
            self.session = Some(session);
            self.runs.clear();
        }

        pub fn clear_session(&mut self) {
            self.session = None;
            self.runs.clear();
        }

        pub fn set_session_runs(&mut self, runs: Vec<RunSummary>) {
            self.runs = runs;
        }
    }

    #[derive(Debug, Clone)]
    pub enum OverlayState {
        CommandPicker { selected: usize },
        DaemonPicker { selected: usize },
        SessionPicker { selected: usize },
        SkillManager { selected: usize },
        SkillMentionPicker { selected: usize },
        SkillDetail,
        ProviderPicker { selected: usize },
        ModelPicker { provider: String, selected: usize },
        RunPicker { selected: usize },
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
        // Active turn is the latest live viewport. Stable history comes from session projection only.
        pub active_turn: Option<ActiveTurn>,
        active_turn_session_id: Option<String>,
        pub(crate) active_turn_runtime_start: Option<usize>,
        pending_runtime_refresh_session_id: Option<String>,
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
                provider_items: Vec::new(),
                model_items: Vec::new(),
                available_models: Vec::new(),
                pending_session: None,
                conversation_cells: Vec::new(),
                runtime_cells: Vec::new(),
                activity: ActivityState::default(),
                active_turn: None,
                active_turn_session_id: None,
                active_turn_runtime_start: None,
                pending_runtime_refresh_session_id: None,
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
                .or(self.pending_runtime_refresh_session_id.as_deref())
                .or_else(|| self.current_session_id())
        }

        pub fn pending_runtime_refresh_session_id(&self) -> Option<&str> {
            self.pending_runtime_refresh_session_id.as_deref()
        }

        pub fn active_turn_has_tool_call(&self) -> bool {
            self.active_turn
                .as_ref()
                .is_some_and(|turn| turn.cells.iter().any(|cell| cell.tool_call_id().is_some()))
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
            // Keep the TUI boundary value-owned for now. Most session clones in this
            // crate are test setup or snapshot handoff points; moving state to Arc
            // should wait for evidence that clone cost is a real hot path.
            self.thread.set_session(session.clone());
            self.pending_session = None;
            self.runtime_cells.clear();
            self.activity.clear();
            self.clear_active_response();
            self.pending_runtime_refresh_session_id = None;
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
                self.finish_active_assistant_segment();
                self.flush_active_turn_to_runtime();
            }
            let old_cells = std::mem::take(&mut self.conversation_cells);
            self.thread.session = Some(session.clone());
            self.replace_session_projection(messages_from_session(&session));
            self.reanchor_runtime_cells(&old_cells);
            self.reconcile_runtime_conversation_cells();
            if self.pending_runtime_refresh_session_id.as_deref() == Some(session.id.as_str()) {
                self.pending_runtime_refresh_session_id = None;
            }
        }

        fn current_stream_finished_in_session(&self, session: &ChatSession) -> bool {
            let Some(stream_id) = self.current_stream_id.as_deref() else {
                return false;
            };
            session.turns.iter().any(|turn| {
                turn.id == stream_id
                    && matches!(
                        turn.status,
                        ChatTurnStatus::Completed
                            | ChatTurnStatus::Canceled
                            | ChatTurnStatus::Failed
                    )
            })
        }

        fn active_turn_finished_in_session(&self, session: &ChatSession) -> bool {
            let active_tool_call_ids = self.current_turn_tool_call_ids();
            let Some(active_user) = self.current_turn_user_content() else {
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
            if !self.current_turn_tool_call_ids().is_empty() {
                return false;
            }
            let Some(active_user) = self.current_turn_user_content() else {
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
            if self.current_turn_has_response_content() {
                return false;
            }
            let Some(active_user) = self.current_turn_user_content() else {
                return false;
            };
            messages_from_session(session).iter().any(|message| {
                matches!(
                    message,
                    ShellMessage::UserMessage { content } if content.trim_end() == active_user
                )
            })
        }

        fn current_turn_user_content(&self) -> Option<&str> {
            self.active_turn_runtime_cells()
                .and_then(|runtime_cells| {
                    runtime_cells
                        .iter()
                        .rev()
                        .find(|entry| entry.cell.kind == TranscriptCellKind::User)
                        .map(|entry| entry.cell.body.trim_end())
                })
                .or_else(|| {
                    self.active_turn.as_ref().and_then(|active_turn| {
                        active_turn
                            .cells
                            .iter()
                            .find(|cell| cell.kind == TranscriptCellKind::User)
                            .map(|cell| cell.body.trim_end())
                    })
                })
        }

        fn current_turn_tool_call_ids(&self) -> Vec<&str> {
            let mut ids = self
                .active_turn_runtime_cells()
                .into_iter()
                .flat_map(|runtime_cells| runtime_cells.iter())
                .filter_map(|entry| entry.cell.tool_call_id())
                .collect::<Vec<_>>();
            if let Some(active_turn) = self.active_turn.as_ref() {
                ids.extend(
                    active_turn
                        .cells
                        .iter()
                        .filter_map(|cell| cell.tool_call_id()),
                );
            }
            ids
        }

        pub fn clear_current_session(&mut self, notice: impl Into<String>) {
            self.thread.clear_session();
            self.replace_session_projection(Vec::new());
            self.runtime_cells.clear();
            self.activity.clear();
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
            self.clear_active_response();
            self.reset_message_scroll();
            self.pending_session = pending_session;
            self.clear_overlay();
            self.composer.clear();
            self.is_streaming = false;
        }

        pub fn clear_overlay(&mut self) {
            self.overlay = None;
            self.pending_session_delete_id = None;
            self.selected_skill = None;
        }

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

        pub fn open_provider_picker(&mut self) {
            self.overlay = Some(OverlayState::ProviderPicker { selected: 0 });
        }

        pub fn open_model_picker(&mut self, provider: impl Into<String>) {
            self.overlay = Some(OverlayState::ModelPicker {
                provider: provider.into(),
                selected: 0,
            });
        }

        pub fn open_run_picker(&mut self) {
            self.overlay = Some(OverlayState::RunPicker { selected: 0 });
        }

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
                | Some(OverlayState::ProviderPicker { selected })
                | Some(OverlayState::ModelPicker { selected, .. })
                | Some(OverlayState::RunPicker { selected }) => {
                    let next =
                        (*selected as isize + delta).clamp(0, len.saturating_sub(1) as isize);
                    *selected = next as usize;
                }
                Some(OverlayState::SkillDetail) | Some(OverlayState::Help) | None => {}
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
                OverlayState::ProviderPicker { .. } => Some(self.provider_items.len()),
                OverlayState::ModelPicker { .. } => Some(self.model_items.len()),
                OverlayState::RunPicker { .. } => Some(self.run_picker_items().len()),
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
            build_work_picker_items(&self.thread.runs)
        }

        pub fn run_picker_items(&self) -> Vec<WorkPickerItem> {
            self.work_picker_items()
        }

        pub fn set_session_runs(&mut self, runs: Vec<RunSummary>) {
            self.thread.set_session_runs(runs);
        }

        pub fn clear_thread_runs(&mut self) {
            self.thread.runs.clear();
            self.activity.clear();
        }
    }

    pub fn build_work_picker_items(runs: &[RunSummary]) -> Vec<WorkPickerItem> {
        let mut items = Vec::new();
        items.extend(runs.iter().filter_map(|run| {
            run.run_id.as_ref().map(|_| WorkPickerItem::Run {
                kind: run.kind,
                title: run.title.clone(),
                status: run.status.to_string(),
            })
        }));
        items
    }

    pub fn work_run_kind_label(kind: RunKind) -> &'static str {
        match kind {
            RunKind::WorkspaceRun => "workspace run",
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
            self.active_turn_runtime_start = None;
            self.active_progress_started_at_ms = None;
            self.active_tool_progress_started_at_ms.clear();
            self.activity.clear();
            self.active_assistant_stream_body.clear();
            self.active_tool_call_ids.clear();
            self.active_tool_result_ids.clear();
        }

        fn finish_active_assistant_segment(&mut self) {
            let Some(mut active_cell) = self.take_active_assistant_cell() else {
                return;
            };
            active_cell.body = active_cell.body.trim_end().to_string();
            if !active_cell.body.trim().is_empty() {
                let _ = active_cell.finalize();
                self.push_current_turn_runtime_cell(active_cell);
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

        pub fn suppress_missing_terminal_frame_for_stream(&mut self, stream_id: &str) -> bool {
            if self.canceled_stream_ids.remove(stream_id) {
                return true;
            }
            self.ignore_stream_frames && self.current_stream_id.as_deref() != Some(stream_id)
        }

        pub fn complete_stream_without_terminal_frame(&mut self, stream_id: &str) -> bool {
            if self.current_stream_id.as_deref() != Some(stream_id)
                || !self.current_turn_has_response_content()
            {
                return false;
            }
            self.is_streaming = false;
            self.current_stream_id = None;
            self.finish_active_assistant_segment();
            self.flush_active_turn_to_runtime();
            self.status = "Stream finished".to_string();
            true
        }

        fn current_turn_has_response_content(&self) -> bool {
            self.active_turn.as_ref().is_some_and(|turn| {
                turn.cells.iter().any(|cell| {
                    !matches!(cell.kind, TranscriptCellKind::User)
                        && (!cell.body.trim().is_empty() || cell.tool_call_id().is_some())
                })
            }) || self
                .active_turn_runtime_cells()
                .is_some_and(runtime_response_after_last_user)
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
                        .map(|call_id| {
                            *tool_started_at.entry(call_id.to_string()).or_insert(now_ms)
                        })
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

        fn take_active_assistant_cell(&mut self) -> Option<TranscriptCell> {
            let active_turn = self.active_turn.as_mut()?;
            let index = active_turn.active_assistant_index.take()?;
            if index >= active_turn.cells.len() {
                return None;
            }
            let cell = active_turn.cells.remove(index);
            if active_turn.cells.is_empty()
                && active_turn.queued_updates.is_empty()
                && !self.is_streaming
            {
                self.active_turn = None;
            }
            Some(cell)
        }

        fn take_active_tool_cell(&mut self, call_id: &str) -> Option<TranscriptCell> {
            let active_turn = self.active_turn.as_mut()?;
            let index = active_turn
                .cells
                .iter()
                .position(|cell| cell.tool_call_id() == Some(call_id))?;
            if active_turn
                .active_assistant_index
                .is_some_and(|assistant_index| {
                    assistant_index > index && assistant_index < active_turn.cells.len()
                })
            {
                active_turn.active_assistant_index =
                    active_turn.active_assistant_index.map(|index| index - 1);
            }
            let cell = active_turn.cells.remove(index);
            if active_turn.cells.is_empty()
                && active_turn.queued_updates.is_empty()
                && !self.is_streaming
            {
                self.active_turn = None;
            }
            Some(cell)
        }

        fn reset_active_progress_if_no_live_cells(&mut self) {
            let has_live_cell = self
                .active_turn
                .as_ref()
                .is_some_and(|turn| turn.cells.iter().any(|cell| cell.is_active));
            if !has_live_cell {
                self.active_progress_started_at_ms = None;
            }
        }

        fn push_current_turn_runtime_cell(&mut self, mut cell: TranscriptCell) -> bool {
            if self.active_turn_runtime_start.is_none() {
                self.active_turn_runtime_start = Some(self.runtime_cells.len());
            }
            if cell.is_active {
                let _ = cell.finalize();
            }
            if matches!(
                cell.kind,
                TranscriptCellKind::Assistant | TranscriptCellKind::User
            ) && cell.body.trim().is_empty()
            {
                return false;
            }
            if self
                .conversation_cells
                .iter()
                .any(|persisted| is_persisted_duplicate_cell(&cell, persisted))
                || self
                    .runtime_cells
                    .iter()
                    .any(|entry| is_persisted_duplicate_cell(&cell, &entry.cell))
            {
                return false;
            }
            let refresh_session_id = self
                .active_turn_session_id
                .clone()
                .or_else(|| self.current_session_id().map(ToOwned::to_owned));
            self.runtime_cells.push(AnchoredRuntimeCell {
                base_cell_index: self.conversation_cells.len(),
                cell,
            });
            self.pending_runtime_refresh_session_id = refresh_session_id;
            true
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
            if let Some(mut cell) = self.take_active_tool_cell(call_id) {
                let _ = cell.merge_tool_result(success, result);
                self.activity
                    .record_tool_result(call_id, success, &cell.body);
                self.active_tool_progress_started_at_ms.remove(call_id);
                self.push_current_turn_runtime_cell(cell);
                self.reset_active_progress_if_no_live_cells();
                return;
            }
            if let Some(entry) = self
                .runtime_cells
                .iter_mut()
                .find(|entry| entry.cell.tool_call_id() == Some(call_id))
            {
                let _ = entry.cell.merge_tool_result(success, result);
                self.activity
                    .record_tool_result(call_id, success, &entry.cell.body);
                self.active_tool_progress_started_at_ms.remove(call_id);
                self.reset_active_progress_if_no_live_cells();
                return;
            }
            let assistant_name = self.assistant_name().to_string();
            let mut cell = cell_from_message(
                &ShellMessage::ToolResult {
                    call_id: call_id.to_string(),
                    success,
                    result: result.to_string(),
                },
                &assistant_name,
            );
            let _ = cell.finalize();
            self.activity
                .record_tool_result(call_id, success, &cell.body);
            self.push_current_turn_runtime_cell(cell);
        }

        pub fn push_local_user_message(&mut self, content: String) {
            self.reset_message_scroll();
            self.last_error_notice = None;
            self.flush_active_turn_to_runtime();
            let cell = cell_from_message(
                &ShellMessage::UserMessage { content },
                self.assistant_name(),
            );
            self.active_turn_session_id = self.current_session_id().map(ToOwned::to_owned);
            self.active_turn_runtime_start = Some(self.runtime_cells.len());
            self.push_current_turn_runtime_cell(cell);
            self.active_turn = Some(ActiveTurn::default());
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
                    self.is_streaming = false;
                    self.current_stream_id = None;
                    self.finish_active_assistant_segment();
                    self.flush_active_turn_to_runtime();
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

        #[cfg(test)]
        pub fn transcript_cells_for_render(&self) -> Vec<TranscriptCell> {
            crate::visual_timeline::VisualTimeline::from_state(self).render_cells()
        }

        fn flush_active_turn_to_runtime(&mut self) {
            let Some(mut active_turn) = self.active_turn.take() else {
                self.clear_active_response();
                return;
            };
            let refresh_session_id = self
                .active_turn_session_id
                .clone()
                .or_else(|| self.current_session_id().map(ToOwned::to_owned));
            let base_cell_index = self.conversation_cells.len();
            let mut pushed_any = false;
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
                pushed_any = true;
            }
            self.clear_active_response();
            if pushed_any {
                self.pending_runtime_refresh_session_id = refresh_session_id;
            }
        }

        fn reconcile_runtime_conversation_cells(&mut self) {
            let active_start = self.active_turn_runtime_start;
            let mut runtime_index = 0usize;
            self.runtime_cells.retain(|entry| {
                let current_index = runtime_index;
                runtime_index += 1;
                if active_start.is_some_and(|start| current_index >= start) {
                    return true;
                }
                let persisted_duplicate = self
                    .conversation_cells
                    .iter()
                    .any(|cell| active_cell_projected_by(&entry.cell, cell));
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
                    && let Some(new_index) =
                        self.conversation_cells.iter().position(|c| c == old_cell)
                {
                    entry.base_cell_index = new_index;
                }
            }
        }

        fn assistant_name(&self) -> &str {
            self.default_agent_name.as_deref().unwrap_or("Agent")
        }

        pub(crate) fn active_turn_runtime_cells(&self) -> Option<&[AnchoredRuntimeCell]> {
            self.active_turn_runtime_start
                .map(|start| &self.runtime_cells[start.min(self.runtime_cells.len())..])
        }

        fn last_runtime_error_matches(&self, content: &str) -> bool {
            let normalized = normalized_error_notice(content);
            self.runtime_cells.iter().rev().any(|entry| {
                entry.cell.title == "Error"
                    && normalized_error_notice(&entry.cell.body) == normalized
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

    fn runtime_response_after_last_user(runtime_cells: &[AnchoredRuntimeCell]) -> bool {
        for entry in runtime_cells.iter().rev() {
            if entry.cell.kind == TranscriptCellKind::User {
                return false;
            }
            if !matches!(entry.cell.kind, TranscriptCellKind::Notice)
                && (!entry.cell.body.trim().is_empty() || entry.cell.tool_call_id().is_some())
            {
                return true;
            }
        }
        false
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
        use super::{AppState, OverlayState};
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
            assert!(tool_subtitles[1].contains("#call-2 · running"));
            assert!(tool_subtitles[1].contains("0s"));
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
            assert_eq!(state.runtime_cells.len(), 2);
            assert!(state.runtime_cells[0].cell.body.contains("Checking..."));
            assert_eq!(state.runtime_cells[1].cell.title, "Tool · bash");
            assert!(state.runtime_cells[1].cell.body.contains("Input:"));
            assert!(state.runtime_cells[1].cell.body.contains("Output:"));
            let active_turn = state.active_turn.as_ref().expect("active turn");
            assert_eq!(active_turn.cells.len(), 1);
            let active = active_turn.cells.last().expect("active assistant");
            assert_eq!(active.kind, TranscriptCellKind::Assistant);
            assert!(active.body.contains("Done."));

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
            let mut session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
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
                types::ChatTurnEventKind::ToolCall {
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
            let mut session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
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
                types::ChatTurnEventKind::ToolCall {
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
        fn work_picker_includes_runs() {
            let mut state = AppState::empty();
            state.thread.runs.push(types::RunSummary {
                id: "run-local".to_string(),
                kind: types::RunKind::WorkspaceRun,
                container_id: "session-1".to_string(),
                root_run_id: Some("run-local".to_string()),
                title: "Run One".to_string(),
                subtitle: None,
                status: types::RunStatus::Running,
                updated_at: 1,
                started_at: Some(1),
                ended_at: None,
                session_id: Some("session-1".to_string()),
                run_id: Some("run-local".to_string()),
                parent_run_id: None,
                agent_id: Some("agent-1".to_string()),
                effective_model: None,
                provider: None,
                event_count: 0,
            });

            let items = state.work_picker_items();
            assert_eq!(items.len(), 1);
            assert!(matches!(
                items[0],
                super::WorkPickerItem::Run {
                    kind: types::RunKind::WorkspaceRun,
                    ..
                }
            ));
        }

        #[test]
        fn refresh_current_session_preserves_notice_messages() {
            let mut state = AppState::empty();
            let mut session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
            session.messages.push(types::ChatMessage::user("hello"));
            state.set_current_session(session.clone());
            state.push_info("notice");

            let mut updated = session.clone();
            updated.messages.push(types::ChatMessage::assistant("hi"));
            state.refresh_current_session(updated);

            assert_eq!(state.conversation_cells.len(), 2);
            assert_eq!(state.runtime_cells.len(), 1);
            assert_eq!(state.runtime_cells[0].cell.title, "Info");
        }

        #[test]
        fn refresh_current_session_keeps_active_turn_until_stream_finishes() {
            let mut state = AppState::empty();
            let session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
            state.set_current_session(session.clone());
            state.push_local_user_message("hello".to_string());

            state.is_streaming = true;
            state.refresh_current_session(session.clone());
            assert!(state.active_turn.as_ref().unwrap().cells.is_empty());
            assert_eq!(state.runtime_cells[0].cell.body, "hello");

            let mut updated = session;
            updated.messages.push(types::ChatMessage::user("hello"));
            state.is_streaming = false;
            state.refresh_current_session(updated);
            assert!(state.active_turn.is_none());
            assert_eq!(state.conversation_cells.len(), 1);
            assert_eq!(state.conversation_cells[0].body, "hello");
        }

        #[test]
        fn refresh_current_session_keeps_streaming_turn_when_user_is_only_persisted_event() {
            let mut state = AppState::empty();
            let mut session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
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
            let mut session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
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
                types::ChatTurnEventKind::ToolCall {
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
        fn refresh_current_session_keeps_completed_runtime_turn_until_session_persists_answer() {
            let mut state = AppState::empty();
            let mut session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
            state.set_current_session(session.clone());
            state.push_local_user_message("hello".to_string());
            state.apply_stream_frame(StreamFrame::Data {
                content: "done".to_string(),
            });
            state.apply_stream_frame(StreamFrame::Done { total_tokens: None });

            session.messages.push(types::ChatMessage::user("hello"));
            state.refresh_current_session(session);

            assert!(state.active_turn.is_none());
            assert_eq!(state.runtime_cells.len(), 1);
            let rendered = state.transcript_cells_for_render();
            assert_eq!(rendered.len(), 2);
            assert_eq!(rendered[0].body, "hello");
            assert_eq!(rendered[1].body, "done");
        }

        #[test]
        fn refresh_current_session_clears_active_turn_when_legacy_messages_project_answer() {
            let mut state = AppState::empty();
            let mut session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
            state.set_current_session(session.clone());
            state.push_local_user_message("hello".to_string());
            state.apply_stream_frame(StreamFrame::Data {
                content: "done".to_string(),
            });
            state.apply_stream_frame(StreamFrame::Done { total_tokens: None });

            session.messages.push(types::ChatMessage::user("hello"));
            session.messages.push(types::ChatMessage::assistant("done"));
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
            let mut session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
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
            let mut session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
            state.set_current_session(session.clone());
            state.begin_stream("stream-1".to_string());
            state.push_local_user_message("hello".to_string());
            state.apply_stream_frame(StreamFrame::Data {
                content: "do".to_string(),
            });

            session.messages.push(types::ChatMessage::user("hello"));
            session.messages.push(types::ChatMessage::assistant("done"));
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
            let mut session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
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
            let mut session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
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
            let mut session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
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
            let mut session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
            state.set_current_session(session.clone());

            state.begin_stream("local-stream-id".to_string());
            state.push_local_user_message("create two tasks\n".to_string());
            state.apply_stream_frame(StreamFrame::ToolCall {
                id: "call-1".to_string(),
                name: "bash".to_string(),
                arguments: serde_json::json!({}),
            });
            assert!(state.active_turn.is_some());

            session.record_turn_user_message("persisted-turn-id", "create two tasks\n");
            session.record_turn_event(
                "persisted-turn-id",
                types::ChatTurnEventKind::ToolCall {
                    call_id: "call-1".to_string(),
                    name: "bash".to_string(),
                    arguments: "{}".to_string(),
                },
            );
            session.record_turn_event(
                "persisted-turn-id",
                types::ChatTurnEventKind::ToolResult {
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
            let mut session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
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
                types::ChatTurnEventKind::ToolCall {
                    call_id: "call-1".to_string(),
                    name: "spawn_subagent_batch".to_string(),
                    arguments: "{}".to_string(),
                },
            );
            session.record_turn_event(
                "persisted-turn-id",
                types::ChatTurnEventKind::ToolResult {
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
                    .any(|cell| cell.kind == TranscriptCellKind::Assistant
                        && cell.body == "parent ok")
            );
        }

        #[test]
        fn refresh_current_session_clears_active_turn_when_persisted_tool_call_matches() {
            let mut state = AppState::empty();
            let mut session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
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
                types::ChatTurnEventKind::ToolCall {
                    call_id: "call-team".to_string(),
                    name: "spawn_subagent_batch".to_string(),
                    arguments: "{}".to_string(),
                },
            );
            session.record_turn_event(
                "persisted-turn-id",
                types::ChatTurnEventKind::ToolResult {
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
            let mut session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
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
            let mut session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
            state.set_current_session(session.clone());

            state.begin_stream("local-stream-id".to_string());
            state.push_local_user_message("coordinate team".to_string());
            state.apply_stream_frame(StreamFrame::Ack {
                content: "parent ok".to_string(),
            });

            session.add_message(types::ChatMessage::user("coordinate team"));
            session.add_message(types::ChatMessage::assistant("parent ok"));

            state.refresh_current_session(session);

            assert!(!state.is_streaming);
            assert!(state.current_stream_id.is_none());
            assert!(state.active_turn.is_none());
        }

        #[test]
        fn pending_user_message_stays_before_local_assistant_finalize() {
            let mut state = AppState::empty();
            let session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
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
        fn daemon_backed_stream_done_moves_completed_turn_to_stable_runtime() {
            let mut state = AppState::empty();
            let session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
            state.set_current_session(session);
            state.push_local_user_message("hello".to_string());
            state.apply_stream_frame(StreamFrame::Ack {
                content: "done".to_string(),
            });

            state.apply_stream_frame(StreamFrame::Done { total_tokens: None });

            assert!(state.active_turn.is_none());
            assert_eq!(state.runtime_cells.len(), 2);
            let rendered = state.transcript_cells_for_render();
            assert_eq!(rendered.len(), 2);
            assert_eq!(rendered[0].kind, TranscriptCellKind::User);
            assert_eq!(rendered[1].kind, TranscriptCellKind::Assistant);
        }

        #[test]
        fn refresh_after_interrupted_stream_preserves_partial_turn_after_user() {
            let mut state = AppState::empty();
            let session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
            state.set_current_session(session.clone());
            state.push_local_user_message("first".to_string());
            state.apply_stream_frame(StreamFrame::Ack {
                content: "partial answer".to_string(),
            });
            state.cancel_active_response();

            let mut updated = session;
            updated.messages.push(types::ChatMessage::user("first"));
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
            let session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
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
            let session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
            state.set_current_session(session.clone());

            state.push_local_user_message("first".to_string());
            state.apply_stream_frame(StreamFrame::error(500, "preflight failed"));

            state.push_local_user_message("second".to_string());
            state.apply_stream_frame(StreamFrame::Ack {
                content: "OK".to_string(),
            });
            state.apply_stream_frame(StreamFrame::Done { total_tokens: None });

            let mut updated = session;
            updated.messages.push(types::ChatMessage::user("second"));
            updated.messages.push(types::ChatMessage::assistant("OK"));
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
            let mut session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
            state.set_current_session(session.clone());
            state.push_error("Stream error 500: Preflight check failed:\n- missing secret");

            session.record_turn_user_message("turn-1", "hello");
            session.record_turn_event(
                "turn-1",
                types::ChatTurnEventKind::Error {
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
            let session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
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

            let mut persisted = types::ChatSession::new("agent-1".to_string(), "model".to_string());
            persisted.record_turn_user_message("turn-1", "hello");
            persisted.record_turn_event(
                "turn-1",
                types::ChatTurnEventKind::ToolCall {
                    call_id: "call-1".to_string(),
                    name: "bash".to_string(),
                    arguments: "{\"command\":\"pwd\"}".to_string(),
                },
            );
            persisted.record_turn_event(
                "turn-1",
                types::ChatTurnEventKind::ToolResult {
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
        fn refresh_flushes_active_assistant_when_completed_projection_only_has_tools() {
            let mut state = AppState::empty();
            let session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
            state.set_current_session(session);
            state.begin_stream("turn-1".to_string());
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
                content: "local final answer".to_string(),
            });

            let mut persisted = types::ChatSession::new("agent-1".to_string(), "model".to_string());
            persisted.record_turn_user_message("turn-1", "hello");
            persisted.record_turn_event(
                "turn-1",
                types::ChatTurnEventKind::ToolCall {
                    call_id: "call-1".to_string(),
                    name: "bash".to_string(),
                    arguments: "{\"command\":\"pwd\"}".to_string(),
                },
            );
            persisted.record_turn_event(
                "turn-1",
                types::ChatTurnEventKind::ToolResult {
                    call_id: "call-1".to_string(),
                    success: true,
                    result: "/tmp".to_string(),
                },
            );
            persisted.complete_turn_with_assistant_message("turn-1", "");

            state.refresh_current_session(persisted);

            assert!(state.active_turn.is_none());
            assert!(state.active_turn_runtime_start.is_none());
            let rendered = state.transcript_cells_for_render();
            assert!(rendered.iter().any(|cell| {
                cell.kind == TranscriptCellKind::Assistant && cell.body == "local final answer"
            }));
        }

        #[test]
        fn refresh_removes_runtime_cells_when_persisted_turn_has_equivalent_live_content() {
            let mut state = AppState::empty();
            let session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
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

            let mut persisted = types::ChatSession::new("agent-1".to_string(), "model".to_string());
            persisted.record_turn_user_message("turn-1", "hello");
            persisted.record_turn_event(
                "turn-1",
                types::ChatTurnEventKind::ToolCall {
                    call_id: "call-1".to_string(),
                    name: "spawn_subagent_batch".to_string(),
                    arguments: "{\"specs\":[{\"task\":\"reply\"}]}".to_string(),
                },
            );
            persisted.record_turn_event(
                "turn-1",
                types::ChatTurnEventKind::ToolResult {
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
            let session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
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

            let mut persisted = types::ChatSession::new("agent-1".to_string(), "model".to_string());
            persisted.record_turn_user_message("turn-1", "run one subagent");
            persisted.record_turn_event(
                "turn-1",
                types::ChatTurnEventKind::ToolCall {
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
                types::ChatTurnEventKind::ToolResult {
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
                types::ChatTurnEventKind::ToolCall {
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
                types::ChatTurnEventKind::ToolResult {
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
            let mut session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
            session.messages.push(types::ChatMessage::user("hello"));
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
            let mut session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
            session.messages.push(types::ChatMessage::user("hello"));

            state.set_current_session(session);

            assert_eq!(state.conversation_cells.len(), 1);
            assert_eq!(state.conversation_cells[0].kind, TranscriptCellKind::User);
            assert!(state.runtime_cells.is_empty());
        }

        #[test]
        fn refresh_current_session_preserves_runtime_cells_and_streaming_active_turn() {
            let mut state = AppState::empty();
            let mut session = types::ChatSession::new("agent-1".to_string(), "model".to_string());
            session.messages.push(types::ChatMessage::user("hello"));
            state.set_current_session(session.clone());
            state.push_info("notice");
            state.is_streaming = true;
            state.apply_stream_frame(StreamFrame::Ack {
                content: "chunk".to_string(),
            });

            let mut updated = session.clone();
            updated
                .messages
                .push(types::ChatMessage::assistant("reply"));
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
}

mod transcript {
    use serde_json::Value;
    use types::{ChatRole, ChatSession, ChatTurnEventKind};
    use types::{ChatSessionEvent, StreamFrame};

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
                TranscriptCellKind::User
                    | TranscriptCellKind::Assistant
                    | TranscriptCellKind::System
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
                Self::InfoNotice { .. } | Self::ErrorNotice { .. } => MessageGroup::RuntimeNotice,
            }
        }
    }

    pub fn messages_from_session(session: &ChatSession) -> Vec<ShellMessage> {
        if session.turns.iter().any(|turn| !turn.events.is_empty()) {
            return session
                .turns
                .iter()
                .flat_map(|turn| {
                    turn.events.iter().map(|event| match &event.kind {
                        ChatTurnEventKind::UserMessage { content } => ShellMessage::UserMessage {
                            content: content.clone(),
                        },
                        ChatTurnEventKind::AssistantMessage { content } => {
                            ShellMessage::AssistantMessage {
                                content: content.clone(),
                            }
                        }
                        ChatTurnEventKind::ToolCall {
                            call_id,
                            name,
                            arguments,
                        } => ShellMessage::ToolCall {
                            call_id: call_id.clone(),
                            name: name.clone(),
                            arguments: arguments.clone(),
                        },
                        ChatTurnEventKind::ToolResult {
                            call_id,
                            success,
                            result,
                        } => ShellMessage::ToolResult {
                            call_id: call_id.clone(),
                            success: *success,
                            result: result.clone(),
                        },
                        ChatTurnEventKind::Progress { message } => ShellMessage::InfoNotice {
                            content: message.clone(),
                        },
                        ChatTurnEventKind::Error { message } => ShellMessage::ErrorNotice {
                            content: message.clone(),
                        },
                        ChatTurnEventKind::Canceled => ShellMessage::InfoNotice {
                            content: "Canceled current response.".to_string(),
                        },
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

    pub fn transcript_cells(
        messages: &[ShellMessage],
        assistant_name: &str,
    ) -> Vec<TranscriptCell> {
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
            StreamFrame::Start { .. } | StreamFrame::Event { .. } | StreamFrame::Done { .. } => {
                None
            }
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

    #[cfg(test)]
    mod tests {
        use super::{
            MessageGroup, ShellMessage, TranscriptCellKind, cell_from_message,
            message_from_session_event, message_from_stream_frame, messages_from_session,
            transcript_cells,
        };
        use types::{ChatMessage, ChatSession, ChatTurnEventKind};
        use types::{ChatSessionEvent, StreamFrame};

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
        fn messages_from_session_keeps_completed_team_activity() {
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
        fn messages_from_session_uses_live_cancel_notice_text() {
            let mut session = ChatSession::new("agent-1".to_string(), "model".to_string());
            session.record_turn_user_message("turn-1", "cancel me");
            session.record_turn_event("turn-1", ChatTurnEventKind::Canceled);

            let transcript = messages_from_session(&session);

            assert_eq!(
                transcript.last(),
                Some(&ShellMessage::InfoNotice {
                    content: "Canceled current response.".to_string()
                })
            );
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
}

pub use app::run_tui;

#[derive(Debug, Clone, Default)]
pub struct TuiLaunchOptions {
    pub agent: Option<String>,
    pub session: Option<String>,
    pub message: Option<String>,
}
