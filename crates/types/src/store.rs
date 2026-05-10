//! Storage trait abstractions for tools.
//!
//! These traits define the storage interfaces that tools require.
//! Implementations are provided by downstream crates (e.g., runtime).

use std::future::Future;
use std::pin::Pin;

use crate::contracts::request::AgentNode as ContractAgentNode;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config_types::ConfigDocument;
use crate::error::Result;

// ── AgentStore ───────────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize)]
pub struct AgentCreateRequest {
    pub name: String,
    pub agent: ContractAgentNode,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AgentUpdateRequest {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub agent: Option<ContractAgentNode>,
}

pub trait AgentStore: Send + Sync {
    fn list_agents(&self) -> Result<Value>;
    fn get_agent(&self, id: &str) -> Result<Value>;
    fn create_agent(&self, request: AgentCreateRequest) -> Result<Value>;
    fn update_agent(&self, request: AgentUpdateRequest) -> Result<Value>;
    fn delete_agent(&self, id: &str) -> Result<Value>;
}

// ── SessionStore ─────────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize)]
pub struct SessionCreateRequest {
    pub agent_id: String,
    pub model: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub skill_id: Option<String>,
    #[serde(default)]
    pub retention: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SessionSearchQuery {
    pub query: String,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub skill_id: Option<String>,
    #[serde(default)]
    pub include_archived: Option<bool>,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SessionListFilter {
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub skill_id: Option<String>,
    #[serde(default)]
    pub include_messages: Option<bool>,
    #[serde(default)]
    pub include_archived: Option<bool>,
}

pub trait SessionStore: Send + Sync {
    fn list_sessions(&self, filter: SessionListFilter) -> Result<Value>;
    fn get_session(&self, id: &str) -> Result<Value>;
    fn create_session(&self, request: SessionCreateRequest) -> Result<Value>;
    fn archive_session(&self, id: &str) -> Result<Value>;
    fn unarchive_session(&self, id: &str) -> Result<Value>;
    fn purge_session(&self, id: &str) -> Result<Value>;
    fn delete_session(&self, id: &str) -> Result<Value>;
    fn search_sessions(&self, query: SessionSearchQuery) -> Result<Value>;
    fn cleanup_sessions(&self) -> Result<Value>;
}

// ── ProcessManager ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessSessionInfo {
    pub session_id: String,
    pub command: String,
    pub cwd: Option<String>,
    pub started_at: i64,
    pub status: String,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessPollResult {
    pub session_id: String,
    pub output: String,
    pub status: String,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessLog {
    pub session_id: String,
    pub output: String,
    pub offset: usize,
    pub limit: usize,
    pub total: usize,
    pub truncated: bool,
}

pub trait ProcessManager: Send + Sync {
    fn spawn(&self, command: String, cwd: Option<String>) -> anyhow::Result<String>;
    fn poll(&self, session_id: &str) -> anyhow::Result<ProcessPollResult>;
    fn write(&self, session_id: &str, data: &str) -> anyhow::Result<()>;
    fn kill(&self, session_id: &str) -> anyhow::Result<()>;
    fn list(&self) -> anyhow::Result<Vec<ProcessSessionInfo>>;
    fn log(&self, session_id: &str, offset: usize, limit: usize) -> anyhow::Result<ProcessLog>;
}

// ── ReplySender ──────────────────────────────────────────────────────

pub trait ReplySender: Send + Sync {
    fn send(&self, message: String) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>;
}

// ── SecretStore ──────────────────────────────────────────────────────

pub trait SecretStore: Send + Sync {
    fn list_secrets(&self) -> Result<Value>;
    fn get_secret(&self, key: &str) -> Result<Option<String>>;
    fn set_secret(&self, key: &str, value: &str, description: Option<String>) -> Result<()>;
    fn delete_secret(&self, key: &str) -> Result<()>;
    fn has_secret(&self, key: &str) -> Result<bool>;
}

// ── ConfigStore ──────────────────────────────────────────────────────

pub trait ConfigStore: Send + Sync {
    fn get_effective_config(&self) -> Result<ConfigDocument>;
    fn get_writable_config(&self) -> Result<ConfigDocument>;
    fn persist_config(&self, config: &ConfigDocument) -> Result<()>;
    fn reset_config(&self) -> Result<ConfigDocument>;
}

// ── OpsProvider ─────────────────────────────────────────────────────

pub trait OpsProvider: Send + Sync {
    fn daemon_health(&self) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + '_>>;
    fn task_summary(&self, status: Option<&str>, limit: usize) -> Result<Value>;
    fn log_tail(&self, lines: usize, path: Option<&str>) -> Result<Value>;
}
