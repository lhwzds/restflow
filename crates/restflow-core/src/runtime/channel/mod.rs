//! Channel Message Handler Module
//!
//! This module bridges the channel infrastructure (from restflow-core) with
//! the task execution system. It handles:
//!
//! - Processing inbound messages from interactive channels (Telegram, etc.)
//! - Routing commands (/help, /agents, /run, /status, /stop)
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
//!        ┌────────────┼────────────┐
//!        ▼            ▼            ▼
//! ┌────────────┐ ┌───────────┐ ┌─────────────┐
//! │  Commands  │ │   Chat    │ │   Ignore    │
//! │ commands.rs│ │ Dispatcher│ │             │
//! └────────────┘ └───────────┘ └─────────────┘
//!        │              │
//!        └──────────────┘
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
//! // Basic setup (commands only):
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
mod handler;
mod router;
pub mod tool_trace_emitter;
mod trigger;
mod turn_persistence;
mod voice_transcript;

pub use chat_dispatcher::{ChatDispatcher, ChatDispatcherConfig, ChatError, ChatSessionManager};
pub use debounce::MessageDebouncer;
pub use handler::{
    MessageHandlerConfig, MessageHandlerHandle, start_message_handler,
    start_message_handler_with_chat, start_message_handler_with_pairing,
};
pub use router::{MessageRouter, RouteDecision};
pub use tool_trace_emitter::{
    ToolTraceEmitter, ToolTraceSubagentSink, append_message_trace, append_turn_cancelled,
    append_turn_completed, append_turn_completed_with_execution, append_turn_failed,
    append_turn_failed_with_execution, append_turn_started, append_turn_started_with_execution,
    build_execution_steps,
};
pub use trigger::{BackgroundAgentTrigger, SystemStatus};
pub(crate) use turn_persistence::build_turn_persistence_payload;
pub(crate) use voice_transcript::{
    hydrate_voice_message_metadata, replace_latest_user_message_content,
};

// Re-export for convenience
pub use commands::handle_command;

#[cfg(test)]
pub use trigger::mock;
