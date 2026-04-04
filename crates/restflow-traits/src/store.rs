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
use restflow_contracts::request::{
    AgentNode as ContractAgentNode, DurabilityMode as ContractDurabilityMode,
    ExecutionMode as ContractExecutionMode, MemoryConfig as ContractMemoryConfig,
    NotificationConfig as ContractNotificationConfig, ResourceLimits as ContractResourceLimits,
    TaskSchedule as ContractTaskSchedule,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config_types::ConfigDocument;
use crate::error::Result;

pub const MANAGE_TASK_OPERATIONS: &[&str] = &[
    "create",
    "convert_session",
    "promote_to_background",
    "run_batch",
    "save_team",
    "list_teams",
    "get_team",
    "delete_team",
    "update",
    "delete",
    "list",
    "control",
    "progress",
    "send_message",
    "list_messages",
    "list_deliverables",
    "list_traces",
    "read_trace",
    "pause",
    "resume",
    "stop",
    "run",
];

pub const MANAGE_BACKGROUND_AGENT_OPERATIONS: &[&str] = MANAGE_TASK_OPERATIONS;

pub const MANAGE_TASK_OPERATIONS_CSV: &str = "create, convert_session, promote_to_background, run_batch, save_team, list_teams, get_team, delete_team, update, delete, list, control, progress, send_message, list_messages, list_deliverables, list_traces, read_trace, pause, resume, stop, run";

pub const MANAGE_BACKGROUND_AGENT_OPERATIONS_CSV: &str = MANAGE_TASK_OPERATIONS_CSV;

pub const MANAGE_TASKS_TOOL_DESCRIPTION: &str = "Manage tasks. CRITICAL: create only defines the task, to immediately execute use 'run' operation. Operations: create (define new task, does NOT run), convert_session (convert an existing chat session into a task), promote_to_background (promote current interactive session into a task), run_batch (create multiple tasks from workers/team and optionally trigger run_now), save_team/list_teams/get_team/delete_team (manage reusable batch templates), run (trigger now), pause/resume (toggle schedule), stop (interrupt current/future execution without deleting the definition), delete (remove definition; auto-created bound chat session is archived when safe), list (browse tasks), progress (execution history), send_message/list_messages (interact with running tasks), list_deliverables (read typed outputs), list_traces/read_trace (diagnose execution traces).";

pub const MANAGE_BACKGROUND_AGENTS_TOOL_DESCRIPTION: &str = MANAGE_TASKS_TOOL_DESCRIPTION;

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

// ── TaskStore ────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskCreateRequest {
    pub name: String,
    pub agent_id: String,
    #[serde(default)]
    pub chat_session_id: Option<String>,
    pub schedule: ContractTaskSchedule,
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub input_template: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub durability_mode: Option<ContractDurabilityMode>,
    #[serde(default)]
    pub memory: Option<ContractMemoryConfig>,
    #[serde(default)]
    pub memory_scope: Option<String>,
    #[serde(default)]
    pub resource_limits: Option<ContractResourceLimits>,
    #[serde(default)]
    pub preview: bool,
    #[serde(default)]
    pub approval_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskConvertSessionRequest {
    pub session_id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub schedule: Option<ContractTaskSchedule>,
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub durability_mode: Option<ContractDurabilityMode>,
    #[serde(default)]
    pub memory: Option<ContractMemoryConfig>,
    #[serde(default)]
    pub memory_scope: Option<String>,
    #[serde(default)]
    pub resource_limits: Option<ContractResourceLimits>,
    #[serde(default)]
    pub run_now: Option<bool>,
    #[serde(default)]
    pub preview: bool,
    #[serde(default)]
    pub approval_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskUpdateRequest {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub chat_session_id: Option<String>,
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub input_template: Option<String>,
    #[serde(default)]
    pub schedule: Option<ContractTaskSchedule>,
    #[serde(default)]
    pub notification: Option<ContractNotificationConfig>,
    #[serde(default)]
    pub execution_mode: Option<ContractExecutionMode>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub durability_mode: Option<ContractDurabilityMode>,
    #[serde(default)]
    pub memory: Option<ContractMemoryConfig>,
    #[serde(default)]
    pub memory_scope: Option<String>,
    #[serde(default)]
    pub resource_limits: Option<ContractResourceLimits>,
    #[serde(default)]
    pub preview: bool,
    #[serde(default)]
    pub approval_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskControlRequest {
    pub id: String,
    pub action: String,
    #[serde(default)]
    pub preview: bool,
    #[serde(default)]
    pub approval_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskDeleteRequest {
    pub id: String,
    #[serde(default)]
    pub preview: bool,
    #[serde(default)]
    pub approval_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskProgressRequest {
    pub id: String,
    #[serde(default)]
    pub event_limit: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskMessageRequest {
    pub id: String,
    pub message: String,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskMessageListRequest {
    pub id: String,
    #[serde(default)]
    pub limit: Option<usize>,
}

pub type BackgroundAgentCreateRequest = TaskCreateRequest;
pub type BackgroundAgentConvertSessionRequest = TaskConvertSessionRequest;
pub type BackgroundAgentUpdateRequest = TaskUpdateRequest;
pub type BackgroundAgentControlRequest = TaskControlRequest;
pub type BackgroundAgentDeleteRequest = TaskDeleteRequest;
pub type BackgroundAgentProgressRequest = TaskProgressRequest;
pub type BackgroundAgentMessageRequest = TaskMessageRequest;
pub type BackgroundAgentMessageListRequest = TaskMessageListRequest;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    #[test]
    fn background_agent_requests_round_trip_with_approval_id() {
        let create: TaskCreateRequest = serde_json::from_value(json!({
            "name": "Task",
            "agent_id": "agent-1",
            "schedule": {
                "type": "interval",
                "interval_ms": 1000
            },
            "preview": true,
            "approval_id": "approval-1"
        }))
        .expect("create request should deserialize");
        assert_eq!(create.approval_id.as_deref(), Some("approval-1"));

        let convert: TaskConvertSessionRequest = serde_json::from_value(json!({
            "session_id": "session-1",
            "preview": true,
            "approval_id": "approval-2"
        }))
        .expect("convert request should deserialize");
        assert_eq!(convert.approval_id.as_deref(), Some("approval-2"));

        let update: TaskUpdateRequest = serde_json::from_value(json!({
            "id": "task-1",
            "preview": true,
            "approval_id": "approval-3"
        }))
        .expect("update request should deserialize");
        assert_eq!(update.approval_id.as_deref(), Some("approval-3"));

        let control: TaskControlRequest = serde_json::from_value(json!({
            "id": "task-1",
            "action": "run_now",
            "approval_id": "approval-4"
        }))
        .expect("control request should deserialize");
        assert_eq!(control.approval_id.as_deref(), Some("approval-4"));

        let delete: TaskDeleteRequest = serde_json::from_value(json!({
            "id": "task-1",
            "approval_id": "approval-5"
        }))
        .expect("delete request should deserialize");
        assert_eq!(delete.approval_id.as_deref(), Some("approval-5"));
    }

    #[test]
    fn task_request_aliases_round_trip_with_approval_id() {
        let create: TaskCreateRequest = serde_json::from_value(json!({
            "name": "Task",
            "agent_id": "agent-1",
            "schedule": {
                "type": "interval",
                "interval_ms": 1000
            },
            "preview": true,
            "approval_id": "approval-1"
        }))
        .expect("task create request should deserialize");
        assert_eq!(create.approval_id.as_deref(), Some("approval-1"));

        let delete: TaskDeleteRequest = serde_json::from_value(json!({
            "id": "task-1",
            "approval_id": "approval-5"
        }))
        .expect("task delete request should deserialize");
        assert_eq!(delete.approval_id.as_deref(), Some("approval-5"));
    }

    #[derive(Default)]
    struct MockBackgroundAgentStore {
        calls: Arc<Mutex<Vec<&'static str>>>,
    }

    impl MockBackgroundAgentStore {
        fn calls(&self) -> Vec<&'static str> {
            self.calls.lock().expect("calls lock").clone()
        }
    }

    impl BackgroundAgentStore for MockBackgroundAgentStore {
        fn create_background_agent(&self, request: BackgroundAgentCreateRequest) -> Result<Value> {
            self.calls
                .lock()
                .expect("calls lock")
                .push("create_background_agent");
            assert_eq!(request.name, "Task");
            Ok(json!({"ok": true}))
        }

        fn convert_session_to_background_agent(
            &self,
            _request: BackgroundAgentConvertSessionRequest,
        ) -> Result<Value> {
            panic!("not expected")
        }

        fn update_background_agent(&self, _request: BackgroundAgentUpdateRequest) -> Result<Value> {
            panic!("not expected")
        }

        fn delete_background_agent(&self, _request: BackgroundAgentDeleteRequest) -> Result<Value> {
            panic!("not expected")
        }

        fn list_background_agents(&self, status: Option<String>) -> Result<Value> {
            self.calls
                .lock()
                .expect("calls lock")
                .push("list_background_agents");
            assert_eq!(status.as_deref(), Some("active"));
            Ok(json!([{"id": "task-1"}]))
        }

        fn control_background_agent(
            &self,
            _request: BackgroundAgentControlRequest,
        ) -> Result<Value> {
            panic!("not expected")
        }

        fn get_background_agent_progress(
            &self,
            _request: BackgroundAgentProgressRequest,
        ) -> Result<Value> {
            panic!("not expected")
        }

        fn send_background_agent_message(
            &self,
            _request: BackgroundAgentMessageRequest,
        ) -> Result<Value> {
            panic!("not expected")
        }

        fn list_background_agent_messages(
            &self,
            _request: BackgroundAgentMessageListRequest,
        ) -> Result<Value> {
            panic!("not expected")
        }

        fn list_background_agent_deliverables(
            &self,
            _request: BackgroundAgentDeliverableListRequest,
        ) -> Result<Value> {
            panic!("not expected")
        }

        fn list_background_agent_traces(
            &self,
            _request: BackgroundAgentTraceListRequest,
        ) -> Result<Value> {
            panic!("not expected")
        }

        fn read_background_agent_trace(
            &self,
            _request: BackgroundAgentTraceReadRequest,
        ) -> Result<Value> {
            panic!("not expected")
        }
    }

    #[test]
    fn task_store_forwards_to_background_agent_store() {
        let store = MockBackgroundAgentStore::default();

        let create_result = TaskStore::create_task(
            &store,
            TaskCreateRequest {
                name: "Task".to_string(),
                agent_id: "agent-1".to_string(),
                chat_session_id: None,
                schedule: ContractTaskSchedule::Interval {
                    interval_ms: 1_000,
                    start_at: None,
                },
                input: None,
                input_template: None,
                timeout_secs: None,
                durability_mode: None,
                memory: None,
                memory_scope: None,
                resource_limits: None,
                preview: false,
                approval_id: None,
            },
        )
        .expect("create_task should forward");
        assert_eq!(create_result["ok"], true);

        let list_result = TaskStore::list_tasks(&store, Some("active".to_string()))
            .expect("list_tasks should forward");
        assert_eq!(list_result.as_array().map(|items| items.len()), Some(1));
        assert_eq!(
            store.calls(),
            vec!["create_background_agent", "list_background_agents"]
        );
    }

    #[test]
    fn task_store_trait_object_forwards_to_background_agent_store() {
        let store: Arc<dyn BackgroundAgentStore> = Arc::new(MockBackgroundAgentStore::default());

        let result = TaskStore::list_tasks(store.as_ref(), Some("active".to_string()))
            .expect("list_tasks should forward through trait object");

        assert_eq!(result.as_array().map(|items| items.len()), Some(1));
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskDeliverableListRequest {
    pub id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskTraceListRequest {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskTraceReadRequest {
    pub trace_id: String,
    #[serde(default)]
    pub line_limit: Option<usize>,
}

pub type BackgroundAgentDeliverableListRequest = TaskDeliverableListRequest;
pub type BackgroundAgentTraceListRequest = TaskTraceListRequest;
pub type BackgroundAgentTraceReadRequest = TaskTraceReadRequest;

pub trait BackgroundAgentStore: Send + Sync {
    fn create_background_agent(&self, request: BackgroundAgentCreateRequest) -> Result<Value>;
    fn convert_session_to_background_agent(
        &self,
        request: BackgroundAgentConvertSessionRequest,
    ) -> Result<Value>;
    fn update_background_agent(&self, request: BackgroundAgentUpdateRequest) -> Result<Value>;
    fn delete_background_agent(&self, request: BackgroundAgentDeleteRequest) -> Result<Value>;
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
    fn list_background_agent_traces(
        &self,
        request: BackgroundAgentTraceListRequest,
    ) -> Result<Value>;
    fn read_background_agent_trace(
        &self,
        request: BackgroundAgentTraceReadRequest,
    ) -> Result<Value>;
}

pub trait TaskStore: BackgroundAgentStore + Send + Sync {
    fn create_task(&self, request: TaskCreateRequest) -> Result<Value> {
        self.create_background_agent(request)
    }

    fn convert_session_to_task(&self, request: TaskConvertSessionRequest) -> Result<Value> {
        self.convert_session_to_background_agent(request)
    }

    fn update_task(&self, request: TaskUpdateRequest) -> Result<Value> {
        self.update_background_agent(request)
    }

    fn delete_task(&self, request: TaskDeleteRequest) -> Result<Value> {
        self.delete_background_agent(request)
    }

    fn list_tasks(&self, status: Option<String>) -> Result<Value> {
        self.list_background_agents(status)
    }

    fn control_task(&self, request: TaskControlRequest) -> Result<Value> {
        self.control_background_agent(request)
    }

    fn get_task_progress(&self, request: TaskProgressRequest) -> Result<Value> {
        self.get_background_agent_progress(request)
    }

    fn send_task_message(&self, request: TaskMessageRequest) -> Result<Value> {
        self.send_background_agent_message(request)
    }

    fn list_task_messages(&self, request: TaskMessageListRequest) -> Result<Value> {
        self.list_background_agent_messages(request)
    }

    fn list_task_deliverables(&self, request: TaskDeliverableListRequest) -> Result<Value> {
        self.list_background_agent_deliverables(request)
    }

    fn list_task_traces(&self, request: TaskTraceListRequest) -> Result<Value> {
        self.list_background_agent_traces(request)
    }

    fn read_task_trace(&self, request: TaskTraceReadRequest) -> Result<Value> {
        self.read_background_agent_trace(request)
    }
}

impl<T: ?Sized> TaskStore for T where T: BackgroundAgentStore + Send + Sync {}

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
                            crate::error::ToolError::Tool(format!("Invalid timestamp: {}", e))
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
    fn update(&self, id: &str, patch: WorkItemPatch)
    -> std::result::Result<WorkItemRecord, String>;
    fn delete(&self, id: &str) -> std::result::Result<bool, String>;
    fn list(&self, query: WorkItemQuery) -> std::result::Result<Vec<WorkItemRecord>, String>;
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
    fn create_trigger(&self, workflow_id: &str, config: Value, id: Option<&str>) -> Result<Value>;
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
    async fn install_skill(&self, id: &str, source: Option<&str>, overwrite: bool)
    -> Result<Value>;
    fn uninstall_skill(&self, id: &str) -> Result<Value>;
    fn list_installed(&self) -> Result<Value>;
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
    fn daemon_status(&self) -> Result<Value>;
    fn daemon_health(&self) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + '_>>;
    fn background_summary(&self, status: Option<&str>, limit: usize) -> Result<Value>;
    fn session_summary(&self, limit: usize) -> Result<Value>;
    fn log_tail(&self, lines: usize, path: Option<&str>) -> Result<Value>;
}
