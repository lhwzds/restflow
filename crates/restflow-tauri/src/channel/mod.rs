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
//! │        BackgroundAgentRunner / AppState       │
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

pub use restflow_core::runtime::channel::{
    BackgroundAgentTrigger, ChatDispatcher, ChatDispatcherConfig, ChatError, ChatSessionManager,
    MessageDebouncer, MessageHandlerConfig, MessageRouter, RouteDecision, SystemStatus,
    forward_to_background_agent, handle_command, start_message_handler,
    start_message_handler_with_chat,
};
