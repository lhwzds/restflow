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
//! - `transactional_checkpoint`: Prepare-then-execute-then-commit checkpoint pattern
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
//! use restflow_core::runtime::background_agent::{
//!     BackgroundAgentRunner, AgentRuntimeExecutor, RunnerConfig,
//!     TelegramNotifier, TaskStreamEvent, NoopHeartbeatEmitter,
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
//! let heartbeat_emitter = Arc::new(NoopHeartbeatEmitter);
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
//! use restflow_core::runtime::background_agent::events::{TaskStreamEvent, TASK_STREAM_EVENT};
//!
//! let started = TaskStreamEvent::started("task-123", "My Task", "agent-456", "api");
//! let output = TaskStreamEvent::output("task-123", "Processing...\n", false);
//! let completed = TaskStreamEvent::completed("task-123", "Task completed successfully", 1500);
//! let _event_name = TASK_STREAM_EVENT;
//! ```
//!
//! # Status Events
//!
//! The runner emits heartbeat events inline during its poll cycle:
//!
//! ```ignore
//! use restflow_core::runtime::background_agent::{HeartbeatEvent, HEARTBEAT_EVENT};
//!
//! let _event_name = HEARTBEAT_EVENT;
//! let _event = HeartbeatEvent::Warning(restflow_core::runtime::background_agent::HeartbeatWarning {
//!     code: "SLOW_LOOP".into(),
//!     message: "Runner is catching up".into(),
//!     timestamp: chrono::Utc::now().timestamp_millis(),
//! });
//! ```
//!
//! # Retry Example
//!
//! ```ignore
//! use restflow_core::runtime::background_agent::retry::{RetryConfig, RetryState};
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
//! use restflow_core::runtime::background_agent::failover::{FailoverConfig, FailoverManager};
//! use crate::ModelId;
//!
//! let config = FailoverConfig::with_fallbacks(
//!     ModelId::ClaudeSonnet4_5,
//!     vec![ModelId::Gpt5, ModelId::DeepseekChat],
//! );
//! let manager = FailoverManager::new(config);
//!
//! // Get the best available model
//! if let Some(model) = manager.get_available_model().await {
//!     // Use this model
//! }
//! ```
//!
//! # Transactional Checkpoint Example
//!
//! ```ignore
//! use restflow_core::runtime::background_agent::{
//!     prepare, commit_if_success, UncommittedCheckpoint,
//! };
//!
//! // Before tool execution, prepare checkpoint in memory
//! let checkpoint = prepare(&agent_state, task_id.to_string(), "tool execution".to_string());
//!
//! // Execute the tool
//! let result = execute_tool(...).await;
//!
//! // Only persist checkpoint if tool succeeded
//! commit_if_success(&storage, Some(checkpoint), &result)?;
//! ```

pub mod broadcast_emitter;
pub mod cli_executor;
pub mod error_classification;
pub mod events;
pub mod executor;
pub mod failover;
pub mod heartbeat;
pub mod model_catalog;
pub mod notifier;
pub mod outcome;
pub mod persist;
pub mod preflight;
pub mod reply_sender;
pub mod retry;
pub mod runner;
pub mod skill_snapshot;
#[cfg(any(test, feature = "test-utils"))]
pub mod testkit;
pub mod transactional_checkpoint;

pub use crate::runtime::orchestrator::OrchestratingAgentExecutor;
pub use cli_executor::{CliAgentExecutor, create_cli_executor_with_events};
pub use events::{
    ChannelEventEmitter, ExecutionStats, NoopEventEmitter, StreamEventKind, TASK_STREAM_EVENT,
    TaskEventEmitter, TaskStreamEvent,
};
pub use executor::{AgentRuntimeExecutor, SessionInputMode, SessionTurnRuntimeOptions};
pub use failover::{FailoverConfig, FailoverManager, ModelStatus, execute_with_failover};
pub use heartbeat::{
    ChannelHeartbeatEmitter, HEARTBEAT_EVENT, HeartbeatEmitter, HeartbeatEvent, HeartbeatPulse,
    HeartbeatWarning, NoopHeartbeatEmitter, RunnerStatus, RunnerStatusEvent, SystemStats,
};
pub use notifier::TelegramNotifier;
pub use outcome::{
    CompactionMetrics, ExecutionErrorClassification, ExecutionErrorKind, ExecutionFailure,
    ExecutionMetrics, ExecutionOutcome, RetryClass, SessionExecutionResult,
};
pub use persist::{MemoryPersister, PersistConfig, PersistResult};
pub use reply_sender::BackgroundReplySenderFactory;
pub use retry::{ErrorCategory, RetryConfig, RetryState, is_transient_error};
pub use runner::{
    AgentExecutor, BackgroundAgentRunner, ExecutionResult, NoopNotificationSender,
    NotificationSender, RunnerConfig, RunnerHandle,
};
pub use transactional_checkpoint::{
    CheckpointMeta, UncommittedCheckpoint, commit_if_success, prepare,
};
