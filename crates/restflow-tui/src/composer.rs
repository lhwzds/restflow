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

    pub fn cursor_position(&self) -> (u16, u16) {
        let before_cursor = &self.draft[..self.cursor];
        let row = before_cursor.split('\n').count().saturating_sub(1) as u16;
        let column = before_cursor
            .rsplit('\n')
            .next()
            .map(|segment| segment.chars().count() as u16)
            .unwrap_or(0);
        (column, row)
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
}
