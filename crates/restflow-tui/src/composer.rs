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
    #[allow(dead_code)]
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
