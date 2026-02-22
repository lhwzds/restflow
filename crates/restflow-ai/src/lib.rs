//! RestFlow AI - Rust-powered AI Agent Framework
//!
//! This crate provides:
//! - ReAct (Reasoning + Acting) loop for AI agents
//! - Multi-provider LLM client (OpenAI, Anthropic)
//! - Tool registry and execution
//! - Evaluation engine
//! - Memory system (working memory with sliding window)

#![allow(dead_code)]

pub mod agent;
pub mod cache;
pub mod embedding;
pub mod error;
pub mod eval;
mod http_client;
pub mod llm;
pub mod lsp;
pub mod security;
pub mod steer;
pub mod text_utils;
pub mod tools;

// Re-export commonly used types
pub use agent::{
    AgentConfig, AgentExecutor, AgentResult, AgentState, AgentStatus, CheckpointDurability,
    ExecutionStep, ResourceLimits, ResourceUsage, Scratchpad, TraceEvent,
};
pub use embedding::{
    EmbeddingCache, EmbeddingConfig, EmbeddingProvider, OpenAIEmbedding, VoyageEmbedding,
};
pub use error::{AiError, Result};
pub use llm::{
    AnthropicClient, ClaudeCodeClient, CodexClient, DefaultLlmClientFactory, GeminiCliClient,
    LlmClient, LlmClientFactory, LlmProvider, Message, ModelSpec, OpenAIClient, OpenCodeClient,
    Role, SwappableLlm,
};
pub use security::{
    NetworkAllowlist, NetworkEcosystem, SecurityDecision, SecurityGate, ToolAction,
};
pub use steer::{SteerMessage, SteerSource};
pub use tools::{
    AgentCreateRequest, AgentCrudTool, AgentStore, AgentUpdateRequest, AuthProfileCreateRequest,
    AuthProfileStore, AuthProfileTestRequest, AuthProfileTool, CredentialInput, DeliverableStore,
    DiagnosticsProvider, DiagnosticsTool, EmailTool, HttpTool, LoggingWrapper, MemoryClearRequest,
    MemoryCompactRequest, MemoryExportRequest, MemoryManagementTool, MemoryManager,
    MemorySearchMatch, MemorySearchTool, MemoryStore, ProcessManager, ProcessTool,
    RateLimitWrapper, ReplySender, ReplyTool, SaveDeliverableTool, SecretResolver, SemanticMemory,
    SessionCreateRequest, SessionListFilter, SessionSearchQuery, SessionStore, SessionTool,
    SkillContent, SkillInfo, SkillProvider, SkillRecord, SkillTool, SkillUpdate, SwitchModelTool,
    TimeoutWrapper, Tool, ToolOutput, ToolRegistry, ToolSchema, ToolWrapper, TranscribeTool,
    VisionTool, WrappedTool,
};
