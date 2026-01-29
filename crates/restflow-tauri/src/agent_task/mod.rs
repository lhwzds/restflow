//! Agent Task module - Scheduled agent execution system.
//!
//! This module provides the infrastructure for scheduling and executing agent
//! tasks on a recurring or one-time basis, with optional notification support.
//!
//! # Architecture
//!
//! - `runner`: The background task runner that polls for and executes tasks
//! - `executor`: Real agent executor that bridges to restflow_ai
//! - `cli_executor`: CLI-based executor for external coding agents (claude, aider)
//! - `pty_cli_executor`: PTY-based executor for interactive CLI tools
//! - `notifier`: Telegram notification sender for task results
//! - `events`: Real-time streaming events for frontend updates
//! - `heartbeat`: Periodic health monitoring and connection status
//! - `AgentExecutor`: Trait for executing agents (allows dependency injection)
//! - `NotificationSender`: Trait for sending notifications (allows DI)
//! - `TaskEventEmitter`: Trait for emitting real-time events (allows DI)
//!
//! # Usage
//!
//! ```ignore
//! use restflow_tauri::agent_task::{
//!     AgentTaskRunner, RunnerConfig, RealAgentExecutor, CliExecutor,
//!     PtyCliExecutor, TelegramNotifier, TaskStreamEvent, HeartbeatRunner
//! };
//!
//! // For API-based execution:
//! let executor = Arc::new(RealAgentExecutor::new(storage.clone()));
//!
//! // For CLI-based execution (non-interactive):
//! let cli_executor = Arc::new(CliExecutor::default_claude());
//!
//! // For PTY-based execution (interactive CLIs):
//! let pty_executor = Arc::new(PtyCliExecutor::default_claude());
//!
//! let notifier = Arc::new(TelegramNotifier::new(storage.secrets.clone()));
//! let runner = Arc::new(AgentTaskRunner::new(
//!     task_storage,
//!     executor,
//!     notifier,
//!     RunnerConfig::default(),
//! ));
//!
//! let handle = runner.clone().start();
//!
//! // Start heartbeat monitoring
//! let heartbeat = Arc::new(HeartbeatRunner::new(HeartbeatConfig::default()));
//! let heartbeat_handle = heartbeat.start_with_tauri(app_handle);
//!
//! // Later, to stop:
//! handle.stop().await?;
//! heartbeat_handle.stop().await?;
//! ```
//!
//! # Streaming Events
//!
//! The events module provides real-time streaming to the frontend:
//!
//! ```ignore
//! use restflow_tauri::agent_task::events::{TaskStreamEvent, TASK_STREAM_EVENT};
//! use tauri::Manager;
//!
//! // Emit task started event
//! app_handle.emit(TASK_STREAM_EVENT, TaskStreamEvent::started(
//!     "task-123", "My Task", "agent-456", "cli:claude"
//! ));
//!
//! // Stream output
//! app_handle.emit(TASK_STREAM_EVENT, TaskStreamEvent::output(
//!     "task-123", "Processing...\n", false
//! ));
//!
//! // Emit completion
//! app_handle.emit(TASK_STREAM_EVENT, TaskStreamEvent::completed(
//!     "task-123", "Task completed successfully", 1500
//! ));
//! ```
//!
//! # Heartbeat Events
//!
//! The heartbeat module provides connection monitoring:
//!
//! ```ignore
//! use restflow_tauri::agent_task::heartbeat::{HeartbeatEvent, HEARTBEAT_EVENT};
//! use tauri::Manager;
//!
//! // Frontend listens to heartbeat events
//! app_handle.listen(HEARTBEAT_EVENT, |event| {
//!     let heartbeat: HeartbeatEvent = serde_json::from_str(event.payload()).unwrap();
//!     match heartbeat {
//!         HeartbeatEvent::Pulse(pulse) => {
//!             // Update connection status, task counts, etc.
//!         }
//!         HeartbeatEvent::StatusChange(status) => {
//!             // Handle runner status changes
//!         }
//!         HeartbeatEvent::Warning(warning) => {
//!             // Display warning to user
//!         }
//!     }
//! });
//! ```

pub mod cli_executor;
pub mod events;
pub mod executor;
pub mod heartbeat;
pub mod notifier;
pub mod pty_cli_executor;
pub mod runner;

pub use cli_executor::CliExecutor;
pub use events::{
    ChannelEventEmitter, ExecutionStats, NoopEventEmitter, StreamEventKind, TaskEventEmitter,
    TaskStreamEvent, TauriEventEmitter, TASK_STREAM_EVENT,
};
pub use executor::RealAgentExecutor;
pub use heartbeat::{
    ChannelHeartbeatEmitter, HeartbeatConfig, HeartbeatEmitter, HeartbeatEvent, HeartbeatHandle,
    HeartbeatPulse, HeartbeatRunner, HeartbeatWarning, NoopHeartbeatEmitter, RunnerStatus,
    RunnerStatusEvent, SystemStats, TauriHeartbeatEmitter, HEARTBEAT_EVENT,
};
pub use notifier::TelegramNotifier;
pub use pty_cli_executor::PtyCliExecutor;
pub use runner::{
    AgentExecutor, AgentTaskRunner, NoopNotificationSender, NotificationSender, RunnerConfig,
    RunnerHandle,
};
