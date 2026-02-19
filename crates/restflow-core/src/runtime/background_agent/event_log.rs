use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Event types for background agent execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum AgentEvent {
    /// Task started initialization.
    TaskStarted {
        timestamp: i64,
        task_id: String,
        agent_id: String,
    },

    /// LLM generation step.
    LlmGeneration {
        timestamp: i64,
        step: u32,
        tokens_in: u32,
        tokens_out: u32,
        model: String,
    },

    /// Tool call started.
    ToolCallStarted {
        timestamp: i64,
        step: u32,
        tool_name: String,
        input: String,
    },

    /// Tool call completed.
    ToolCallCompleted {
        timestamp: i64,
        step: u32,
        tool_name: String,
        success: bool,
        output: String,
        duration_ms: u64,
    },

    /// Human message sent to agent.
    HumanMessage { timestamp: i64, content: String },

    /// Task paused.
    Paused { timestamp: i64, reason: String },

    /// Task resumed.
    Resumed { timestamp: i64 },

    /// Checkpoint saved.
    CheckpointSaved {
        timestamp: i64,
        checkpoint_id: String,
    },

    /// Error occurred.
    Error { timestamp: i64, error: String },

    /// Task completed.
    TaskCompleted { timestamp: i64, result: String },
}

impl AgentEvent {
    /// Get the timestamp of this event.
    pub fn timestamp(&self) -> i64 {
        match self {
            AgentEvent::TaskStarted { timestamp, .. } => *timestamp,
            AgentEvent::LlmGeneration { timestamp, .. } => *timestamp,
            AgentEvent::ToolCallStarted { timestamp, .. } => *timestamp,
            AgentEvent::ToolCallCompleted { timestamp, .. } => *timestamp,
            AgentEvent::HumanMessage { timestamp, .. } => *timestamp,
            AgentEvent::Paused { timestamp, .. } => *timestamp,
            AgentEvent::Resumed { timestamp, .. } => *timestamp,
            AgentEvent::CheckpointSaved { timestamp, .. } => *timestamp,
            AgentEvent::Error { timestamp, .. } => *timestamp,
            AgentEvent::TaskCompleted { timestamp, .. } => *timestamp,
        }
    }
}

/// Append-only JSONL event log for background agent tasks.
///
/// Each event is serialized to JSON and written as a single line.
/// The log can be read back to reconstruct agent state.
pub struct EventLog {
    writer: BufWriter<std::fs::File>,
    mirror_writer: Option<BufWriter<std::fs::File>>,
    #[allow(dead_code)]
    path: String,
    #[allow(dead_code)]
    task_id: String,
    #[allow(dead_code)]
    run_id: Option<String>,
}

impl Drop for EventLog {
    fn drop(&mut self) {
        // Ensure all buffered data is flushed when EventLog is dropped
        let _ = self.writer.flush();
        if let Some(mirror) = self.mirror_writer.as_mut() {
            let _ = mirror.flush();
        }
    }
}

impl EventLog {
    pub fn legacy_log_path(task_id: &str, log_dir: &Path) -> PathBuf {
        log_dir.join(format!("{}.jsonl", task_id))
    }

    pub fn run_log_path(task_id: &str, run_id: &str, log_dir: &Path) -> PathBuf {
        log_dir.join(task_id).join(format!("{}.jsonl", run_id))
    }

    pub fn list_run_ids(task_id: &str, log_dir: &Path) -> Result<Vec<String>> {
        let task_dir = log_dir.join(task_id);
        if !task_dir.exists() {
            return Ok(Vec::new());
        }

        let mut run_ids: Vec<String> = std::fs::read_dir(task_dir)?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter_map(|name| name.strip_suffix(".jsonl").map(|s| s.to_string()))
            .collect();

        run_ids.sort();
        run_ids.reverse();
        Ok(run_ids)
    }

    fn open_writer(path: &Path) -> Result<BufWriter<std::fs::File>> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("Failed to open event log: {}", path.display()))?;
        Ok(BufWriter::new(file))
    }

    /// Create a new event log for the given task.
    ///
    /// Creates the log directory if it doesn't exist.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use restflow_core::runtime::background_agent::event_log::EventLog;
    ///
    /// let log = EventLog::new("task-123", std::path::Path::new("~/.restflow/logs/tasks")).unwrap();
    /// ```
    pub fn new(task_id: &str, log_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(log_dir)
            .with_context(|| format!("Failed to create log directory: {}", log_dir.display()))?;

        let path = Self::legacy_log_path(task_id, log_dir);
        let writer = Self::open_writer(&path)?;

        Ok(Self {
            writer,
            mirror_writer: None,
            path: path.to_string_lossy().to_string(),
            task_id: task_id.to_string(),
            run_id: None,
        })
    }

    /// Create a per-run event log and mirror records into legacy task-level log.
    pub fn new_for_run(task_id: &str, run_id: &str, log_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(log_dir)
            .with_context(|| format!("Failed to create log directory: {}", log_dir.display()))?;

        let run_path = Self::run_log_path(task_id, run_id, log_dir);
        if let Some(parent) = run_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create run log directory: {}", parent.display()))?;
        }

        let writer = Self::open_writer(&run_path)?;
        let legacy_path = Self::legacy_log_path(task_id, log_dir);
        let mirror_writer = Some(Self::open_writer(&legacy_path)?);

        Ok(Self {
            writer,
            mirror_writer,
            path: run_path.to_string_lossy().to_string(),
            task_id: task_id.to_string(),
            run_id: Some(run_id.to_string()),
        })
    }

    /// Append an event to the log.
    ///
    /// # Example
    ///
    /// ```ignore
    /// # use restflow_core::runtime::background_agent::event_log::{EventLog, AgentEvent};
    /// use chrono::Utc;
    ///
    /// # let mut log = EventLog::new("task-123", std::path::Path::new("/tmp")).unwrap();
    /// log.append(&AgentEvent::TaskStarted {
    ///     timestamp: Utc::now().timestamp_millis(),
    ///     task_id: "task-123".to_string(),
    ///     agent_id: "agent-1".to_string(),
    /// }).unwrap();
    /// ```
    pub fn append(&mut self, event: &AgentEvent) -> Result<()> {
        let json = serde_json::to_string(event)
            .with_context(|| format!("Failed to serialize event: {:?}", event))?;

        writeln!(self.writer, "{}", json).with_context(|| "Failed to write event to log")?;
        self.writer
            .flush()
            .with_context(|| "Failed to flush event log")?;

        if let Some(mirror) = self.mirror_writer.as_mut() {
            writeln!(mirror, "{}", json)
                .with_context(|| "Failed to write event to mirrored task log")?;
            mirror
                .flush()
                .with_context(|| "Failed to flush mirrored task log")?;
        }

        Ok(())
    }

    /// Read all events from a log file.
    ///
    /// Returns an empty vector if the file doesn't exist.
    pub fn read_all(path: &Path) -> Result<Vec<AgentEvent>> {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // File doesn't exist - return empty vector as documented
                return Ok(Vec::new());
            }
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("Failed to read log file: {}", path.display()));
            }
        };

        let mut events = Vec::new();
        for (line_num, line) in content.lines().enumerate() {
            if line.trim().is_empty() {
                continue; // Skip empty lines
            }

            let event: AgentEvent = serde_json::from_str(line).with_context(|| {
                format!("Failed to parse event on line {}: {}", line_num + 1, line)
            })?;

            events.push(event);
        }

        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_event_log_append_and_read() {
        let temp_dir = TempDir::new().unwrap();
        let task_id = "test-task-1";

        // Create log and append events
        let mut log = EventLog::new(task_id, temp_dir.path()).unwrap();

        let event1 = AgentEvent::LlmGeneration {
            timestamp: 1234567891,
            step: 1,
            tokens_in: 100,
            tokens_out: 50,
            model: "gpt-4".to_string(),
        };

        let event2 = AgentEvent::TaskStarted {
            timestamp: 1234567890,
            task_id: task_id.to_string(),
            agent_id: "agent-1".to_string(),
        };

        log.append(&event1).unwrap();
        log.append(&event2).unwrap();

        // Drop log to flush
        drop(log);

        // Read back
        let log_path = temp_dir.path().join(format!("{}.jsonl", task_id));
        let events = EventLog::read_all(&log_path).unwrap();

        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_new_for_run_writes_run_and_legacy() {
        let temp_dir = TempDir::new().unwrap();
        let task_id = "task-123";
        let run_id = "1771471846966-abcd";
        let mut log = EventLog::new_for_run(task_id, run_id, temp_dir.path()).unwrap();

        let event = AgentEvent::TaskCompleted {
            timestamp: 123,
            result: "ok".to_string(),
        };
        log.append(&event).unwrap();
        drop(log);

        let run_path = EventLog::run_log_path(task_id, run_id, temp_dir.path());
        let legacy_path = EventLog::legacy_log_path(task_id, temp_dir.path());
        assert!(run_path.exists());
        assert!(legacy_path.exists());

        let run_events = EventLog::read_all(&run_path).unwrap();
        let legacy_events = EventLog::read_all(&legacy_path).unwrap();
        assert_eq!(run_events.len(), 1);
        assert_eq!(legacy_events.len(), 1);
    }

    #[test]
    fn test_list_run_ids_sorted_desc() {
        let temp_dir = TempDir::new().unwrap();
        let task_id = "task-list";
        let task_dir = temp_dir.path().join(task_id);
        fs::create_dir_all(&task_dir).unwrap();
        fs::write(task_dir.join("100-a.jsonl"), "{}\n").unwrap();
        fs::write(task_dir.join("200-b.jsonl"), "{}\n").unwrap();

        let runs = EventLog::list_run_ids(task_id, temp_dir.path()).unwrap();
        assert_eq!(runs, vec!["200-b".to_string(), "100-a".to_string()]);
    }

    #[test]
    fn test_event_log_serialization() {
        let event = AgentEvent::ToolCallCompleted {
            timestamp: 1234567890,
            step: 1,
            tool_name: "bash".to_string(),
            success: true,
            output: "Hello, world!".to_string(),
            duration_ms: 100,
        };

        let json = serde_json::to_string(&event).unwrap();
        let parsed: AgentEvent = serde_json::from_str(&json).unwrap();

        match parsed {
            AgentEvent::ToolCallCompleted {
                tool_name,
                success,
                output,
                ..
            } => {
                assert_eq!(tool_name, "bash");
                assert!(success);
                assert_eq!(output, "Hello, world!");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_event_log_empty_lines() {
        let temp_dir = TempDir::new().unwrap();
        let task_id = "test-task-2";

        let mut log = EventLog::new(task_id, temp_dir.path()).unwrap();

        let event = AgentEvent::TaskStarted {
            timestamp: 1234567890,
            task_id: task_id.to_string(),
            agent_id: "agent-1".to_string(),
        };
        log.append(&event).unwrap();

        // Manually add empty lines to log
        let log_path = temp_dir.path().join(format!("{}.jsonl", task_id));
        let mut file = fs::OpenOptions::new().append(true).open(&log_path).unwrap();
        writeln!(file).unwrap();
        writeln!(file).unwrap();

        // Read back - should ignore empty lines
        let events = EventLog::read_all(&log_path).unwrap();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_event_timestamp() {
        let event = AgentEvent::Paused {
            timestamp: 9999999999,
            reason: "user request".to_string(),
        };
        assert_eq!(event.timestamp(), 9999999999);
    }

    #[test]
    fn test_event_log_creates_directory() {
        let temp_dir = TempDir::new().unwrap();
        let nested_dir = temp_dir.path().join("nested").join("logs");

        assert!(!nested_dir.exists());

        let _log = EventLog::new("test-task", &nested_dir).unwrap();

        assert!(nested_dir.exists());
    }

    #[test]
    fn test_read_all_missing_file_returns_empty() {
        // Test that read_all returns empty vector for non-existent file
        let temp_dir = TempDir::new().unwrap();
        let non_existent_path = temp_dir.path().join("does-not-exist.jsonl");

        // Verify file doesn't exist
        assert!(!non_existent_path.exists());

        // read_all should return Ok(Vec::new()) as documented
        let events = EventLog::read_all(&non_existent_path).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_all_event_types_serializable() {
        let events = vec![
            AgentEvent::TaskStarted {
                timestamp: 0,
                task_id: "test".to_string(),
                agent_id: "agent".to_string(),
            },
            AgentEvent::LlmGeneration {
                timestamp: 0,
                step: 1,
                tokens_in: 10,
                tokens_out: 5,
                model: "gpt-4".to_string(),
            },
            AgentEvent::ToolCallStarted {
                timestamp: 0,
                step: 1,
                tool_name: "bash".to_string(),
                input: "echo hi".to_string(),
            },
            AgentEvent::ToolCallCompleted {
                timestamp: 0,
                step: 1,
                tool_name: "bash".to_string(),
                success: true,
                output: "hi".to_string(),
                duration_ms: 10,
            },
            AgentEvent::HumanMessage {
                timestamp: 0,
                content: "hello".to_string(),
            },
            AgentEvent::Paused {
                timestamp: 0,
                reason: "pause".to_string(),
            },
            AgentEvent::Resumed { timestamp: 0 },
            AgentEvent::CheckpointSaved {
                timestamp: 0,
                checkpoint_id: "cp-1".to_string(),
            },
            AgentEvent::Error {
                timestamp: 0,
                error: "error".to_string(),
            },
            AgentEvent::TaskCompleted {
                timestamp: 0,
                result: "done".to_string(),
            },
        ];

        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
            // Ensure round-trip preserves event
            match (event, parsed) {
                (AgentEvent::TaskStarted { .. }, AgentEvent::TaskStarted { .. }) => (),
                (AgentEvent::LlmGeneration { .. }, AgentEvent::LlmGeneration { .. }) => (),
                (AgentEvent::ToolCallStarted { .. }, AgentEvent::ToolCallStarted { .. }) => (),
                (AgentEvent::ToolCallCompleted { .. }, AgentEvent::ToolCallCompleted { .. }) => (),
                (AgentEvent::HumanMessage { .. }, AgentEvent::HumanMessage { .. }) => (),
                (AgentEvent::Paused { .. }, AgentEvent::Paused { .. }) => (),
                (AgentEvent::Resumed { .. }, AgentEvent::Resumed { .. }) => (),
                (AgentEvent::CheckpointSaved { .. }, AgentEvent::CheckpointSaved { .. }) => (),
                (AgentEvent::Error { .. }, AgentEvent::Error { .. }) => (),
                (AgentEvent::TaskCompleted { .. }, AgentEvent::TaskCompleted { .. }) => (),
                _ => panic!("Event type mismatch"),
            }
        }
    }
}
