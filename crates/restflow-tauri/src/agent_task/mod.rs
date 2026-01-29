//! Agent Task module - Scheduled agent execution system.
//!
//! This module provides the infrastructure for scheduling and executing agent
//! tasks on a recurring or one-time basis, with optional notification support.
//!
//! # Architecture
//!
//! - `runner`: The background task runner that polls for and executes tasks
//! - `executor`: Real agent executor that bridges to restflow_ai
//! - `AgentExecutor`: Trait for executing agents (allows dependency injection)
//! - `NotificationSender`: Trait for sending notifications (allows DI)
//!
//! # Usage
//!
//! ```ignore
//! use restflow_tauri::agent_task::{AgentTaskRunner, RunnerConfig, RealAgentExecutor};
//!
//! let executor = Arc::new(RealAgentExecutor::new(storage.clone()));
//! let runner = Arc::new(AgentTaskRunner::new(
//!     task_storage,
//!     executor,
//!     notifier,
//!     RunnerConfig::default(),
//! ));
//!
//! let handle = runner.clone().start();
//!
//! // Later, to stop:
//! handle.stop().await?;
//! ```

pub mod executor;
pub mod runner;

pub use executor::RealAgentExecutor;
pub use runner::{
    AgentExecutor,
    AgentTaskRunner,
    NoopNotificationSender,
    NotificationSender,
    RunnerConfig,
    RunnerHandle,
};
