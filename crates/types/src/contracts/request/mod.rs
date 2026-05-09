mod defaults;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use super::ExecutionScope;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "data")]
pub enum IpcRequest {
    Ping,
    GetStatus,
    Shutdown,

    ListAgents,
    GetAgent {
        id: String,
    },
    CreateAgent {
        name: String,
        agent: AgentNode,
    },
    UpdateAgent {
        id: String,
        name: Option<String>,
        agent: Option<AgentNode>,
    },
    DeleteAgent {
        id: String,
    },

    ListSkills,
    GetSkill {
        id: String,
    },
    GetSkillReference {
        skill_id: String,
        ref_id: String,
    },

    ListTasks {
        status: Option<String>,
    },
    ListRunnableTasks {
        current_time: Option<i64>,
    },
    GetTask {
        id: String,
    },
    RunCleanup,

    ListSecrets,
    GetSecret {
        key: String,
    },
    SetSecret {
        key: String,
        value: String,
        description: Option<String>,
    },
    CreateSecret {
        key: String,
        value: String,
        description: Option<String>,
    },
    UpdateSecret {
        key: String,
        value: String,
        description: Option<String>,
    },
    DeleteSecret {
        key: String,
    },

    GetConfig,
    GetGlobalConfig,
    SetConfig {
        config: SystemConfig,
    },

    ListSessions,
    ListFullSessions,
    ListSessionsByAgent {
        agent_id: String,
    },
    ListSessionsBySkill {
        skill_id: String,
    },
    CountSessions,
    DeleteSessionsOlderThan {
        older_than_ms: i64,
    },
    GetSession {
        id: String,
    },
    CreateSession {
        agent_id: Option<String>,
        model: Option<String>,
        name: Option<String>,
        skill_id: Option<String>,
    },
    UpdateSession {
        id: String,
        updates: ChatSessionUpdate,
    },
    RenameSession {
        id: String,
        name: String,
    },
    ArchiveSession {
        id: String,
    },
    DeleteSession {
        id: String,
    },
    SearchSessions {
        query: String,
        agent_id: Option<String>,
        limit: Option<usize>,
    },
    AddMessage {
        session_id: String,
        role: String,
        content: String,
    },
    AppendMessage {
        session_id: String,
        message: ChatMessage,
    },
    ExecuteChatSession {
        session_id: String,
        user_input: Option<String>,
        workspace_root: Option<String>,
    },
    ExecuteChatSessionStream {
        session_id: String,
        user_input: Option<String>,
        stream_id: String,
        workspace_root: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<ExecutionScope>,
    },
    SteerChatSessionStream {
        session_id: String,
        instruction: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<ExecutionScope>,
    },
    CancelChatSessionStream {
        stream_id: String,
    },
    GetSessionMessages {
        session_id: String,
        limit: Option<usize>,
    },
    ListExecutionContainers,
    ListRuns {
        query: RunListQuery,
    },
    GetExecutionRunThread {
        run_id: String,
    },
    ListChildRuns {
        query: ChildRunListQuery,
    },
    GetExecutionRunTimeline {
        run_id: String,
    },
    ListTerminalSessions,
    GetTerminalSession {
        id: String,
    },
    CreateTerminalSession,
    RenameTerminalSession {
        id: String,
        name: String,
    },
    UpdateTerminalSession {
        id: String,
        name: Option<String>,
        working_directory: Option<String>,
        startup_command: Option<String>,
    },
    SaveTerminalSession {
        session: TerminalSession,
    },
    DeleteTerminalSession {
        id: String,
    },
    MarkAllTerminalSessionsStopped,

    ListAuthProfiles,
    GetAuthProfile {
        id: String,
    },
    AddAuthProfile {
        name: String,
        credential: Credential,
        source: String,
        provider: String,
    },
    RemoveAuthProfile {
        id: String,
    },
    UpdateAuthProfile {
        id: String,
        updates: ProfileUpdate,
    },
    EnableAuthProfile {
        id: String,
    },
    DisableAuthProfile {
        id: String,
        reason: String,
    },
    GetApiKey {
        provider: String,
    },
    GetApiKeyForProfile {
        id: String,
    },
    TestAuthProfile {
        id: String,
    },
    MarkAuthSuccess {
        id: String,
    },
    MarkAuthFailure {
        id: String,
    },
    ClearAuthProfiles,

    GetTaskHistory {
        id: String,
    },
    CreateTask {
        spec: TaskSpec,
    },
    CreateTaskFromSession {
        request: TaskFromSessionRequest,
    },
    UpdateTask {
        id: String,
        patch: TaskPatch,
    },
    DeleteTask {
        id: String,
        approval_id: Option<String>,
    },
    ControlTask {
        id: String,
        action: String,
        approval_id: Option<String>,
    },
    GetTaskProgress {
        id: String,
        event_limit: Option<usize>,
    },
    SendTaskMessage {
        id: String,
        message: String,
        source: Option<String>,
    },
    HandleTaskApproval {
        id: String,
        approved: bool,
    },
    ListTaskMessages {
        id: String,
        limit: Option<usize>,
    },
    SubscribeTaskEvents {
        task_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<ExecutionScope>,
    },
    SubscribeSessionEvents,
    ListRunArtifacts {
        #[serde(default)]
        run_id: Option<String>,
        #[serde(default)]
        task_id: Option<String>,
    },
    SwitchSessionModel {
        session_id: String,
        model_ref: WireModelRef,
        #[serde(default)]
        reason: Option<String>,
    },

    GetSystemInfo,
    GetAvailableModels,
    GetAvailableTools,
    GetAvailableToolDefinitions,
    ExecuteTool {
        name: String,
        input: Value,
    },
    ListMcpServers,

    BuildAgentSystemPrompt {
        agent_node: AgentNode,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CodexCliExecutionMode {
    Safe,
    Bypass,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SkillPreflightPolicyMode {
    Off,
    #[default]
    Warn,
    Enforce,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelRoutingConfig {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routine_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moderate_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub complex_model: Option<String>,
    pub escalate_on_failure: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
pub enum ApiKeyConfig {
    Direct(String),
    Secret(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WireModelRef {
    pub provider: String,
    pub model: String,
}

pub type ModelRef = WireModelRef;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AgentNode {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_ref: Option<WireModelRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_cli_reasoning_effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_cli_execution_mode: Option<CodexCliExecutionMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_config: Option<ApiKeyConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_variables: Option<HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_preflight_policy_mode: Option<SkillPreflightPolicyMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_routing: Option<ModelRoutingConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct InlineAgentRunConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SpawnPriority {
    Low,
    #[default]
    Normal,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RunSpawnRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline: Option<InlineAgentRunConfig>,
    pub task: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<SpawnPriority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Credential {
    ApiKey {
        key: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        email: Option<String>,
    },
    Token {
        token: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        expires_at: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        email: Option<String>,
    },
    OAuth {
        access_token: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        refresh_token: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        expires_at: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        email: Option<String>,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileUpdate {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ExecutionMode {
    #[default]
    Api,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TaskSchedule {
    Once {
        run_at: i64,
    },
    Interval {
        interval_ms: i64,
        start_at: Option<i64>,
    },
    Cron {
        expression: String,
        #[serde(default)]
        timezone: Option<String>,
    },
}

impl Default for TaskSchedule {
    fn default() -> Self {
        Self::Interval {
            interval_ms: 3_600_000,
            start_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceLimits {
    #[serde(default = "defaults::default_max_tool_calls")]
    pub max_tool_calls: usize,
    #[serde(default = "defaults::default_max_duration_secs")]
    pub max_duration_secs: u64,
    #[serde(default = "defaults::default_max_output_bytes")]
    pub max_output_bytes: usize,
    #[serde(default)]
    pub max_cost_usd: Option<f64>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_tool_calls: defaults::default_max_tool_calls(),
            max_duration_secs: defaults::default_max_duration_secs(),
            max_output_bytes: defaults::default_max_output_bytes(),
            max_cost_usd: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContinuationConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "defaults::default_segment_iterations")]
    pub segment_iterations: usize,
    #[serde(default = "defaults::default_max_total_iterations")]
    pub max_total_iterations: usize,
    #[serde(default)]
    pub max_total_cost_usd: Option<f64>,
    #[serde(default = "defaults::default_inter_segment_pause_ms")]
    pub inter_segment_pause_ms: u64,
}

impl Default for ContinuationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            segment_iterations: defaults::default_segment_iterations(),
            max_total_iterations: defaults::default_max_total_iterations(),
            max_total_cost_usd: None,
            inter_segment_pause_ms: defaults::default_inter_segment_pause_ms(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskSpec {
    pub name: String,
    pub agent_id: String,
    #[serde(default)]
    pub chat_session_id: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub input_template: Option<String>,
    pub schedule: TaskSchedule,
    #[serde(default)]
    pub execution_mode: Option<ExecutionMode>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub resource_limits: Option<ResourceLimits>,
    #[serde(default)]
    pub prerequisites: Vec<String>,
    #[serde(default)]
    pub continuation: Option<ContinuationConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TaskPatch {
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
    pub schedule: Option<TaskSchedule>,
    #[serde(default)]
    pub execution_mode: Option<ExecutionMode>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub resource_limits: Option<ResourceLimits>,
    #[serde(default)]
    pub prerequisites: Option<Vec<String>>,
    #[serde(default)]
    pub continuation: Option<ContinuationConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TaskFromSessionRequest {
    pub session_id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub schedule: Option<TaskSchedule>,
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    pub resource_limits: Option<ResourceLimits>,
    #[serde(default)]
    pub run_now: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChatExecutionStatus {
    #[default]
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatMediaType {
    Voice,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatMessageMedia {
    pub media_type: ChatMediaType,
    pub file_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_sec: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatMessageTranscript {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionStepInfo {
    pub step_type: String,
    pub name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageExecution {
    pub steps: Vec<ExecutionStepInfo>,
    pub duration_ms: u64,
    pub tokens_used: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u32>,
    pub status: ChatExecutionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    #[serde(default)]
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution: Option<MessageExecution>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<ChatMessageMedia>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript: Option<ChatMessageTranscript>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ChatSessionUpdate {
    pub agent_id: Option<String>,
    pub model: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum StorageMode {
    #[default]
    DatabaseOnly,
    FileSystemOnly,
    Hybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillScript {
    pub id: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillReference {
    pub id: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillGating {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bins: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SkillStatus {
    #[default]
    Active,
    Completed,
    Archived,
    Draft,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SkillSource {
    System,
    #[default]
    User,
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<String>,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder_path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggested_tools: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<SkillScript>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<SkillReference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gating: Option<SkillGating>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    #[serde(default)]
    pub status: SkillStatus,
    #[serde(default)]
    pub auto_complete: bool,
    #[serde(default)]
    pub storage_mode: StorageMode,
    #[serde(default)]
    pub is_synced: bool,
    #[serde(default)]
    pub source: SkillSource,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionContainerKind {
    Workspace,
    Task,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionContainerRef {
    pub kind: ExecutionContainerKind,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunListQuery {
    pub container: ExecutionContainerRef,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChildRunListQuery {
    pub parent_run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum TerminalStatus {
    Running,
    #[default]
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TerminalSession {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    #[serde(default)]
    pub status: TerminalStatus,
    #[serde(default)]
    pub history: Option<String>,
    #[serde(default)]
    pub stopped_at: Option<i64>,
    #[serde(default)]
    pub working_directory: Option<String>,
    #[serde(default)]
    pub startup_command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AgentSettings {
    pub tool_timeout_secs: u64,
    pub llm_timeout_secs: Option<u64>,
    pub bash_timeout_secs: u64,
    pub python_timeout_secs: u64,
    pub browser_timeout_secs: u64,
    pub process_session_ttl_secs: u64,
    pub approval_timeout_secs: u64,
    #[serde(default)]
    pub auto_review_tools: bool,
    pub max_iterations: usize,
    pub max_depth: usize,
    #[serde(alias = "child_run_timeout_secs")]
    pub subagent_timeout_secs: u64,
    #[serde(alias = "max_parallel_child_runs")]
    pub max_parallel_subagents: usize,
    pub max_tool_calls: usize,
    pub max_tool_concurrency: usize,
    pub max_tool_result_length: usize,
    pub prune_tool_max_chars: usize,
    pub compact_preserve_tokens: usize,
    pub max_wall_clock_secs: Option<u64>,
    pub default_task_timeout_secs: u64,
    pub default_max_duration_secs: u64,
    #[serde(default)]
    pub fallback_models: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ApiSettings {
    pub session_list_limit: u32,
    pub task_progress_event_limit: usize,
    pub task_message_list_limit: usize,
    pub web_search_num_results: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RuntimeSettings {
    pub task_runner_poll_interval_ms: u64,
    pub task_runner_max_concurrent_tasks: usize,
    pub chat_max_session_history: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RegistrySettings {
    pub github_cache_ttl_secs: u64,
    pub marketplace_cache_ttl_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SystemConfig {
    pub worker_count: usize,
    pub task_timeout_seconds: u64,
    pub stall_timeout_seconds: u64,
    #[serde(default)]
    pub task_api_timeout_seconds: Option<u64>,
    #[serde(default)]
    pub chat_response_timeout_seconds: Option<u64>,
    pub max_retries: u32,
    pub chat_session_retention_days: u32,
    pub task_retention_days: u32,
    pub log_file_retention_days: u32,
    pub experimental_features: Vec<String>,
    #[serde(default)]
    pub agent: AgentSettings,
    #[serde(default)]
    pub api_defaults: ApiSettings,
    #[serde(default)]
    pub runtime_defaults: RuntimeSettings,
    #[serde(default)]
    pub registry_defaults: RegistrySettings,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_roundtrip<T>(value: &T)
    where
        T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_string(value).unwrap();
        let decoded: T = serde_json::from_str(&json).unwrap();
        assert_eq!(&decoded, value);
    }

    fn sample_agent_node() -> AgentNode {
        AgentNode {
            model_ref: Some(WireModelRef {
                provider: "openai".to_string(),
                model: "gpt-5".to_string(),
            }),
            prompt: Some("You are helpful.".to_string()),
            temperature: Some(0.2),
            codex_cli_reasoning_effort: Some("high".to_string()),
            codex_cli_execution_mode: Some(CodexCliExecutionMode::Safe),
            api_key_config: Some(ApiKeyConfig::Secret("OPENAI_API_KEY".to_string())),
            tools: Some(vec!["bash".to_string()]),
            skills: Some(vec!["skill-1".to_string()]),
            skill_variables: Some(HashMap::from([(
                "topic".to_string(),
                "contracts".to_string(),
            )])),
            skill_preflight_policy_mode: Some(SkillPreflightPolicyMode::Warn),
            model_routing: Some(ModelRoutingConfig {
                enabled: true,
                routine_model: Some("gpt-5-mini".to_string()),
                moderate_model: Some("gpt-5".to_string()),
                complex_model: Some("gpt-5-pro".to_string()),
                escalate_on_failure: true,
            }),
        }
    }

    #[test]
    fn wire_model_ref_alias_round_trips() {
        let model_ref = WireModelRef {
            provider: "openai".to_string(),
            model: "gpt-5".to_string(),
        };
        assert_roundtrip(&model_ref);

        let legacy_alias: ModelRef = model_ref.clone();
        assert_eq!(legacy_alias, model_ref);
    }

    #[test]
    fn run_spawn_request_round_trips() {
        let request = RunSpawnRequest {
            agent_id: Some("coder".to_string()),
            inline: Some(InlineAgentRunConfig {
                name: Some("Temp".to_string()),
                system_prompt: Some("You are focused.".to_string()),
                allowed_tools: Some(vec!["bash".to_string()]),
                max_iterations: Some(3),
            }),
            task: "Write code".to_string(),
            timeout_secs: Some(30),
            max_iterations: Some(5),
            priority: Some(SpawnPriority::High),
            model: Some("gpt-5.4-codex".to_string()),
            model_provider: Some("openai-codex".to_string()),
            parent_run_id: Some("run-1".to_string()),
        };

        assert_roundtrip(&request);
    }

    #[test]
    fn ipc_request_task_round_trips() {
        let request = IpcRequest::CreateTask {
            spec: TaskSpec {
                name: "nightly".to_string(),
                agent_id: "agent-1".to_string(),
                chat_session_id: Some("session-1".to_string()),
                description: Some("run nightly".to_string()),
                input: Some("execute".to_string()),
                input_template: Some("{{task.input}}".to_string()),
                schedule: TaskSchedule::Cron {
                    expression: "0 9 * * *".to_string(),
                    timezone: Some("America/Los_Angeles".to_string()),
                },
                execution_mode: Some(ExecutionMode::Api),
                timeout_secs: Some(300),
                resource_limits: Some(ResourceLimits {
                    max_tool_calls: 10,
                    max_duration_secs: 60,
                    max_output_bytes: 1024,
                    max_cost_usd: Some(1.5),
                }),
                prerequisites: vec!["task-1".to_string()],
                continuation: Some(ContinuationConfig {
                    enabled: true,
                    segment_iterations: 10,
                    max_total_iterations: 100,
                    max_total_cost_usd: Some(5.0),
                    inter_segment_pause_ms: 500,
                }),
            },
        };
        assert_roundtrip(&request);
    }

    #[test]
    fn ipc_request_create_task_from_session_round_trips() {
        let request = IpcRequest::CreateTaskFromSession {
            request: TaskFromSessionRequest {
                session_id: "session-1".to_string(),
                name: Some("Converted Session".to_string()),
                schedule: Some(TaskSchedule::Cron {
                    expression: "0 9 * * *".to_string(),
                    timezone: Some("America/Los_Angeles".to_string()),
                }),
                input: Some("execute".to_string()),
                timeout_secs: Some(300),
                resource_limits: Some(ResourceLimits {
                    max_tool_calls: 10,
                    max_duration_secs: 60,
                    max_output_bytes: 1024,
                    max_cost_usd: Some(1.5),
                }),
                run_now: Some(true),
            },
        };
        assert_roundtrip(&request);
    }

    #[test]
    fn task_from_session_contract_defaults_match_expected_semantics() {
        let contract: TaskFromSessionRequest = serde_json::from_value(serde_json::json!({
            "session_id": "session-1"
        }))
        .expect("convert defaults");

        assert_eq!(contract.run_now, None);
    }

    #[test]
    fn task_contract_defaults_match_expected_semantics() {
        let contract: TaskSpec = serde_json::from_value(serde_json::json!({
            "name": "nightly",
            "agent_id": "agent-1",
            "schedule": {
                "type": "interval",
                "interval_ms": 60000,
                "start_at": null
            },
            "execution_mode": {
                "type": "api"
            },
            "memory": {},
            "resource_limits": {},
            "continuation": {}
        }))
        .expect("task defaults");

        assert_eq!(contract.execution_mode, Some(ExecutionMode::Api));

        let limits = contract.resource_limits.expect("resource limits");
        assert_eq!(limits.max_tool_calls, defaults::default_max_tool_calls());
        assert_eq!(
            limits.max_duration_secs,
            defaults::default_max_duration_secs()
        );
        assert_eq!(
            limits.max_output_bytes,
            defaults::default_max_output_bytes()
        );

        let continuation = contract.continuation.expect("continuation");
        assert_eq!(
            continuation.segment_iterations,
            defaults::default_segment_iterations()
        );
        assert_eq!(
            continuation.max_total_iterations,
            defaults::default_max_total_iterations()
        );
        assert_eq!(
            continuation.inter_segment_pause_ms,
            defaults::default_inter_segment_pause_ms()
        );
    }

    #[test]
    fn list_runs_and_child_runs_round_trip() {
        let request = IpcRequest::ListRuns {
            query: RunListQuery {
                container: ExecutionContainerRef {
                    kind: ExecutionContainerKind::Task,
                    id: "task-1".to_string(),
                },
            },
        };
        assert_roundtrip(&request);

        let child_request = IpcRequest::ListChildRuns {
            query: ChildRunListQuery {
                parent_run_id: "run-root".to_string(),
            },
        };
        assert_roundtrip(&child_request);
    }

    #[test]
    fn ipc_request_session_round_trips() {
        let request = IpcRequest::AppendMessage {
            session_id: "session-1".to_string(),
            message: ChatMessage {
                id: "msg-1".to_string(),
                role: "user".to_string(),
                content: "hello".to_string(),
                timestamp: 1,
                execution: Some(MessageExecution {
                    steps: vec![ExecutionStepInfo {
                        step_type: "tool_call".to_string(),
                        name: "bash".to_string(),
                        status: "completed".to_string(),
                        duration_ms: Some(12),
                    }],
                    duration_ms: 12,
                    tokens_used: 20,
                    cost_usd: Some(0.01),
                    input_tokens: Some(10),
                    output_tokens: Some(10),
                    status: ChatExecutionStatus::Completed,
                }),
                media: Some(ChatMessageMedia {
                    media_type: ChatMediaType::Voice,
                    file_path: "/tmp/audio.wav".to_string(),
                    duration_sec: Some(3),
                }),
                transcript: Some(ChatMessageTranscript {
                    text: "hello".to_string(),
                    model: Some("whisper-1".to_string()),
                    updated_at: Some(1),
                }),
            },
        };
        assert_roundtrip(&request);
    }

    #[test]
    fn ipc_request_auth_round_trips() {
        let request = IpcRequest::UpdateAuthProfile {
            id: "profile-1".to_string(),
            updates: ProfileUpdate {
                name: Some("Main".to_string()),
                enabled: Some(true),
                priority: Some(1),
            },
        };
        assert_roundtrip(&request);
    }

    #[test]
    fn ipc_request_terminal_round_trips() {
        let request = IpcRequest::SaveTerminalSession {
            session: TerminalSession {
                id: "terminal-1".to_string(),
                name: "Main".to_string(),
                created_at: 1,
                status: TerminalStatus::Running,
                history: Some("ls".to_string()),
                stopped_at: None,
                working_directory: Some("/tmp".to_string()),
                startup_command: Some("pwd".to_string()),
            },
        };
        assert_roundtrip(&request);
    }

    #[test]
    fn ipc_request_agent_round_trips() {
        let request = IpcRequest::BuildAgentSystemPrompt {
            agent_node: sample_agent_node(),
        };
        assert_roundtrip(&request);
    }
}
