//! Channel Message Handler Module
//!
//! This module bridges the channel infrastructure (from restflow-core) with
//! the task execution system. It handles:
//!
//! - Processing inbound messages from interactive channels (Telegram, etc.)
//! - Routing commands (/help, /agents, /run, /status, /stop)
//! - Forwarding messages to running background agents
//! - Dispatching natural language messages to AI chat
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
//! │           MessageRouter                 │
//! │    (router.rs - routing decisions)      │
//! └─────────────────────────────────────────┘
//!                     │
//!        ┌────────────┼────────────┬────────────┐
//!        ▼            ▼            ▼            ▼
//! ┌────────────┐ ┌──────────┐ ┌───────────┐ ┌─────────────┐
//! │  Commands  │ │ Forwarder│ │   Chat    │ │   Ignore    │
//! │ commands.rs│ │forwarder │ │ Dispatcher│ │             │
//! └────────────┘ └──────────┘ └───────────┘ └─────────────┘
//!        │            │              │
//!        └────────────┴──────────────┘
//!                     │
//!                     ▼
//! ┌─────────────────────────────────────────┐
//! │          BackgroundAgentTrigger Trait              │
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
//! use restflow_tauri::channel::{
//!     start_message_handler, start_message_handler_with_chat,
//!     MessageHandlerConfig, ChatDispatcher, ChatDispatcherConfig,
//! };
//!
//! // Basic setup (commands + task forwarding only):
//! let router: Arc<ChannelRouter> = /* from state */;
//! let task_trigger: Arc<dyn BackgroundAgentTrigger> = /* your implementation */;
//! start_message_handler(router, task_trigger, MessageHandlerConfig::default());
//!
//! // With AI chat support:
//! let chat_dispatcher: Arc<ChatDispatcher> = /* create dispatcher */;
//! start_message_handler_with_chat(router, task_trigger, chat_dispatcher, config);
//! ```

mod chat_dispatcher;
mod commands;
mod debounce;
mod forwarder;
mod handler;
mod router;
mod trigger;

pub use chat_dispatcher::{ChatDispatcher, ChatDispatcherConfig, ChatError, ChatSessionManager};
pub use debounce::MessageDebouncer;
pub use handler::{MessageHandlerConfig, start_message_handler, start_message_handler_with_chat};
pub use router::{MessageRouter, RouteDecision};
pub use trigger::{BackgroundAgentTrigger, SystemStatus};

// Re-export for convenience
pub use commands::handle_command;
pub use forwarder::forward_to_background_agent;

#[cfg(test)]
pub use trigger::mock;
