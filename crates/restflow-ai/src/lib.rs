//! RestFlow AI - Rust-powered AI Agent Framework
//!
//! This crate provides:
//! - Core tool abstractions (Tool, ToolError, ToolRegistry, SecurityGate, etc.)
//! - ReAct (Reasoning + Acting) loop for AI agents
//! - Multi-provider LLM client (OpenAI, Anthropic)
//! - Evaluation engine
//! - Memory system (working memory with sliding window)

pub mod agent;
pub mod cache;
pub mod error;
pub mod llm;
pub mod steer;
pub mod text_utils;
pub mod tools;

// Re-export commonly used types
pub use agent::{
    AgentConfig, AgentExecutor, AgentResult, AgentState, AgentStatus, CheckpointDurability,
    ExecutionStep, ResourceLimits, ResourceUsage, Scratchpad, SubagentDeps, SubagentManagerImpl,
    SubagentSpawner,
};
pub use agent::context_manager::{
    CompactStats, ContextManagerConfig, PruneStats, TokenEstimator,
};
pub use error::{AiError, Result};
pub use llm::{
    AnthropicClient, ClaudeCodeClient, CodexClient, DefaultLlmClientFactory, GeminiCliClient,
    LlmClient, LlmClientFactory, LlmProvider, LlmSwitcherImpl, Message, ModelSpec, OpenAIClient,
    OpenCodeClient, Role, SwappableLlm,
};
pub use steer::{SteerMessage, SteerSource};
// Core tool abstractions
pub use tools::{
    // Error types
    ToolError,
    // Tool trait and core types
    SecretResolver, Tool, ToolErrorCategory, ToolOutput, ToolSchema, check_security,
    // Registry and toolset
    ToolRegistry, Toolset, ToolsetContext,
    // Wrappers
    LoggingWrapper, RateLimitWrapper, TimeoutWrapper, ToolWrapper, WrappedTool,
};
