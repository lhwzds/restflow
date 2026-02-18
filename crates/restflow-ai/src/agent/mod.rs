//! Agent module - Pluggable execution strategies
//!
//! ## Default Strategy: ReAct (Reasoning + Acting)
//!
//! 1. Think - LLM reasons about the current state
//! 2. Decide - LLM chooses an action
//! 3. Act - Execute the chosen tool
//! 4. Observe - Record the result
//! 5. Repeat until goal is achieved or max iterations
//!
//! ## Available Strategies
//!
//! | Strategy | Status | Best For |
//! |----------|--------|----------|
//! | ReAct | ✅ Implemented | General tasks |
//! | Pre-Act | 🚧 Planned | Cost optimization |
//! | Reflexion | 🚧 Planned | Learning from failures |
//! | Hierarchical | 🚧 Planned | Complex multi-part tasks |
//! | Swarm | 🚧 Planned | Multi-agent collaboration |
//! | Tree-of-Thought | 🚧 Planned | Creative problem solving |
//!
//! ## Usage
//!
//! ```rust,ignore
//! use restflow_ai::agent::strategy::{AgentStrategyFactory, StrategyType};
//!
//! // Use default (ReAct)
//! let agent = AgentStrategyFactory::default(llm, tools);
//! let result = agent.execute(config).await?;
//!
//! // Use specific strategy
//! let agent = AgentStrategyFactory::create(StrategyType::PreAct, llm, tools);
//! let result = agent.execute(config).await?;
//! ```

mod checkpoint;
mod context;
mod deferred;
mod definitions;
mod execution_engine;
mod executor;
mod history;
pub mod model_router;
mod prompt_flags;
pub mod react;
mod resource;
mod scratchpad;
mod state;
mod step;
pub mod strategy;
mod stream;
mod streaming_buffer;
pub mod stuck;
mod sub_agent;
mod trace;

pub use checkpoint::{AgentCheckpoint, checkpoint_restore, checkpoint_save};
pub use context::{
    AgentContext, ContextDiscoveryConfig, ContextLoader, DiscoveredContext, MemoryContext,
    SkillSummary, WorkspaceContextCache,
};
pub use deferred::{DeferredExecutionManager, DeferredStatus, DeferredToolCall};
pub use definitions::{AgentDefinition, AgentDefinitionRegistry, builtin_agents};
pub use execution_engine::{AgentExecutionEngine, AgentExecutionEngineConfig, ExecutionResult};
pub use executor::{AgentConfig, AgentExecutor, AgentResult, CheckpointDurability};
pub use history::{HistoryPipeline, HistoryProcessor, TrimOldMessagesProcessor};
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
    SubagentResult, SubagentState, SubagentStatus, SubagentTracker, spawn_subagent,
};
pub use trace::TraceEvent;
