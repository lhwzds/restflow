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
mod state;
mod step;
mod stream;
mod streaming_buffer;
pub mod stuck;
mod sub_agent;
pub mod team;

/// Default base prompt used when no agent-specific prompt is configured.
pub const DEFAULT_AGENT_PROMPT: &str = "You are a helpful AI assistant.";

pub use checkpoint::{AgentCheckpoint, checkpoint_restore, checkpoint_save};
pub use context::{
    AgentContext, ContextDiscoveryConfig, ContextLoader, DiscoveredContext, MemoryContext,
    SkillSummary, WorkspaceContextCache,
};
pub use deferred::{DeferredExecutionManager, DeferredStatus, DeferredToolCall};
pub use executor::{AgentConfig, AgentExecutor, AgentResult, CheckpointDurability};
pub use model_router::{ModelRoutingConfig, TaskTier, classify_task, select_model};
pub use prompt_flags::PromptFlags;
pub use resource::{ResourceError, ResourceLimits, ResourceTracker, ResourceUsage};
pub use state::{AgentState, AgentStatus};
pub use step::ExecutionStep;
pub use stream::{
    ChannelEmitter, NullEmitter, SharedStreamEmitter, StreamEmitter, ToolCallAccumulator,
};
pub use streaming_buffer::StreamDisplayMode;
pub use stuck::{StuckAction, StuckDetector, StuckDetectorConfig, StuckInfo};
pub use sub_agent::{
    RunTraceContext, SpawnHandle, SpawnPriority, SubagentCompletion, SubagentConfig,
    SubagentDefLookup, SubagentDefSnapshot, SubagentDefSummary, SubagentDeps,
    SubagentExecutionBridge, SubagentManagerImpl, SubagentResult, SubagentSpawner, SubagentState,
    SubagentStatus, SubagentTracker, execute_subagent_plan,
};
pub use team::{extract_team_execution_context, inject_team_execution_context, record_pending_team_approval};
