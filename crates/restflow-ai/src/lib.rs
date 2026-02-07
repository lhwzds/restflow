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

#![allow(dead_code)]

pub mod agent;
pub mod cache;
pub mod embedding;
pub mod error;
pub mod eval;
pub mod graph;
mod http_client;
pub mod llm;
pub mod lsp;
pub mod memory;
pub mod security;
pub mod steer;
pub mod tools;
pub mod workflow_def;

#[cfg(feature = "python")]
pub mod python;

// Re-export commonly used types
pub use agent::{
    AgentConfig, AgentExecutor, AgentResult, AgentState, AgentStatus, ExecutionResult, TraceEvent,
    UnifiedAgent, UnifiedAgentConfig,
};
pub use embedding::{
    EmbeddingCache, EmbeddingConfig, EmbeddingProvider, OpenAIEmbedding, VoyageEmbedding,
};
pub use error::{AiError, Result};
pub use graph::{Graph, GraphNode};
pub use llm::{
    AnthropicClient, ClaudeCodeClient, CodexClient, DefaultLlmClientFactory, LlmClient,
    LlmClientFactory, LlmProvider, Message, ModelSpec, OpenAIClient, OpenCodeClient, Role,
    SwappableLlm,
};
pub use memory::{
    CompactionConfig, CompactionResult, ContextCompactor, DEFAULT_MAX_MESSAGES, WorkingMemory,
};
pub use security::{SecurityDecision, SecurityGate, ToolAction};
pub use steer::{SteerMessage, SteerSource};
pub use tools::{
    AgentCreateRequest, AgentCrudTool, AgentStore, AgentUpdateRequest, AuthProfileCreateRequest,
    AuthProfileStore, AuthProfileTestRequest, AuthProfileTool, CredentialInput,
    DiagnosticsProvider, DiagnosticsTool, EmailTool, HttpTool, MemoryClearRequest,
    MemoryCompactRequest, MemoryExportRequest, MemoryManagementTool, MemoryManager,
    MemorySearchMatch, MemorySearchTool, PythonTool, SecretResolver, SemanticMemory,
    SessionCreateRequest, SessionListFilter, SessionSearchQuery, SessionStore, SessionTool,
    SkillContent, SkillInfo, SkillProvider, SkillRecord, SkillTool, SkillUpdate, SwitchModelTool,
    Tool, ToolOutput, ToolRegistry, ToolSchema, TranscribeTool, VisionTool,
};
pub use workflow_def::WorkflowDef;
