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
//! - `notifier`: Telegram notification sender for task results
//! - `AgentExecutor`: Trait for executing agents (allows dependency injection)
//! - `NotificationSender`: Trait for sending notifications (allows DI)
//!
//! # Usage
//!
//! ```ignore
//! use restflow_tauri::agent_task::{
//!     AgentTaskRunner, RunnerConfig, RealAgentExecutor, CliExecutor, TelegramNotifier
//! };
//!
//! // For API-based execution:
//! let executor = Arc::new(RealAgentExecutor::new(storage.clone()));
//!
//! // For CLI-based execution:
//! let cli_executor = Arc::new(CliExecutor::default_claude());
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
//! // Later, to stop:
//! handle.stop().await?;
//! ```

pub mod cli_executor;
pub mod executor;
pub mod notifier;
pub mod runner;

pub use cli_executor::CliExecutor;
pub use executor::RealAgentExecutor;
pub use notifier::TelegramNotifier;
pub use runner::{
    AgentExecutor,
    AgentTaskRunner,
    NoopNotificationSender,
    NotificationSender,
    RunnerConfig,
    RunnerHandle,
};
