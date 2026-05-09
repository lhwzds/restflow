//! Agent Task module - Scheduled agent execution system.
//!
//! This module provides the infrastructure for scheduling and executing agent
//! tasks on a recurring or one-time basis.
//! It is the durable task runtime owner in `runtime`, while delegated
//! sub-agent execution remains an `ai` capability injected into this
//! runtime when needed.
//!
//! # Architecture
//!
//! - `runner`: The task runner that polls for and executes tasks
//! - `executor`: Real agent executor that bridges to `ai`
//! - `events`: Real-time streaming events for runtime clients
//! - `heartbeat`: Status types and emitters (integrated into runner)
//! - `retry`: Retry mechanism for transient failures
//! - `failover`: Model failover system for automatic fallback
//! - `AgentExecutor`: Trait for executing agents (allows dependency injection)
//! - `TaskEventEmitter`: Trait for emitting real-time events (allows DI)
//!
//! # Execution
//!
//! Tasks are durable triggers for bound chat sessions. The runner executes each
//! task as a session turn through the injected `AgentExecutor`.
//!
//! # Usage
//!
//! ```ignore
//! use runtime::runtime::task_runtime::{
//!     TaskRunner, AgentRuntimeExecutor, TaskRunnerConfig,
//!     TaskStreamEvent, NoopHeartbeatEmitter, RetryConfig, FailoverConfig,
//!     FailoverManager
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
//! let heartbeat_emitter = Arc::new(NoopHeartbeatEmitter);
//!
//! let runner = Arc::new(TaskRunner::with_heartbeat_emitter(
//!     task_storage,
//!     executor,
//!     TaskRunnerConfig::default(),
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
//! The events module provides real-time streaming to runtime clients:
//!
//! ```ignore
//! use runtime::runtime::task_runtime::events::{TaskStreamEvent, TASK_STREAM_EVENT};
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
//! use runtime::runtime::task_runtime::{HeartbeatEvent, HEARTBEAT_EVENT};
//!
//! let _event_name = HEARTBEAT_EVENT;
//! let _event = HeartbeatEvent::Warning(runtime::runtime::task_runtime::HeartbeatWarning {
//!     code: "SLOW_LOOP".into(),
//!     message: "Runner is catching up".into(),
//!     timestamp: chrono::Utc::now().timestamp_millis(),
//! });
//! ```
//!
//! # Retry Example
//!
//! ```ignore
//! use runtime::runtime::task_runtime::retry::{RetryConfig, RetryState};
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
//! use runtime::runtime::task_runtime::failover::{FailoverConfig, FailoverManager};
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
pub mod error_classification;
pub mod events;
pub mod executor;
pub mod failover;
pub mod heartbeat;
pub mod model_catalog;
pub mod outcome;
pub mod preflight;
pub mod reply_sender;
pub mod retry;
pub mod runner;
pub mod skill_snapshot;
#[cfg(any(test, feature = "test-utils"))]
pub mod testkit;

pub use crate::runtime::orchestrator::OrchestratingAgentExecutor;
pub use events::{
    ChannelEventEmitter, ExecutionStats, NoopEventEmitter, StreamEventKind, TASK_STREAM_EVENT,
    TaskEventEmitter, TaskStreamEvent,
};
pub use executor::{AgentRuntimeExecutor, SessionInputMode, SessionTurnRuntimeOptions};
#[cfg(any(test, feature = "test-utils"))]
pub use executor::{TestLlmFactoryGuard, install_test_llm_factory};
pub use failover::{FailoverConfig, FailoverManager, ModelStatus, execute_with_failover};
pub use heartbeat::{
    ChannelHeartbeatEmitter, HEARTBEAT_EVENT, HeartbeatEmitter, HeartbeatEvent, HeartbeatPulse,
    HeartbeatWarning, NoopHeartbeatEmitter, RunnerStatus, RunnerStatusEvent, SystemStats,
};
pub use outcome::{
    CompactionMetrics, ExecutionErrorClassification, ExecutionErrorKind, ExecutionFailure,
    ExecutionMetrics, ExecutionOutcome, RetryClass, SessionExecutionResult,
};
pub use reply_sender::TaskReplySenderFactory;
pub use retry::{ErrorCategory, RetryConfig, RetryState, is_transient_error};
pub use runner::{AgentExecutor, ExecutionResult, TaskRunner, TaskRunnerConfig, TaskRunnerHandle};
