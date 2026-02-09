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
pub enum BackgroundAgentStatus {
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

impl BackgroundAgentStatus {
    pub const fn as_str(&self) -> &'static str {
        match self {
            BackgroundAgentStatus::Active => "active",
            BackgroundAgentStatus::Paused => "paused",
            BackgroundAgentStatus::Running => "running",
            BackgroundAgentStatus::Completed => "completed",
            BackgroundAgentStatus::Failed => "failed",
        }
    }
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

/// Scope for background-agent memory persistence.
///
/// Controls whether long-term memory is shared across all background agents of
/// an agent or isolated per background agent.
#[derive(Debug, Clone, Default, Serialize, TS, PartialEq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    /// Share long-term memory across background agents using the same agent_id.
    #[default]
    SharedAgent,
    /// Isolate long-term memory by background agent.
    #[serde(rename = "per_background_agent")]
    PerBackgroundAgent,
}

impl<'de> Deserialize<'de> for MemoryScope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "shared_agent" => Ok(Self::SharedAgent),
            "per_background_agent" => Ok(Self::PerBackgroundAgent),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["shared_agent", "per_background_agent"],
            )),
        }
    }
}

fn default_memory_scope() -> MemoryScope {
    MemoryScope::SharedAgent
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

    /// Scope for long-term memory persistence.
    /// Shared scope stores memory under the agent ID, while isolated scope
    /// stores memory under a background-agent-specific namespace.
    #[serde(default = "default_memory_scope")]
    pub memory_scope: MemoryScope,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_messages: default_max_messages(),
            enable_file_memory: true,
            persist_on_complete: true,
            memory_scope: MemoryScope::SharedAgent,
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

/// Creation payload for background agents.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct BackgroundAgentSpec {
    /// Display name of the background agent
    pub name: String,
    /// ID of the agent to execute
    pub agent_id: String,
    /// Optional description
    #[serde(default)]
    pub description: Option<String>,
    /// Optional input prompt
    #[serde(default)]
    pub input: Option<String>,
    /// Optional input template rendered at runtime
    #[serde(default)]
    pub input_template: Option<String>,
    /// Schedule configuration
    pub schedule: TaskSchedule,
    /// Optional notification configuration
    #[serde(default)]
    pub notification: Option<NotificationConfig>,
    /// Optional execution mode
    #[serde(default)]
    pub execution_mode: Option<ExecutionMode>,
    /// Optional memory configuration
    #[serde(default)]
    pub memory: Option<MemoryConfig>,
}

/// Partial update payload for background agents.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct BackgroundAgentPatch {
    /// New display name
    #[serde(default)]
    pub name: Option<String>,
    /// New description
    #[serde(default)]
    pub description: Option<String>,
    /// New agent ID
    #[serde(default)]
    pub agent_id: Option<String>,
    /// New input prompt
    #[serde(default)]
    pub input: Option<String>,
    /// New input template
    #[serde(default)]
    pub input_template: Option<String>,
    /// New schedule configuration
    #[serde(default)]
    pub schedule: Option<TaskSchedule>,
    /// New notification configuration
    #[serde(default)]
    pub notification: Option<NotificationConfig>,
    /// New execution mode
    #[serde(default)]
    pub execution_mode: Option<ExecutionMode>,
    /// New memory configuration
    #[serde(default)]
    pub memory: Option<MemoryConfig>,
}

/// Control actions for a background agent.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundAgentControlAction {
    /// Start an agent that is not active
    Start,
    /// Pause future executions
    Pause,
    /// Resume scheduled executions
    Resume,
    /// Stop current/future execution
    Stop,
    /// Trigger immediate execution
    RunNow,
}

/// Source for background communication messages.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq, Default)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundMessageSource {
    /// Message provided by a human user
    #[default]
    User,
    /// Message provided by another agent
    Agent,
    /// System generated message
    System,
}

/// Delivery state of background communication messages.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq, Default)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundMessageStatus {
    /// Waiting to be injected into a running agent
    #[default]
    Queued,
    /// Successfully injected to a running agent
    Delivered,
    /// Processed and consumed by a run
    Consumed,
    /// Delivery failed
    Failed,
}

impl BackgroundMessageStatus {
    pub const fn as_str(&self) -> &'static str {
        match self {
            BackgroundMessageStatus::Queued => "queued",
            BackgroundMessageStatus::Delivered => "delivered",
            BackgroundMessageStatus::Consumed => "consumed",
            BackgroundMessageStatus::Failed => "failed",
        }
    }
}

/// A communication message sent to a background agent.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BackgroundMessage {
    /// Message ID
    pub id: String,
    /// Target background agent ID
    pub background_agent_id: String,
    /// Source of the message
    pub source: BackgroundMessageSource,
    /// Delivery status
    pub status: BackgroundMessageStatus,
    /// Message content
    pub message: String,
    /// Message creation timestamp
    #[ts(type = "number")]
    pub created_at: i64,
    /// Delivery timestamp
    #[serde(default)]
    #[ts(type = "number | null")]
    pub delivered_at: Option<i64>,
    /// Consumption timestamp
    #[serde(default)]
    #[ts(type = "number | null")]
    pub consumed_at: Option<i64>,
    /// Error details for failed delivery
    #[serde(default)]
    pub error: Option<String>,
}

impl BackgroundMessage {
    /// Create a new queued background message.
    pub fn new(
        background_agent_id: String,
        source: BackgroundMessageSource,
        message: String,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            background_agent_id,
            source,
            status: BackgroundMessageStatus::Queued,
            message,
            created_at: chrono::Utc::now().timestamp_millis(),
            delivered_at: None,
            consumed_at: None,
            error: None,
        }
    }

    /// Mark message as delivered to a running agent.
    pub fn mark_delivered(&mut self) {
        self.status = BackgroundMessageStatus::Delivered;
        self.delivered_at = Some(chrono::Utc::now().timestamp_millis());
        self.error = None;
    }

    /// Mark message as consumed by an execution.
    pub fn mark_consumed(&mut self) {
        self.status = BackgroundMessageStatus::Consumed;
        self.consumed_at = Some(chrono::Utc::now().timestamp_millis());
        self.error = None;
    }

    /// Mark message delivery as failed.
    pub fn mark_failed(&mut self, error: String) {
        self.status = BackgroundMessageStatus::Failed;
        self.error = Some(error);
    }
}

/// Aggregated progress snapshot for a background agent.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BackgroundProgress {
    /// Background agent ID
    pub background_agent_id: String,
    /// Current status
    pub status: BackgroundAgentStatus,
    /// Current stage label from the latest event
    #[serde(default)]
    pub stage: Option<String>,
    /// Most recent event
    #[serde(default)]
    pub recent_event: Option<TaskEvent>,
    /// Recent events in descending order
    #[serde(default)]
    pub recent_events: Vec<TaskEvent>,
    /// Last run timestamp
    #[serde(default)]
    #[ts(type = "number | null")]
    pub last_run_at: Option<i64>,
    /// Next run timestamp
    #[serde(default)]
    #[ts(type = "number | null")]
    pub next_run_at: Option<i64>,
    /// Total token usage
    #[serde(default)]
    pub total_tokens_used: u32,
    /// Total execution cost
    #[serde(default)]
    pub total_cost_usd: f64,
    /// Successful run count
    #[serde(default)]
    pub success_count: u32,
    /// Failed run count
    #[serde(default)]
    pub failure_count: u32,
    /// Pending queued message count
    #[serde(default)]
    pub pending_message_count: u32,
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
    /// Tokens used during execution
    #[serde(default)]
    pub tokens_used: Option<u32>,
    /// Cost in USD for this execution
    #[serde(default)]
    pub cost_usd: Option<f64>,
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
    /// Context compaction occurred during execution
    Compaction,
}

/// An agent task represents a scheduled execution of an agent
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BackgroundAgent {
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
    /// Optional template rendered to construct runtime input
    #[serde(default)]
    pub input_template: Option<String>,
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
    pub status: BackgroundAgentStatus,
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
    /// Cumulative tokens used across executions
    #[serde(default)]
    pub total_tokens_used: u32,
    /// Cumulative cost in USD across executions
    #[serde(default)]
    pub total_cost_usd: f64,
    /// Last error message if failed
    #[serde(default)]
    pub last_error: Option<String>,
    /// Webhook configuration for external triggers
    #[serde(default)]
    pub webhook: Option<super::webhook::WebhookConfig>,
    /// Summary message pointer for compacted task sessions
    #[serde(default)]
    pub summary_message_id: Option<String>,
}

impl BackgroundAgent {
    /// Create a new agent task with the given parameters
    pub fn new(id: String, name: String, agent_id: String, schedule: TaskSchedule) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        let next_run = Self::calculate_next_run(&schedule, now);

        Self {
            id,
            name,
            description: None,
            agent_id,
            input: None,
            input_template: None,
            schedule,
            execution_mode: ExecutionMode::default(),
            notification: NotificationConfig::default(),
            memory: MemoryConfig::default(),
            status: BackgroundAgentStatus::Active,
            created_at: now,
            updated_at: now,
            last_run_at: None,
            next_run_at: next_run,
            success_count: 0,
            failure_count: 0,
            total_tokens_used: 0,
            total_cost_usd: 0.0,
            last_error: None,
            webhook: None,
            summary_message_id: None,
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
            TaskSchedule::Interval {
                interval_ms,
                start_at,
            } => {
                if *interval_ms <= 0 {
                    return None;
                }
                let start = start_at.unwrap_or(from_time);
                if start > from_time {
                    Some(start)
                } else {
                    // Calculate next interval after from_time using saturating
                    // arithmetic to avoid overflow with large elapsed times.
                    let elapsed = from_time - start;
                    let intervals_passed = elapsed / interval_ms;
                    let next_count = intervals_passed.saturating_add(1);
                    let offset = next_count.saturating_mul(*interval_ms);
                    let next = start.saturating_add(offset);
                    if next <= from_time {
                        // Overflow saturated to i64::MAX or interval too small
                        None
                    } else {
                        Some(next)
                    }
                }
            }
            TaskSchedule::Cron {
                expression,
                timezone,
            } => {
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

        let normalized = expression.trim();
        let field_count = normalized.split_whitespace().count();
        let schedule = if field_count == 5 {
            // Accept standard 5-field cron expressions by prepending seconds.
            Schedule::from_str(&format!("0 {}", normalized))
                .or_else(|_| Schedule::from_str(normalized))
                .ok()?
        } else {
            Schedule::from_str(normalized).ok()?
        };
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
        self.status = BackgroundAgentStatus::Running;
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
                self.status = BackgroundAgentStatus::Completed;
                self.next_run_at = None;
            }
            _ => {
                self.status = BackgroundAgentStatus::Active;
                self.update_next_run();
            }
        }
    }

    /// Mark the task as failed
    pub fn set_failed(&mut self, error: String) {
        self.failure_count += 1;
        self.last_error = Some(error);
        self.status = BackgroundAgentStatus::Failed;
        self.updated_at = chrono::Utc::now().timestamp_millis();
        self.update_next_run(); // Still schedule next run
    }

    /// Pause the task
    pub fn pause(&mut self) {
        self.status = BackgroundAgentStatus::Paused;
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }

    /// Resume the task
    pub fn resume(&mut self) {
        self.status = BackgroundAgentStatus::Active;
        self.updated_at = chrono::Utc::now().timestamp_millis();
        self.update_next_run();
    }

    /// Check if the task should run now
    pub fn should_run(&self, current_time: i64) -> bool {
        if self.status != BackgroundAgentStatus::Active {
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
        self.status == BackgroundAgentStatus::Active
    }

    /// Check if the task is running
    pub fn is_running(&self) -> bool {
        self.status == BackgroundAgentStatus::Running
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
            tokens_used: None,
            cost_usd: None,
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

/// Compatibility alias for background-agent-centric schedule naming.
pub type BackgroundAgentSchedule = TaskSchedule;

/// Compatibility alias for background-agent-centric event naming.
pub type BackgroundAgentEvent = TaskEvent;

/// Compatibility alias for background-agent-centric event type naming.
pub type BackgroundAgentEventType = TaskEventType;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_background_agent_new() {
        let task = BackgroundAgent::new(
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
        assert_eq!(task.status, BackgroundAgentStatus::Active);
        assert!(task.input_template.is_none());
        assert!(task.created_at > 0);
        assert!(task.next_run_at.is_some());
        assert_eq!(task.success_count, 0);
        assert_eq!(task.failure_count, 0);
    }

    #[test]
    fn test_once_schedule_calculation() {
        let future_time = chrono::Utc::now().timestamp_millis() + 10000;
        let schedule = TaskSchedule::Once {
            run_at: future_time,
        };

        let next =
            BackgroundAgent::calculate_next_run(&schedule, chrono::Utc::now().timestamp_millis());
        assert_eq!(next, Some(future_time));

        // Past time should return None
        let past_time = chrono::Utc::now().timestamp_millis() - 10000;
        let schedule_past = TaskSchedule::Once { run_at: past_time };
        let next_past = BackgroundAgent::calculate_next_run(
            &schedule_past,
            chrono::Utc::now().timestamp_millis(),
        );
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

        let next = BackgroundAgent::calculate_next_run(&schedule, now);
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
        let next = BackgroundAgent::calculate_next_run(&schedule, now);
        assert!(next.is_some());
        assert!(next.unwrap() > now);
    }

    #[test]
    fn test_five_field_cron_schedule_calculation() {
        let schedule = TaskSchedule::Cron {
            expression: "* * * * *".to_string(), // Every minute (5-field cron)
            timezone: Some("UTC".to_string()),
        };

        let now = chrono::Utc::now().timestamp_millis();
        let next = BackgroundAgent::calculate_next_run(&schedule, now);
        assert!(next.is_some());
        assert!(next.unwrap() > now);
    }

    #[test]
    fn test_interval_zero_returns_none() {
        let schedule = TaskSchedule::Interval {
            interval_ms: 0,
            start_at: Some(1000),
        };
        assert!(BackgroundAgent::calculate_next_run(&schedule, 2000).is_none());
    }

    #[test]
    fn test_interval_negative_returns_none() {
        let schedule = TaskSchedule::Interval {
            interval_ms: -1,
            start_at: Some(1000),
        };
        assert!(BackgroundAgent::calculate_next_run(&schedule, 2000).is_none());
    }

    #[test]
    fn test_interval_overflow_returns_none() {
        let schedule = TaskSchedule::Interval {
            interval_ms: i64::MAX / 2,
            start_at: Some(i64::MAX / 2),
        };
        // With extreme values, saturating arithmetic should prevent panic
        // and return None when the result can't advance past from_time.
        let result = BackgroundAgent::calculate_next_run(&schedule, i64::MAX - 1);
        // Either None (saturated) or a valid future time — no panic.
        if let Some(next) = result {
            assert!(next > i64::MAX - 1);
        }
    }

    #[test]
    fn test_task_status_transitions() {
        let mut task = BackgroundAgent::new(
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
        assert_eq!(task.status, BackgroundAgentStatus::Failed);
        assert_eq!(task.failure_count, 1);
        assert_eq!(task.last_error, Some("Test error".to_string()));
    }

    #[test]
    fn test_pause_and_resume() {
        let mut task = BackgroundAgent::new(
            "task-123".to_string(),
            "Test Task".to_string(),
            "agent-456".to_string(),
            TaskSchedule::Interval {
                interval_ms: 3600000,
                start_at: None,
            },
        );

        task.pause();
        assert_eq!(task.status, BackgroundAgentStatus::Paused);

        task.resume();
        assert_eq!(task.status, BackgroundAgentStatus::Active);
    }

    #[test]
    fn test_should_run() {
        // Use a future timestamp to ensure next_run_at is set
        let future_time = chrono::Utc::now().timestamp_millis() + 100000;

        let mut task = BackgroundAgent::new(
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
        let mut task = BackgroundAgent::new(
            "task-123".to_string(),
            "Test Task".to_string(),
            "agent-456".to_string(),
            TaskSchedule::Once {
                run_at: chrono::Utc::now().timestamp_millis() + 1000,
            },
        );

        task.set_running();
        task.set_completed();

        assert_eq!(task.status, BackgroundAgentStatus::Completed);
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
            TaskSchedule::Interval {
                interval_ms,
                start_at,
            } => {
                assert_eq!(interval_ms, 3600000);
                assert!(start_at.is_none());
            }
            _ => panic!("Expected Interval schedule"),
        }
    }

    #[test]
    fn test_status_default() {
        let status: BackgroundAgentStatus = Default::default();
        assert_eq!(status, BackgroundAgentStatus::Active);
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
    fn test_background_agent_with_api_execution() {
        let task = BackgroundAgent::new(
            "task-123".to_string(),
            "Test Task".to_string(),
            "agent-456".to_string(),
            TaskSchedule::default(),
        );
        assert_eq!(task.execution_mode, ExecutionMode::Api);
    }

    #[test]
    fn test_background_agent_with_cli_execution() {
        let cli_config = CliExecutionConfig {
            binary: "aider".to_string(),
            args: vec!["--yes".to_string()],
            working_dir: Some("/tmp/test".to_string()),
            timeout_secs: 600,
            use_pty: true,
        };

        let task = BackgroundAgent::new_with_cli(
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
        let json =
            r#"{"type":"cli","binary":"aider","args":[],"timeout_secs":300,"use_pty":false}"#;
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
        assert_eq!(config.memory_scope, MemoryScope::SharedAgent);
    }

    #[test]
    fn test_memory_config_custom() {
        let config = MemoryConfig {
            max_messages: 50,
            enable_file_memory: false,
            persist_on_complete: true,
            memory_scope: MemoryScope::PerBackgroundAgent,
        };

        assert_eq!(config.max_messages, 50);
        assert!(!config.enable_file_memory);
        assert!(config.persist_on_complete);
        assert_eq!(config.memory_scope, MemoryScope::PerBackgroundAgent);
    }

    #[test]
    fn test_background_agent_with_memory_config() {
        let task = BackgroundAgent::new(
            "task-123".to_string(),
            "Test Task".to_string(),
            "agent-456".to_string(),
            TaskSchedule::default(),
        );

        // Default memory config should be applied
        assert_eq!(task.memory.max_messages, 100);
        assert!(task.memory.enable_file_memory);
        assert!(task.memory.persist_on_complete);
        assert_eq!(task.memory.memory_scope, MemoryScope::SharedAgent);
    }

    #[test]
    fn test_memory_config_serialization() {
        let config = MemoryConfig {
            max_messages: 75,
            enable_file_memory: true,
            persist_on_complete: false,
            memory_scope: MemoryScope::PerBackgroundAgent,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: MemoryConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.max_messages, 75);
        assert!(deserialized.enable_file_memory);
        assert!(!deserialized.persist_on_complete);
        assert_eq!(deserialized.memory_scope, MemoryScope::PerBackgroundAgent);
    }

    #[test]
    fn test_memory_config_deserialization_with_defaults() {
        // Test deserializing with missing fields uses defaults
        let json = r#"{}"#;
        let config: MemoryConfig = serde_json::from_str(json).unwrap();

        assert_eq!(config.max_messages, 100);
        assert!(config.enable_file_memory);
        assert!(config.persist_on_complete);
        assert_eq!(config.memory_scope, MemoryScope::SharedAgent);
    }

    #[test]
    fn test_memory_scope_serialization() {
        let scope = MemoryScope::PerBackgroundAgent;
        let serialized = serde_json::to_string(&scope).unwrap();
        assert_eq!(serialized, r#""per_background_agent""#);
    }

    #[test]
    fn test_background_agent_serialization_with_memory() {
        let task = BackgroundAgent::new(
            "task-123".to_string(),
            "Test Task".to_string(),
            "agent-456".to_string(),
            TaskSchedule::default(),
        );

        let json = serde_json::to_string(&task).unwrap();
        assert!(json.contains("memory"));
        assert!(json.contains("max_messages"));

        let deserialized: BackgroundAgent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.memory.max_messages, 100);
        assert_eq!(deserialized.memory.memory_scope, MemoryScope::SharedAgent);
    }

    #[test]
    fn test_background_agent_aliases_are_compatible() {
        let schedule = BackgroundAgentSchedule::default();
        let mut background_agent = BackgroundAgent::new(
            "bg-1".to_string(),
            "Background Agent".to_string(),
            "agent-1".to_string(),
            schedule,
        );
        assert_eq!(background_agent.status, BackgroundAgentStatus::Active);

        background_agent.set_running();
        assert_eq!(background_agent.status, BackgroundAgentStatus::Running);

        let event = BackgroundAgentEvent::new(
            background_agent.id.clone(),
            BackgroundAgentEventType::Started,
        )
        .with_message("started");
        assert_eq!(event.event_type, BackgroundAgentEventType::Started);
        assert_eq!(event.message.as_deref(), Some("started"));
    }
}
