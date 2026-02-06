//! Memory system for AI agents
//!
//! This module provides memory management for agent conversations:
//!
//! - **Working Memory**: Runtime sliding window for conversation history
//!   (prevents context overflow by auto-discarding old messages)
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Memory System Architecture                │
//! ├─────────────────────────────────────────────────────────────┤
//! │                                                              │
//! │  Working Memory (Runtime)                                    │
//! │  ┌────────────────────────────────────────────────────────┐ │
//! │  │  VecDeque<Message>                                     │ │
//! │  │  max_messages: 100 (configurable)                      │ │
//! │  │  ↓ overflow → discard oldest (no LLM summary)          │ │
//! │  └────────────────────────────────────────────────────────┘ │
//! │                                                              │
//! └─────────────────────────────────────────────────────────────┘
//! ```

mod compaction;
mod working;

pub use compaction::{
    COMPACTION_PROMPT, CategorizedMessages, CompactionConfig, CompactionEvent, CompactionResult,
    CompactionStorage, ContextCompactor,
};
pub use working::{DEFAULT_MAX_MESSAGES, WorkingMemory};
