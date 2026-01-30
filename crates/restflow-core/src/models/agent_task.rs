//! Agent Task model for scheduled agent execution.
//!
//! Agent tasks represent recurring or one-time scheduled executions of agents
//! with optional notification configurations for reporting results.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Execution mode for agent tasks
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ExecutionMode {
    /// Use restflow-ai API executor (default)
    #[default]
    Api,
    /// Use external CLI tool (e.g., claude, aider)
    Cli(CliExecutionConfig),
}

/// Configuration for CLI-based execution
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct CliExecutionConfig {
    /// CLI binary name (e.g., "claude", "aider")
    pub binary: String,
    /// Additional arguments to pass to the CLI
    #[serde(default)]
    pub args: Vec<String>,
    /// Working directory for CLI execution
    #[serde(default)]
    pub working_dir: Option<String>,
    /// Timeout in seconds for CLI execution
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    /// Whether to use PTY for interactive mode
    #[serde(default)]
    pub use_pty: bool,
}

fn default_timeout_secs() -> u64 {
    300 // 5 minutes default
}

impl Default for CliExecutionConfig {
    fn default() -> Self {
        Self {
            binary: "claude".to_string(),
            args: vec![],
            working_dir: None,
            timeout_secs: default_timeout_secs(),
            use_pty: false,
        }
    }
}

/// Status of an agent task
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Default)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum AgentTaskStatus {
    /// Task is active and will run on schedule
    #[default]
    Active,
    /// Task is paused (will not run until resumed)
    Paused,
    /// Task is currently running
    Running,
    /// Task completed (for one-time tasks)
    Completed,
    /// Task failed on last execution
    Failed,
}

/// Schedule configuration for agent tasks
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TaskSchedule {
    /// Run once at a specific time
    Once {
        /// Unix timestamp in milliseconds when to run
        #[ts(type = "number")]
        run_at: i64,
    },
    /// Run on a recurring interval
    Interval {
        /// Interval in milliseconds between runs
        #[ts(type = "number")]
        interval_ms: i64,
        /// Optional start time (defaults to now)
        #[ts(type = "number | null")]
        start_at: Option<i64>,
    },
    /// Run on a cron schedule
    Cron {
        /// Cron expression (e.g., "0 9 * * *" for 9 AM daily)
        expression: String,
        /// Timezone for the cron expression (e.g., "America/Los_Angeles")
        #[serde(default)]
        timezone: Option<String>,
    },
}

impl Default for TaskSchedule {
    fn default() -> Self {
        TaskSchedule::Interval {
            interval_ms: 3600000, // 1 hour default
            start_at: None,
        }
    }
}

/// Notification configuration for task results
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct NotificationConfig {
    /// Enable Telegram notifications
    #[serde(default)]
    pub telegram_enabled: bool,
    /// Telegram bot token (optional, uses system config if not set)
    #[serde(default)]
    pub telegram_bot_token: Option<String>,
    /// Telegram chat ID to send notifications to
    #[serde(default)]
    pub telegram_chat_id: Option<String>,
    /// Only notify on failure
    #[serde(default)]
    pub notify_on_failure_only: bool,
    /// Include full output in notification
    #[serde(default = "default_true")]
    pub include_output: bool,
}

fn default_true() -> bool {
    true
}

fn default_max_messages() -> usize {
    100
}

/// Memory configuration for agent task execution
///
/// Controls working memory behavior and persistence settings.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct MemoryConfig {
    /// Maximum number of messages to keep in working memory
    /// Older messages are discarded (no summarization)
    #[serde(default = "default_max_messages")]
    pub max_messages: usize,

    /// Enable file memory tools (save_memory, read_memory, etc.)
    /// Allows agents to persist important information to disk
    #[serde(default = "default_true")]
    pub enable_file_memory: bool,

    /// Persist conversation to long-term memory on task completion
    /// Working memory is chunked and stored for future retrieval
    #[serde(default = "default_true")]
    pub persist_on_complete: bool,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_messages: default_max_messages(),
            enable_file_memory: true,
            persist_on_complete: true,
        }
    }
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            telegram_enabled: false,
            telegram_bot_token: None,
            telegram_chat_id: None,
            notify_on_failure_only: false,
            include_output: true, // Default to true for include_output
        }
    }
}

/// Record of a task execution event
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TaskEvent {
    /// Unique event ID
    pub id: String,
    /// Task ID this event belongs to
    pub task_id: String,
    /// Event type
    pub event_type: TaskEventType,
    /// Timestamp of the event (milliseconds since epoch)
    #[ts(type = "number")]
    pub timestamp: i64,
    /// Optional message or details
    #[serde(default)]
    pub message: Option<String>,
    /// Execution output (for completion events)
    #[serde(default)]
    pub output: Option<String>,
    /// Duration of execution in milliseconds (for completion events)
    #[serde(default)]
    #[ts(type = "number | null")]
    pub duration_ms: Option<i64>,
}

/// Type of task event
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum TaskEventType {
    /// Task was created
    Created,
    /// Task started execution
    Started,
    /// Task completed successfully
    Completed,
    /// Task failed with error
    Failed,
    /// Task was paused
    Paused,
    /// Task was resumed
    Resumed,
    /// Notification was sent
    NotificationSent,
    /// Notification failed to send
    NotificationFailed,
}

/// An agent task represents a scheduled execution of an agent
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentTask {
    /// Unique identifier for the task
    pub id: String,
    /// Display name of the task
    pub name: String,
    /// Description of what this task does
    #[serde(default)]
    pub description: Option<String>,
    /// ID of the agent to execute
    pub agent_id: String,
    /// Input/prompt to send to the agent
    #[serde(default)]
    pub input: Option<String>,
    /// Schedule configuration
    pub schedule: TaskSchedule,
    /// Execution mode (API or CLI)
    #[serde(default)]
    pub execution_mode: ExecutionMode,
    /// Notification configuration
    #[serde(default)]
    pub notification: NotificationConfig,
    /// Memory configuration
    #[serde(default)]
    pub memory: MemoryConfig,
    /// Current status of the task
    #[serde(default)]
    pub status: AgentTaskStatus,
    /// Timestamp when the task was created (milliseconds since epoch)
    #[ts(type = "number")]
    pub created_at: i64,
    /// Timestamp when the task was last updated (milliseconds since epoch)
    #[ts(type = "number")]
    pub updated_at: i64,
    /// Timestamp of the last execution (milliseconds since epoch)
    #[serde(default)]
    #[ts(type = "number | null")]
    pub last_run_at: Option<i64>,
    /// Timestamp of the next scheduled execution (milliseconds since epoch)
    #[serde(default)]
    #[ts(type = "number | null")]
    pub next_run_at: Option<i64>,
    /// Count of successful executions
    #[serde(default)]
    pub success_count: u32,
    /// Count of failed executions
    #[serde(default)]
    pub failure_count: u32,
    /// Last error message if failed
    #[serde(default)]
    pub last_error: Option<String>,
    /// Webhook configuration for external triggers
    #[serde(default)]
    pub webhook: Option<super::webhook::WebhookConfig>,
}

impl AgentTask {
    /// Create a new agent task with the given parameters
    pub fn new(
        id: String,
        name: String,
        agent_id: String,
        schedule: TaskSchedule,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        let next_run = Self::calculate_next_run(&schedule, now);

        Self {
            id,
            name,
            description: None,
            agent_id,
            input: None,
            schedule,
            execution_mode: ExecutionMode::default(),
            notification: NotificationConfig::default(),
            memory: MemoryConfig::default(),
            status: AgentTaskStatus::Active,
            created_at: now,
            updated_at: now,
            last_run_at: None,
            next_run_at: next_run,
            success_count: 0,
            failure_count: 0,
            last_error: None,
            webhook: None,
        }
    }

    /// Create a new agent task with CLI execution mode
    pub fn new_with_cli(
        id: String,
        name: String,
        agent_id: String,
        schedule: TaskSchedule,
        cli_config: CliExecutionConfig,
    ) -> Self {
        let mut task = Self::new(id, name, agent_id, schedule);
        task.execution_mode = ExecutionMode::Cli(cli_config);
        task
    }

    /// Calculate the next run time based on the schedule
    pub fn calculate_next_run(schedule: &TaskSchedule, from_time: i64) -> Option<i64> {
        match schedule {
            TaskSchedule::Once { run_at } => {
                if *run_at > from_time {
                    Some(*run_at)
                } else {
                    None // Already passed
                }
            }
            TaskSchedule::Interval { interval_ms, start_at } => {
                let start = start_at.unwrap_or(from_time);
                if start > from_time {
                    Some(start)
                } else {
                    // Calculate next interval after from_time
                    let elapsed = from_time - start;
                    let intervals_passed = elapsed / interval_ms;
                    Some(start + (intervals_passed + 1) * interval_ms)
                }
            }
            TaskSchedule::Cron { expression, timezone } => {
                // Parse and calculate next cron time
                Self::next_cron_time(expression, timezone.as_deref(), from_time)
            }
        }
    }

    /// Calculate next cron execution time
    fn next_cron_time(expression: &str, timezone: Option<&str>, from_time: i64) -> Option<i64> {
        use chrono::{DateTime, Utc};
        use cron::Schedule;
        use std::str::FromStr;

        let schedule = Schedule::from_str(expression).ok()?;
        let from_datetime = DateTime::from_timestamp_millis(from_time)?;

        if let Some(tz_str) = timezone {
            // Parse timezone and find next time in that zone
            if let Ok(tz) = tz_str.parse::<chrono_tz::Tz>() {
                let local_time = from_datetime.with_timezone(&tz);
                let next = schedule.after(&local_time).next()?;
                Some(next.with_timezone(&Utc).timestamp_millis())
            } else {
                // Fallback to UTC if timezone parsing fails
                let next = schedule.after(&from_datetime).next()?;
                Some(next.timestamp_millis())
            }
        } else {
            // Default to UTC
            let next = schedule.after(&from_datetime).next()?;
            Some(next.timestamp_millis())
        }
    }

    /// Update the next run time based on current time
    pub fn update_next_run(&mut self) {
        let now = chrono::Utc::now().timestamp_millis();
        self.next_run_at = Self::calculate_next_run(&self.schedule, now);
        self.updated_at = now;
    }

    /// Mark the task as running
    pub fn set_running(&mut self) {
        self.status = AgentTaskStatus::Running;
        self.last_run_at = Some(chrono::Utc::now().timestamp_millis());
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }

    /// Mark the task as completed successfully
    pub fn set_completed(&mut self) {
        self.success_count += 1;
        self.last_error = None;
        self.updated_at = chrono::Utc::now().timestamp_millis();

        // Determine next status based on schedule type
        match &self.schedule {
            TaskSchedule::Once { .. } => {
                self.status = AgentTaskStatus::Completed;
                self.next_run_at = None;
            }
            _ => {
                self.status = AgentTaskStatus::Active;
                self.update_next_run();
            }
        }
    }

    /// Mark the task as failed
    pub fn set_failed(&mut self, error: String) {
        self.failure_count += 1;
        self.last_error = Some(error);
        self.status = AgentTaskStatus::Failed;
        self.updated_at = chrono::Utc::now().timestamp_millis();
        self.update_next_run(); // Still schedule next run
    }

    /// Pause the task
    pub fn pause(&mut self) {
        self.status = AgentTaskStatus::Paused;
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }

    /// Resume the task
    pub fn resume(&mut self) {
        self.status = AgentTaskStatus::Active;
        self.updated_at = chrono::Utc::now().timestamp_millis();
        self.update_next_run();
    }

    /// Check if the task should run now
    pub fn should_run(&self, current_time: i64) -> bool {
        if self.status != AgentTaskStatus::Active {
            return false;
        }

        if let Some(next_run) = self.next_run_at {
            current_time >= next_run
        } else {
            false
        }
    }

    /// Check if the task is active (can be scheduled)
    pub fn is_active(&self) -> bool {
        self.status == AgentTaskStatus::Active
    }

    /// Check if the task is running
    pub fn is_running(&self) -> bool {
        self.status == AgentTaskStatus::Running
    }
}

impl TaskEvent {
    /// Create a new task event
    pub fn new(task_id: String, event_type: TaskEventType) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            task_id,
            event_type,
            timestamp: chrono::Utc::now().timestamp_millis(),
            message: None,
            output: None,
            duration_ms: None,
        }
    }

    /// Create a new event with a message
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Create a new event with output
    pub fn with_output(mut self, output: impl Into<String>) -> Self {
        self.output = Some(output.into());
        self
    }

    /// Create a new event with duration
    pub fn with_duration(mut self, duration_ms: i64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_task_new() {
        let task = AgentTask::new(
            "task-123".to_string(),
            "Test Task".to_string(),
            "agent-456".to_string(),
            TaskSchedule::Interval {
                interval_ms: 3600000,
                start_at: None,
            },
        );

        assert_eq!(task.id, "task-123");
        assert_eq!(task.name, "Test Task");
        assert_eq!(task.agent_id, "agent-456");
        assert_eq!(task.status, AgentTaskStatus::Active);
        assert!(task.created_at > 0);
        assert!(task.next_run_at.is_some());
        assert_eq!(task.success_count, 0);
        assert_eq!(task.failure_count, 0);
    }

    #[test]
    fn test_once_schedule_calculation() {
        let future_time = chrono::Utc::now().timestamp_millis() + 10000;
        let schedule = TaskSchedule::Once { run_at: future_time };

        let next = AgentTask::calculate_next_run(&schedule, chrono::Utc::now().timestamp_millis());
        assert_eq!(next, Some(future_time));

        // Past time should return None
        let past_time = chrono::Utc::now().timestamp_millis() - 10000;
        let schedule_past = TaskSchedule::Once { run_at: past_time };
        let next_past = AgentTask::calculate_next_run(&schedule_past, chrono::Utc::now().timestamp_millis());
        assert!(next_past.is_none());
    }

    #[test]
    fn test_interval_schedule_calculation() {
        let now = 1000000000000i64; // Fixed time for testing
        let interval = 3600000i64; // 1 hour

        let schedule = TaskSchedule::Interval {
            interval_ms: interval,
            start_at: Some(now - 1000), // Started 1 second ago
        };

        let next = AgentTask::calculate_next_run(&schedule, now);
        assert!(next.is_some());
        let next_time = next.unwrap();
        assert!(next_time > now);
        assert!(next_time <= now + interval);
    }

    #[test]
    fn test_cron_schedule_calculation() {
        let schedule = TaskSchedule::Cron {
            expression: "0 0 9 * * *".to_string(), // Every day at 9 AM
            timezone: Some("UTC".to_string()),
        };

        let now = chrono::Utc::now().timestamp_millis();
        let next = AgentTask::calculate_next_run(&schedule, now);
        assert!(next.is_some());
        assert!(next.unwrap() > now);
    }

    #[test]
    fn test_task_status_transitions() {
        let mut task = AgentTask::new(
            "task-123".to_string(),
            "Test Task".to_string(),
            "agent-456".to_string(),
            TaskSchedule::Interval {
                interval_ms: 3600000,
                start_at: None,
            },
        );

        assert!(task.is_active());
        assert!(!task.is_running());

        task.set_running();
        assert!(task.is_running());
        assert!(task.last_run_at.is_some());

        task.set_completed();
        assert!(task.is_active());
        assert_eq!(task.success_count, 1);

        task.set_running();
        task.set_failed("Test error".to_string());
        assert_eq!(task.status, AgentTaskStatus::Failed);
        assert_eq!(task.failure_count, 1);
        assert_eq!(task.last_error, Some("Test error".to_string()));
    }

    #[test]
    fn test_pause_and_resume() {
        let mut task = AgentTask::new(
            "task-123".to_string(),
            "Test Task".to_string(),
            "agent-456".to_string(),
            TaskSchedule::Interval {
                interval_ms: 3600000,
                start_at: None,
            },
        );

        task.pause();
        assert_eq!(task.status, AgentTaskStatus::Paused);

        task.resume();
        assert_eq!(task.status, AgentTaskStatus::Active);
    }

    #[test]
    fn test_should_run() {
        // Use a future timestamp to ensure next_run_at is set
        let future_time = chrono::Utc::now().timestamp_millis() + 100000;
        
        let mut task = AgentTask::new(
            "task-123".to_string(),
            "Test Task".to_string(),
            "agent-456".to_string(),
            TaskSchedule::Once {
                run_at: future_time,
            },
        );

        // Before run time
        assert!(!task.should_run(future_time - 1000));

        // At run time
        assert!(task.should_run(future_time));

        // After run time
        assert!(task.should_run(future_time + 1000));

        // When paused
        task.pause();
        assert!(!task.should_run(future_time + 1000));
    }

    #[test]
    fn test_once_task_completion() {
        let mut task = AgentTask::new(
            "task-123".to_string(),
            "Test Task".to_string(),
            "agent-456".to_string(),
            TaskSchedule::Once {
                run_at: chrono::Utc::now().timestamp_millis() + 1000,
            },
        );

        task.set_running();
        task.set_completed();

        assert_eq!(task.status, AgentTaskStatus::Completed);
        assert!(task.next_run_at.is_none()); // No next run for one-time tasks
    }

    #[test]
    fn test_task_event_creation() {
        let event = TaskEvent::new("task-123".to_string(), TaskEventType::Started)
            .with_message("Starting execution")
            .with_duration(1500);

        assert_eq!(event.task_id, "task-123");
        assert_eq!(event.event_type, TaskEventType::Started);
        assert_eq!(event.message, Some("Starting execution".to_string()));
        assert_eq!(event.duration_ms, Some(1500));
        assert!(event.timestamp > 0);
    }

    #[test]
    fn test_notification_config_defaults() {
        let config = NotificationConfig::default();

        assert!(!config.telegram_enabled);
        assert!(config.telegram_bot_token.is_none());
        assert!(config.telegram_chat_id.is_none());
        assert!(!config.notify_on_failure_only);
        assert!(config.include_output);
    }

    #[test]
    fn test_schedule_default() {
        let schedule = TaskSchedule::default();

        match schedule {
            TaskSchedule::Interval { interval_ms, start_at } => {
                assert_eq!(interval_ms, 3600000);
                assert!(start_at.is_none());
            }
            _ => panic!("Expected Interval schedule"),
        }
    }

    #[test]
    fn test_status_default() {
        let status: AgentTaskStatus = Default::default();
        assert_eq!(status, AgentTaskStatus::Active);
    }

    #[test]
    fn test_execution_mode_default() {
        let mode: ExecutionMode = Default::default();
        assert_eq!(mode, ExecutionMode::Api);
    }

    #[test]
    fn test_cli_execution_config_default() {
        let config = CliExecutionConfig::default();
        assert_eq!(config.binary, "claude");
        assert!(config.args.is_empty());
        assert!(config.working_dir.is_none());
        assert_eq!(config.timeout_secs, 300);
        assert!(!config.use_pty);
    }

    #[test]
    fn test_agent_task_with_api_execution() {
        let task = AgentTask::new(
            "task-123".to_string(),
            "Test Task".to_string(),
            "agent-456".to_string(),
            TaskSchedule::default(),
        );
        assert_eq!(task.execution_mode, ExecutionMode::Api);
    }

    #[test]
    fn test_agent_task_with_cli_execution() {
        let cli_config = CliExecutionConfig {
            binary: "aider".to_string(),
            args: vec!["--yes".to_string()],
            working_dir: Some("/tmp/test".to_string()),
            timeout_secs: 600,
            use_pty: true,
        };

        let task = AgentTask::new_with_cli(
            "task-123".to_string(),
            "CLI Task".to_string(),
            "agent-456".to_string(),
            TaskSchedule::default(),
            cli_config.clone(),
        );

        match &task.execution_mode {
            ExecutionMode::Cli(config) => {
                assert_eq!(config.binary, "aider");
                assert_eq!(config.args, vec!["--yes".to_string()]);
                assert_eq!(config.working_dir, Some("/tmp/test".to_string()));
                assert_eq!(config.timeout_secs, 600);
                assert!(config.use_pty);
            }
            _ => panic!("Expected CLI execution mode"),
        }
    }

    #[test]
    fn test_execution_mode_serialization() {
        // Test API mode serialization
        let api_mode = ExecutionMode::Api;
        let json = serde_json::to_string(&api_mode).unwrap();
        assert!(json.contains("api"));

        // Test CLI mode serialization
        let cli_mode = ExecutionMode::Cli(CliExecutionConfig {
            binary: "claude".to_string(),
            args: vec!["-p".to_string()],
            working_dir: None,
            timeout_secs: 300,
            use_pty: false,
        });
        let json = serde_json::to_string(&cli_mode).unwrap();
        assert!(json.contains("cli"));
        assert!(json.contains("claude"));
    }

    #[test]
    fn test_execution_mode_deserialization() {
        // Test API mode deserialization
        let json = r#"{"type":"api"}"#;
        let mode: ExecutionMode = serde_json::from_str(json).unwrap();
        assert_eq!(mode, ExecutionMode::Api);

        // Test CLI mode deserialization
        let json = r#"{"type":"cli","binary":"aider","args":[],"timeout_secs":300,"use_pty":false}"#;
        let mode: ExecutionMode = serde_json::from_str(json).unwrap();
        match mode {
            ExecutionMode::Cli(config) => {
                assert_eq!(config.binary, "aider");
            }
            _ => panic!("Expected CLI mode"),
        }
    }

    #[test]
    fn test_memory_config_defaults() {
        let config = MemoryConfig::default();

        assert_eq!(config.max_messages, 100);
        assert!(config.enable_file_memory);
        assert!(config.persist_on_complete);
    }

    #[test]
    fn test_memory_config_custom() {
        let config = MemoryConfig {
            max_messages: 50,
            enable_file_memory: false,
            persist_on_complete: true,
        };

        assert_eq!(config.max_messages, 50);
        assert!(!config.enable_file_memory);
        assert!(config.persist_on_complete);
    }

    #[test]
    fn test_agent_task_with_memory_config() {
        let task = AgentTask::new(
            "task-123".to_string(),
            "Test Task".to_string(),
            "agent-456".to_string(),
            TaskSchedule::default(),
        );

        // Default memory config should be applied
        assert_eq!(task.memory.max_messages, 100);
        assert!(task.memory.enable_file_memory);
        assert!(task.memory.persist_on_complete);
    }

    #[test]
    fn test_memory_config_serialization() {
        let config = MemoryConfig {
            max_messages: 75,
            enable_file_memory: true,
            persist_on_complete: false,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: MemoryConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.max_messages, 75);
        assert!(deserialized.enable_file_memory);
        assert!(!deserialized.persist_on_complete);
    }

    #[test]
    fn test_memory_config_deserialization_with_defaults() {
        // Test deserializing with missing fields uses defaults
        let json = r#"{}"#;
        let config: MemoryConfig = serde_json::from_str(json).unwrap();

        assert_eq!(config.max_messages, 100);
        assert!(config.enable_file_memory);
        assert!(config.persist_on_complete);
    }

    #[test]
    fn test_agent_task_serialization_with_memory() {
        let task = AgentTask::new(
            "task-123".to_string(),
            "Test Task".to_string(),
            "agent-456".to_string(),
            TaskSchedule::default(),
        );

        let json = serde_json::to_string(&task).unwrap();
        assert!(json.contains("memory"));
        assert!(json.contains("max_messages"));

        let deserialized: AgentTask = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.memory.max_messages, 100);
    }
}
