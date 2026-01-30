//! RestFlow AI - Rust-powered AI Agent Framework
//!
//! This crate provides:
//! - ReAct (Reasoning + Acting) loop for AI agents
//! - Multi-provider LLM client (OpenAI, Anthropic)
//! - Tool registry and execution
//! - Dynamic workflow definition (WorkflowDef)
//! - Graph engine with runtime decisions
//! - Evaluation engine
//! - Memory system (working memory with sliding window)
//! - Python SDK via PyO3

pub mod agent;
pub mod error;
pub mod eval;
pub mod graph;
mod http_client;
pub mod llm;
pub mod memory;
pub mod tools;
pub mod workflow_def;

#[cfg(feature = "python")]
pub mod python;

// Re-export commonly used types
pub use agent::{AgentConfig, AgentExecutor, AgentResult, AgentState, AgentStatus, TraceEvent};
pub use error::{AiError, Result};
pub use graph::{Graph, GraphNode};
pub use llm::{AnthropicClient, LlmClient, Message, OpenAIClient, Role};
pub use tools::{
    EmailTool, HttpTool, PythonTool, SkillContent, SkillInfo, SkillProvider, SkillTool, Tool,
    ToolOutput, ToolRegistry, ToolSchema,
};
pub use memory::{WorkingMemory, DEFAULT_MAX_MESSAGES};
pub use workflow_def::WorkflowDef;
