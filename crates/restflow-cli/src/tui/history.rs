use std::path::PathBuf;

use restflow_core::paths;

pub struct InputHistory {
    entries: Vec<String>,
    max_size: usize,
    file_path: PathBuf,
    cursor: Option<usize>,
    pending_input: Option<String>,
}

impl InputHistory {
    pub fn new(max_size: usize) -> Self {
        let file_path = paths::ensure_data_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("input_history.txt");
        let entries = Self::load_from_file(&file_path, max_size);

        Self {
            entries,
            max_size,
            file_path,
            cursor: None,
            pending_input: None,
        }
    }

    fn load_from_file(path: &PathBuf, max_size: usize) -> Vec<String> {
        match std::fs::read_to_string(path) {
            Ok(content) => content
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(str::to_string)
                .rev()
                .take(max_size)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn add(&mut self, entry: String) {
        if entry.trim().is_empty() {
            return;
        }

        if self.entries.last() == Some(&entry) {
            return;
        }

        self.entries.push(entry);

        while self.entries.len() > self.max_size {
            self.entries.remove(0);
        }

        self.reset_navigation();
    }

    pub fn previous(&mut self, current_input: &str) -> Option<String> {
        if self.entries.is_empty() {
            return None;
        }

        let next_index = match self.cursor {
            None => {
                self.pending_input = Some(current_input.to_string());
                self.entries.len().saturating_sub(1)
            }
            Some(0) => 0,
            Some(index) => index.saturating_sub(1),
        };

        self.cursor = Some(next_index);
        self.entries.get(next_index).cloned()
    }

    pub fn next(&mut self) -> Option<String> {
        let Some(current_index) = self.cursor else {
            return None;
        };

        if current_index + 1 >= self.entries.len() {
            self.cursor = None;
            return self.pending_input.take();
        }

        let next_index = current_index + 1;
        self.cursor = Some(next_index);
        self.entries.get(next_index).cloned()
    }

    pub fn reset_navigation(&mut self) {
        self.cursor = None;
        self.pending_input = None;
    }

    pub fn save(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = self.entries.join("\n");
        std::fs::write(&self.file_path, content)?;
        Ok(())
    }
}
