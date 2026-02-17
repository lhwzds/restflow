//! Agent Task module - Scheduled agent execution system.
//!
//! This module provides the infrastructure for scheduling and executing agent
//! tasks on a recurring or one-time basis, with optional notification support.
//!
//! # Architecture
//!
//! - `runner`: The background task runner that polls for and executes tasks
//! - `executor`: Real agent executor that bridges to restflow_ai
//! - `cli_executor`: CLI agent executor for external tools (Claude Code, Aider)
//! - `notifier`: Telegram notification sender for task results
//! - `events`: Real-time streaming events for frontend updates
//! - `heartbeat`: Status types and emitters (integrated into runner)
//! - `retry`: Retry mechanism for transient failures
//! - `failover`: Model failover system for automatic fallback
//! - `AgentExecutor`: Trait for executing agents (allows dependency injection)
//! - `NotificationSender`: Trait for sending notifications (allows DI)
//! - `TaskEventEmitter`: Trait for emitting real-time events (allows DI)
//!
//! # Execution Modes
//!
//! - **API Mode**: Uses the injected `AgentExecutor` for LLM API-based execution
//! - **CLI Mode**: Uses `CliAgentExecutor` for external CLI tools (claude, aider, etc.)
//!
//! # Usage
//!
//! ```ignore
//! use restflow_tauri::background_agent::{
//!     BackgroundAgentRunner, AgentRuntimeExecutor, RunnerConfig,
//!     TelegramNotifier, TaskStreamEvent, TauriHeartbeatEmitter,
//!     RetryConfig, FailoverConfig, FailoverManager
//! };
//!
//! // For API-based execution:
//! let executor = Arc::new(AgentRuntimeExecutor::new(
//!     storage.clone(),
//!     process_registry.clone(),
//!     auth_manager.clone(),
//!     subagent_tracker.clone(),
//!     subagent_definitions.clone(),
//!     subagent_config.clone(),
//! ));
//! let notifier = Arc::new(TelegramNotifier::new(storage.secrets.clone()));
//! let heartbeat_emitter = Arc::new(TauriHeartbeatEmitter::new(app_handle.clone()));
//!
//! let runner = Arc::new(BackgroundAgentRunner::with_heartbeat_emitter(
//!     task_storage,
//!     executor,
//!     notifier,
//!     RunnerConfig::default(),
//!     heartbeat_emitter,
//! ));
//!
//! let handle = runner.clone().start();
//!
//! // Later, to stop:
//! handle.stop().await?;
//! ```
//!
//! # Streaming Events
//!
//! The events module provides real-time streaming to the frontend:
//!
//! ```ignore
//! use restflow_tauri::background_agent::events::{TaskStreamEvent, TASK_STREAM_EVENT};
//! use tauri::Manager;
//!
//! // Emit task started event
//! app_handle.emit(TASK_STREAM_EVENT, TaskStreamEvent::started(
//!     "task-123", "My Task", "agent-456", "api"
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
//! # Status Events
//!
//! The runner emits heartbeat events inline during its poll cycle:
//!
//! ```ignore
//! use restflow_tauri::background_agent::{HeartbeatEvent, HEARTBEAT_EVENT};
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
//!
//! # Retry Example
//!
//! ```ignore
//! use restflow_tauri::background_agent::retry::{RetryConfig, RetryState};
//!
//! let config = RetryConfig::default();
//! let mut state = RetryState::new();
//!
//! // After a failure
//! if state.should_retry(&config, "Connection timeout") {
//!     state.record_failure("Connection timeout", &config);
//!     // Wait before retrying
//! }
//! ```
//!
//! # Failover Example
//!
//! ```ignore
//! use restflow_tauri::background_agent::failover::{FailoverConfig, FailoverManager};
//! use crate::AIModel;
//!
//! let config = FailoverConfig::with_fallbacks(
//!     AIModel::ClaudeSonnet4_5,
//!     vec![AIModel::Gpt5, AIModel::DeepseekChat],
//! );
//! let manager = FailoverManager::new(config);
//!
//! // Get the best available model
//! if let Some(model) = manager.get_available_model().await {
//!     // Use this model
//! }
//! ```

pub mod broadcast_emitter;
pub mod cli_executor;
pub mod event_log;
pub mod event_logging_emitter;
pub mod events;
pub mod executor;
pub mod failover;
pub mod heartbeat;
pub mod model_catalog;
pub mod notifier;
pub mod persist;
pub mod preflight;
pub mod retry;
pub mod runner;
#[cfg(any(test, feature = "test-utils"))]
pub mod testkit;

pub use cli_executor::{CliAgentExecutor, create_cli_executor_with_events};
#[cfg(feature = "tauri-runtime")]
pub use events::TauriEventEmitter;
pub use events::{
    ChannelEventEmitter, ExecutionStats, NoopEventEmitter, StreamEventKind, TASK_STREAM_EVENT,
    TaskEventEmitter, TaskStreamEvent,
};
pub use executor::{AgentRuntimeExecutor, SessionExecutionResult, SessionInputMode};
pub use failover::{FailoverConfig, FailoverManager, ModelStatus, execute_with_failover};
pub use event_log::{AgentEvent, EventLog};
pub use event_logging_emitter::EventLoggingEmitter;
#[cfg(feature = "tauri-runtime")]
pub use heartbeat::TauriHeartbeatEmitter;
pub use heartbeat::{
    ChannelHeartbeatEmitter, HEARTBEAT_EVENT, HeartbeatEmitter, HeartbeatEvent, HeartbeatPulse,
    HeartbeatWarning, NoopHeartbeatEmitter, RunnerStatus, RunnerStatusEvent, SystemStats,
};
pub use notifier::TelegramNotifier;
pub use persist::{MemoryPersister, PersistConfig, PersistResult};
pub use retry::{ErrorCategory, RetryConfig, RetryState, is_transient_error};
pub use runner::{
    AgentExecutor, BackgroundAgentRunner, ExecutionResult, NoopNotificationSender,
    NotificationSender, RunnerConfig, RunnerHandle,
};
