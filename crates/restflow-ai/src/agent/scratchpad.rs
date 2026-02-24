use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::Serialize;
use serde_json::{Value, json};

/// Append-only JSONL scratchpad for agent execution debugging.
#[derive(Debug, Clone)]
pub struct Scratchpad {
    path: PathBuf,
}

#[derive(Debug, Serialize)]
struct ScratchpadEntry {
    timestamp: String,
    iteration: usize,
    event_type: &'static str,
    data: Value,
}

impl Scratchpad {
    /// Create a new scratchpad at path, creating parent directories if needed.
    pub fn new(path: PathBuf) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(&self, iteration: usize, event_type: &'static str, data: Value) {
        let entry = ScratchpadEntry {
            timestamp: Utc::now().to_rfc3339(),
            iteration,
            event_type,
            data,
        };

        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            && let Ok(line) = serde_json::to_string(&entry)
        {
            let _ = writeln!(file, "{line}");
        }
    }

    pub fn log_start(&self, execution_id: &str, model: &str, input: &str) {
        self.append(
            0,
            "execution_start",
            json!({
                "execution_id": execution_id,
                "model": model,
                "input": input,
            }),
        );
    }

    pub fn log_iteration_begin(&self, iteration: usize) {
        self.append(iteration, "iteration_begin", json!({}));
    }

    pub fn log_text_delta(&self, iteration: usize, content: &str) {
        self.append(
            iteration,
            "text_delta",
            json!({
                "content": content,
            }),
        );
    }

    pub fn log_thinking(&self, iteration: usize, content: &str) {
        self.append(
            iteration,
            "thinking",
            json!({
                "content": content,
            }),
        );
    }

    pub fn log_tool_call(&self, iteration: usize, call_id: &str, tool_name: &str, arguments: &str) {
        self.append(
            iteration,
            "tool_call",
            json!({
                "call_id": call_id,
                "tool": tool_name,
                "arguments": arguments,
            }),
        );
    }

    pub fn log_tool_result(
        &self,
        iteration: usize,
        call_id: &str,
        tool_name: &str,
        success: bool,
        result: &str,
    ) {
        self.append(
            iteration,
            "tool_result",
            json!({
                "call_id": call_id,
                "tool": tool_name,
                "success": success,
                "result": result,
            }),
        );
    }

    pub fn log_error(&self, iteration: usize, error: &str) {
        self.append(
            iteration,
            "error",
            json!({
                "error": error,
            }),
        );
    }

    /// Save full tool output to a file alongside the scratchpad JSONL.
    /// Returns the file path on success, None on failure.
    pub fn save_full_output(
        &self,
        call_id: &str,
        tool_name: &str,
        content: &str,
    ) -> Option<PathBuf> {
        let dir = self.path.parent()?.join("tool-output");
        if fs::create_dir_all(&dir).is_err() {
            return None;
        }
        // Sanitize tool_name for filename safety
        let safe_name: String = tool_name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let filename = format!("{safe_name}-{call_id}.txt");
        let path = dir.join(filename);
        match fs::write(&path, content) {
            Ok(()) => Some(path),
            Err(_) => None,
        }
    }

    pub fn log_complete(&self, iteration: usize, total_tokens: u32, total_cost_usd: f64) {
        self.append(
            iteration,
            "execution_complete",
            json!({
                "total_tokens": total_tokens,
                "total_cost_usd": total_cost_usd,
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save_full_output_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scratchpad.jsonl");
        let scratchpad = Scratchpad::new(path).unwrap();

        let content = "Hello, this is the full tool output content.";
        let result = scratchpad.save_full_output("call-42", "bash", content);

        assert!(result.is_some());
        let saved_path = result.unwrap();
        assert!(saved_path.exists());
        assert_eq!(std::fs::read_to_string(&saved_path).unwrap(), content);
        assert!(saved_path.to_string_lossy().contains("bash-call-42.txt"));
    }

    #[test]
    fn test_save_full_output_sanitizes_tool_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scratchpad.jsonl");
        let scratchpad = Scratchpad::new(path).unwrap();

        let result = scratchpad.save_full_output("id1", "web/fetch:v2", "data");
        assert!(result.is_some());
        let saved_path = result.unwrap();
        // Special chars should be replaced with underscores
        let filename = saved_path.file_name().unwrap().to_string_lossy();
        assert_eq!(filename, "web_fetch_v2-id1.txt");
    }

    #[test]
    fn test_scratchpad_append_and_jsonl_format() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scratchpad.jsonl");
        let scratchpad = Scratchpad::new(path.clone()).unwrap();

        scratchpad.log_start("exec-1", "mock-model", "hello");
        scratchpad.log_iteration_begin(1);
        scratchpad.log_tool_call(1, "call-1", "bash", r#"{"command":"ls"}"#);
        scratchpad.log_tool_result(1, "call-1", "bash", true, r#"{"stdout":"ok"}"#);
        scratchpad.log_complete(1, 256, 0.0123);

        let content = std::fs::read_to_string(path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 5);

        for line in lines {
            let parsed: Value = serde_json::from_str(line).unwrap();
            assert!(parsed.get("timestamp").is_some());
            assert!(parsed.get("event_type").is_some());
            assert!(parsed.get("data").is_some());
        }
    }
}
