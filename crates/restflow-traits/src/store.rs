//! Storage trait abstractions for tools.
//!
//! These traits define the storage interfaces that tools require.
//! Implementations are provided by downstream crates (e.g., restflow-core).

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::Result;

// ── MemoryStore ──────────────────────────────────────────────────────

pub trait MemoryStore: Send + Sync {
    fn save(&self, agent_id: &str, title: &str, content: &str, tags: &[String]) -> Result<Value>;
    fn read_by_id(&self, id: &str) -> Result<Option<Value>>;
    fn search(
        &self,
        agent_id: &str,
        tag: Option<&str>,
        search: Option<&str>,
        limit: usize,
    ) -> Result<Value>;
    fn list(&self, agent_id: &str, tag: Option<&str>, limit: usize) -> Result<Value>;
    fn delete(&self, id: &str) -> Result<Value>;
}

// ── MemoryManager ────────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize)]
pub struct MemoryExportRequest {
    pub agent_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub options: Option<Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MemoryClearRequest {
    pub agent_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub delete_sessions: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MemoryCompactRequest {
    pub agent_id: String,
    #[serde(default)]
    pub keep_recent: Option<u32>,
    #[serde(default)]
    pub before_ms: Option<i64>,
}

pub trait MemoryManager: Send + Sync {
    fn stats(&self, agent_id: &str) -> Result<Value>;
    fn export(&self, request: MemoryExportRequest) -> Result<Value>;
    fn clear(&self, request: MemoryClearRequest) -> Result<Value>;
    fn compact(&self, request: MemoryCompactRequest) -> Result<Value>;
}

// ── AgentStore ───────────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize)]
pub struct AgentCreateRequest {
    pub name: String,
    pub agent: Value,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AgentUpdateRequest {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub agent: Option<Value>,
}

pub trait AgentStore: Send + Sync {
    fn list_agents(&self) -> Result<Value>;
    fn get_agent(&self, id: &str) -> Result<Value>;
    fn create_agent(&self, request: AgentCreateRequest) -> Result<Value>;
    fn update_agent(&self, request: AgentUpdateRequest) -> Result<Value>;
    fn delete_agent(&self, id: &str) -> Result<Value>;
}

// ── BackgroundAgentStore ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackgroundAgentCreateRequest {
    pub name: String,
    pub agent_id: String,
    #[serde(default)]
    pub schedule: Option<Value>,
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub input_template: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub durability_mode: Option<String>,
    #[serde(default)]
    pub memory: Option<Value>,
    #[serde(default)]
    pub memory_scope: Option<String>,
    #[serde(default)]
    pub resource_limits: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackgroundAgentUpdateRequest {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub input_template: Option<String>,
    #[serde(default)]
    pub schedule: Option<Value>,
    #[serde(default)]
    pub notification: Option<Value>,
    #[serde(default)]
    pub execution_mode: Option<Value>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub durability_mode: Option<String>,
    #[serde(default)]
    pub memory: Option<Value>,
    #[serde(default)]
    pub memory_scope: Option<String>,
    #[serde(default)]
    pub resource_limits: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackgroundAgentControlRequest {
    pub id: String,
    pub action: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackgroundAgentProgressRequest {
    pub id: String,
    #[serde(default)]
    pub event_limit: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackgroundAgentMessageRequest {
    pub id: String,
    pub message: String,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackgroundAgentMessageListRequest {
    pub id: String,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackgroundAgentDeliverableListRequest {
    pub id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackgroundAgentScratchpadListRequest {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackgroundAgentScratchpadReadRequest {
    pub scratchpad: String,
    #[serde(default)]
    pub line_limit: Option<usize>,
}

pub trait BackgroundAgentStore: Send + Sync {
    fn create_background_agent(&self, request: BackgroundAgentCreateRequest) -> Result<Value>;
    fn update_background_agent(&self, request: BackgroundAgentUpdateRequest) -> Result<Value>;
    fn delete_background_agent(&self, id: &str) -> Result<Value>;
    fn list_background_agents(&self, status: Option<String>) -> Result<Value>;
    fn control_background_agent(&self, request: BackgroundAgentControlRequest) -> Result<Value>;
    fn get_background_agent_progress(
        &self,
        request: BackgroundAgentProgressRequest,
    ) -> Result<Value>;
    fn send_background_agent_message(
        &self,
        request: BackgroundAgentMessageRequest,
    ) -> Result<Value>;
    fn list_background_agent_messages(
        &self,
        request: BackgroundAgentMessageListRequest,
    ) -> Result<Value>;
    fn list_background_agent_deliverables(
        &self,
        request: BackgroundAgentDeliverableListRequest,
    ) -> Result<Value>;
    fn list_background_agent_scratchpads(
        &self,
        request: BackgroundAgentScratchpadListRequest,
    ) -> Result<Value>;
    fn read_background_agent_scratchpad(
        &self,
        request: BackgroundAgentScratchpadReadRequest,
    ) -> Result<Value>;
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
}

pub trait SessionStore: Send + Sync {
    fn list_sessions(&self, filter: SessionListFilter) -> Result<Value>;
    fn get_session(&self, id: &str) -> Result<Value>;
    fn create_session(&self, request: SessionCreateRequest) -> Result<Value>;
    fn delete_session(&self, id: &str) -> Result<Value>;
    fn search_sessions(&self, query: SessionSearchQuery) -> Result<Value>;
    fn cleanup_sessions(&self) -> Result<Value>;
}

// ── AuthProfileStore ─────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CredentialInput {
    ApiKey {
        key: String,
        #[serde(default)]
        email: Option<String>,
    },
    Token {
        token: String,
        #[serde(default)]
        expires_at: Option<String>,
        #[serde(default)]
        email: Option<String>,
    },
    OAuth {
        access_token: String,
        #[serde(default)]
        refresh_token: Option<String>,
        #[serde(default)]
        expires_at: Option<String>,
        #[serde(default)]
        email: Option<String>,
    },
}

impl CredentialInput {
    pub fn expires_at(&self) -> Result<Option<DateTime<Utc>>> {
        match self {
            CredentialInput::Token { expires_at, .. }
            | CredentialInput::OAuth { expires_at, .. } => {
                if let Some(value) = expires_at {
                    let dt = DateTime::parse_from_rfc3339(value)
                        .map(|dt| dt.with_timezone(&Utc))
                        .map_err(|e| {
                            crate::error::ToolError::Tool(format!(
                                "Invalid timestamp: {}",
                                e
                            ))
                        })?;
                    Ok(Some(dt))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct AuthProfileCreateRequest {
    pub name: String,
    pub provider: String,
    #[serde(default)]
    pub source: Option<String>,
    pub credential: CredentialInput,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AuthProfileTestRequest {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
}

pub trait AuthProfileStore: Send + Sync {
    fn list_profiles(&self) -> Result<Value>;
    fn discover_profiles(&self) -> Result<Value>;
    fn add_profile(&self, request: AuthProfileCreateRequest) -> Result<Value>;
    fn remove_profile(&self, id: &str) -> Result<Value>;
    fn test_profile(&self, request: AuthProfileTestRequest) -> Result<Value>;
}

// ── DeliverableStore ─────────────────────────────────────────────────

pub trait DeliverableStore: Send + Sync {
    #[allow(clippy::too_many_arguments)]
    fn save_deliverable(
        &self,
        task_id: &str,
        execution_id: &str,
        deliverable_type: &str,
        title: &str,
        content: &str,
        file_path: Option<&str>,
        content_type: Option<&str>,
        metadata: Option<Value>,
    ) -> Result<Value>;
}

// ── WorkItemProvider ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemStatus {
    Open,
    InProgress,
    Done,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkItemRecord {
    pub id: String,
    pub folder: String,
    pub title: String,
    pub content: String,
    pub priority: Option<String>,
    pub status: WorkItemStatus,
    pub tags: Vec<String>,
    pub assignee: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkItemSpec {
    pub folder: String,
    pub title: String,
    pub content: String,
    pub priority: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkItemPatch {
    pub title: Option<String>,
    pub content: Option<String>,
    pub priority: Option<String>,
    pub status: Option<WorkItemStatus>,
    pub tags: Option<Vec<String>>,
    pub assignee: Option<String>,
    pub folder: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkItemQuery {
    pub folder: Option<String>,
    pub status: Option<WorkItemStatus>,
    pub priority: Option<String>,
    pub tag: Option<String>,
    pub assignee: Option<String>,
    pub search: Option<String>,
}

pub trait WorkItemProvider: Send + Sync {
    fn create(&self, spec: WorkItemSpec) -> std::result::Result<WorkItemRecord, String>;
    fn get(&self, id: &str) -> std::result::Result<Option<WorkItemRecord>, String>;
    fn update(
        &self,
        id: &str,
        patch: WorkItemPatch,
    ) -> std::result::Result<WorkItemRecord, String>;
    fn delete(&self, id: &str) -> std::result::Result<bool, String>;
    fn list(
        &self,
        query: WorkItemQuery,
    ) -> std::result::Result<Vec<WorkItemRecord>, String>;
    fn list_folders(&self) -> std::result::Result<Vec<String>, String>;
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

// ── DiagnosticsProvider ──────────────────────────────────────────────

#[async_trait]
pub trait DiagnosticsProvider: Send + Sync {
    async fn ensure_open(&self, path: &Path) -> Result<()>;
    async fn did_change(&self, path: &Path, content: &str) -> Result<()>;
    async fn wait_for_diagnostics(
        &self,
        path: &Path,
        timeout: Duration,
    ) -> Result<Vec<lsp_types::Diagnostic>>;
    async fn get_diagnostics(&self, path: &Path) -> Result<Vec<lsp_types::Diagnostic>>;
}

// ── ReplySender ──────────────────────────────────────────────────────

pub trait ReplySender: Send + Sync {
    fn send(&self, message: String) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>;
}

// ── SecurityQueryProvider ───────────────────────────────────────────

pub trait SecurityQueryProvider: Send + Sync {
    fn show_policy(&self) -> Result<Value>;
    fn list_permissions(&self) -> Result<Value>;
    fn check_permission(
        &self,
        tool_name: &str,
        operation_name: &str,
        target: Option<&str>,
        summary: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + '_>>;
}

// ── TriggerStore ────────────────────────────────────────────────────

pub trait TriggerStore: Send + Sync {
    fn create_trigger(
        &self,
        workflow_id: &str,
        config: Value,
        id: Option<&str>,
    ) -> Result<Value>;
    fn list_triggers(&self) -> Result<Value>;
    fn delete_trigger(&self, id: &str) -> Result<Value>;
}

// ── TerminalStore ───────────────────────────────────────────────────

pub trait TerminalStore: Send + Sync {
    fn create_session(
        &self,
        name: Option<&str>,
        working_dir: Option<&str>,
        startup_cmd: Option<&str>,
    ) -> Result<Value>;
    fn list_sessions(&self) -> Result<Value>;
    fn send_input(&self, session_id: &str, data: &str) -> Result<Value>;
    fn read_output(&self, session_id: &str) -> Result<Value>;
    fn close_session(&self, session_id: &str) -> Result<Value>;
}

// ── UnifiedMemorySearch ─────────────────────────────────────────────

pub trait UnifiedMemorySearch: Send + Sync {
    fn search(
        &self,
        agent_id: &str,
        query: &str,
        include_sessions: bool,
        limit: u32,
        offset: u32,
    ) -> Result<Value>;
}

// ── KvStore ─────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub trait KvStore: Send + Sync {
    fn get_entry(&self, key: &str) -> Result<Value>;
    #[allow(clippy::too_many_arguments)]
    fn set_entry(
        &self,
        key: &str,
        content: &str,
        visibility: Option<&str>,
        content_type: Option<&str>,
        type_hint: Option<&str>,
        tags: Option<Vec<String>>,
        accessor_id: Option<&str>,
    ) -> Result<Value>;
    fn delete_entry(&self, key: &str, accessor_id: Option<&str>) -> Result<Value>;
    fn list_entries(&self, namespace: Option<&str>) -> Result<Value>;
}

// ── MarketplaceStore ────────────────────────────────────────────────

#[async_trait]
#[allow(clippy::too_many_arguments)]
pub trait MarketplaceStore: Send + Sync {
    #[allow(clippy::too_many_arguments)]
    async fn search_skills(
        &self,
        query: Option<&str>,
        category: Option<&str>,
        tags: Option<Vec<String>>,
        author: Option<&str>,
        limit: Option<usize>,
        offset: Option<usize>,
        source: Option<&str>,
    ) -> Result<Value>;
    async fn skill_info(&self, id: &str, source: Option<&str>) -> Result<Value>;
    async fn install_skill(
        &self,
        id: &str,
        source: Option<&str>,
        overwrite: bool,
    ) -> Result<Value>;
    fn uninstall_skill(&self, id: &str) -> Result<Value>;
    fn list_installed(&self) -> Result<Value>;
}

// ── OpsProvider ─────────────────────────────────────────────────────

pub trait OpsProvider: Send + Sync {
    fn daemon_status(&self) -> Result<Value>;
    fn daemon_health(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + '_>>;
    fn background_summary(
        &self,
        status: Option<&str>,
        limit: usize,
    ) -> Result<Value>;
    fn session_summary(&self, limit: usize) -> Result<Value>;
    fn log_tail(&self, lines: usize, path: Option<&str>) -> Result<Value>;
}
