//! RestFlow AI - Rust-powered AI Agent Framework
//!
//! This crate provides:
//! - ReAct (Reasoning + Acting) loop for AI agents
//! - Multi-provider LLM client (OpenAI, Anthropic)
//! - Tool registry and execution
//! - Graph engine with runtime decisions
//! - Evaluation engine
//! - Memory system (working memory with sliding window)

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

// Re-export commonly used types
pub use agent::{
    AgentConfig, AgentExecutionEngine, AgentExecutionEngineConfig, AgentExecutor, AgentResult,
    AgentState, AgentStatus, ExecutionResult, ResourceLimits, ResourceUsage, TraceEvent,
};
pub use embedding::{
    EmbeddingCache, EmbeddingConfig, EmbeddingProvider, OpenAIEmbedding, VoyageEmbedding,
};
pub use error::{AiError, Result};
pub use graph::{Graph, GraphNode};
pub use llm::{
    AnthropicClient, ClaudeCodeClient, CodexClient, DefaultLlmClientFactory, GeminiCliClient,
    LlmClient, LlmClientFactory, LlmProvider, Message, ModelSpec, OpenAIClient, OpenCodeClient,
    Role, SwappableLlm,
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
    MemorySearchMatch, MemorySearchTool, MemoryStore, ProcessManager, ProcessTool, ReplySender,
    ReplyTool, SecretResolver, SemanticMemory, SessionCreateRequest, SessionListFilter,
    SessionSearchQuery, SessionStore, SessionTool, SkillContent, SkillInfo, SkillProvider,
    SkillRecord, SkillTool, SkillUpdate, SwitchModelTool, Tool, ToolOutput, ToolRegistry,
    ToolSchema, TranscribeTool, VisionTool,
};
