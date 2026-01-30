//! Channel Message Handler Module
//!
//! This module bridges the channel infrastructure (from restflow-core) with
//! the task execution system. It handles:
//!
//! - Processing inbound messages from interactive channels (Telegram, etc.)
//! - Routing commands (/help, /tasks, /run, /status, /stop)
//! - Forwarding messages to running tasks
//! - Handling approval/rejection responses
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │           Interactive Channel           │
//! │       (Telegram, Discord, Slack)        │
//! └─────────────────────────────────────────┘
//!                     │
//!                     ▼
//! ┌─────────────────────────────────────────┐
//! │           MessageHandler                │
//! │   (handler.rs - routes messages)        │
//! └─────────────────────────────────────────┘
//!                     │
//!        ┌────────────┼────────────┐
//!        ▼            ▼            ▼
//! ┌────────────┐ ┌──────────┐ ┌───────────┐
//! │  Commands  │ │ Forwarder│ │   Help    │
//! │ commands.rs│ │forwarder │ │ (default) │
//! └────────────┘ └──────────┘ └───────────┘
//!        │            │
//!        └────────────┘
//!                │
//!                ▼
//! ┌─────────────────────────────────────────┐
//! │          TaskTrigger Trait              │
//! │   (trigger.rs - task operations)        │
//! └─────────────────────────────────────────┘
//!                     │
//!                     ▼
//! ┌─────────────────────────────────────────┐
//! │        AgentTaskRunner / AppState       │
//! └─────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use restflow_tauri::channel::{start_message_handler, MessageHandlerConfig};
//!
//! // In your app setup:
//! let router: Arc<ChannelRouter> = /* from state */;
//! let task_trigger: Arc<dyn TaskTrigger> = /* your implementation */;
//!
//! start_message_handler(router, task_trigger, MessageHandlerConfig::default());
//! ```

mod commands;
mod forwarder;
mod handler;
mod trigger;

pub use handler::{start_message_handler, MessageHandlerConfig};
pub use trigger::{SystemStatus, TaskTrigger};

// Re-export for convenience
pub use commands::handle_command;
pub use forwarder::forward_to_task;

#[cfg(test)]
pub use trigger::mock;
