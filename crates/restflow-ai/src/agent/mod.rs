//! Agent module - ReAct execution strategy
//!
//! ## ReAct (Reasoning + Acting)
//!
//! 1. Think - LLM reasons about the current state
//! 2. Decide - LLM chooses an action
//! 3. Act - Execute the chosen tool
//! 4. Observe - Record the result
//! 5. Repeat until goal is achieved or max iterations

mod checkpoint;
mod context;
pub mod context_manager;
mod deferred;
mod executor;
pub mod model_router;
mod prompt_flags;
mod resource;
mod scratchpad;
mod state;
mod step;
mod stream;
mod streaming_buffer;
pub mod stuck;
mod sub_agent;
mod trace;

/// Default base prompt used when no agent-specific prompt is configured.
pub const DEFAULT_AGENT_PROMPT: &str = "You are a helpful AI assistant.";

pub use checkpoint::{AgentCheckpoint, checkpoint_restore, checkpoint_save};
pub use context::{
    AgentContext, ContextDiscoveryConfig, ContextLoader, DiscoveredContext, MemoryContext,
    SkillSummary, WorkspaceContextCache,
};
pub use deferred::{DeferredExecutionManager, DeferredStatus, DeferredToolCall};
pub use executor::{AgentConfig, AgentExecutor, AgentResult, CheckpointDurability};
pub use model_router::{ModelRoutingConfig, ModelSwitcher, TaskTier, classify_task, select_model};
pub use prompt_flags::PromptFlags;
pub use resource::{ResourceError, ResourceLimits, ResourceTracker, ResourceUsage};
pub use scratchpad::Scratchpad;
pub use state::{AgentState, AgentStatus};
pub use step::ExecutionStep;
pub use stream::{ChannelEmitter, NullEmitter, StreamEmitter, ToolCallAccumulator};
pub use stuck::{StuckAction, StuckDetector, StuckDetectorConfig, StuckInfo};
pub use sub_agent::{
    SpawnHandle, SpawnPriority, SpawnRequest, SubAgentManager, SubagentCompletion, SubagentConfig,
    SubagentDefLookup, SubagentDefSnapshot, SubagentDefSummary, SubagentResult, SubagentState,
    SubagentStatus, SubagentTracker, spawn_subagent,
};
pub use trace::TraceEvent;
