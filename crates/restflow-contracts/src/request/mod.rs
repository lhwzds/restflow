mod defaults;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

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
        #[serde(default)]
        preview: bool,
        #[serde(default)]
        confirmation_token: Option<String>,
    },
    UpdateAgent {
        id: String,
        name: Option<String>,
        agent: Option<AgentNode>,
        #[serde(default)]
        preview: bool,
        #[serde(default)]
        confirmation_token: Option<String>,
    },
    DeleteAgent {
        id: String,
    },

    ListSkills,
    GetSkill {
        id: String,
    },
    CreateSkill {
        skill: Skill,
    },
    UpdateSkill {
        id: String,
        skill: Skill,
    },
    GetSkillReference {
        skill_id: String,
        ref_id: String,
    },
    DeleteSkill {
        id: String,
    },
    ListWorkItems {
        query: ItemQuery,
    },
    ListWorkItemFolders,
    GetWorkItem {
        id: String,
    },
    CreateWorkItem {
        spec: WorkItemSpec,
    },
    UpdateWorkItem {
        id: String,
        patch: WorkItemPatch,
    },
    DeleteWorkItem {
        id: String,
    },

    ListBackgroundAgents {
        status: Option<String>,
    },
    ListRunnableBackgroundAgents {
        current_time: Option<i64>,
    },
    GetBackgroundAgent {
        id: String,
    },
    ListHooks,
    CreateHook {
        hook: Hook,
    },
    UpdateHook {
        id: String,
        hook: Hook,
    },
    DeleteHook {
        id: String,
    },
    TestHook {
        id: String,
    },

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

    SearchMemory {
        query: String,
        agent_id: Option<String>,
        limit: Option<u32>,
    },
    SearchMemoryRanked {
        query: MemorySearchQuery,
        min_score: Option<f64>,
        scoring_preset: Option<String>,
    },
    GetMemoryChunk {
        id: String,
    },
    ListMemory {
        agent_id: Option<String>,
        tag: Option<String>,
    },
    ListMemoryBySession {
        session_id: String,
    },
    AddMemory {
        content: String,
        agent_id: Option<String>,
        tags: Vec<String>,
    },
    CreateMemoryChunk {
        chunk: MemoryChunk,
    },
    DeleteMemory {
        id: String,
    },
    ClearMemory {
        agent_id: Option<String>,
    },
    GetMemoryStats {
        agent_id: Option<String>,
    },
    ExportMemory {
        agent_id: Option<String>,
    },
    ExportMemorySession {
        session_id: String,
    },
    ExportMemoryAdvanced {
        agent_id: String,
        session_id: Option<String>,
        preset: Option<String>,
        include_metadata: Option<bool>,
        include_timestamps: Option<bool>,
        include_source: Option<bool>,
        include_tags: Option<bool>,
    },
    GetMemorySession {
        session_id: String,
    },
    ListMemorySessions {
        agent_id: String,
    },
    CreateMemorySession {
        session: MemorySession,
    },
    DeleteMemorySession {
        session_id: String,
        delete_chunks: bool,
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
    RebuildExternalSession {
        id: String,
    },
    SearchSessions {
        query: String,
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
    },
    ExecuteChatSessionStream {
        session_id: String,
        user_input: Option<String>,
        stream_id: String,
    },
    SteerChatSessionStream {
        session_id: String,
        instruction: String,
    },
    CancelChatSessionStream {
        stream_id: String,
    },
    GetSessionMessages {
        session_id: String,
        limit: Option<usize>,
    },
    ListExecutionContainers,
    ListExecutionSessions {
        query: ExecutionSessionListQuery,
    },
    GetExecutionRunThread {
        run_id: String,
    },
    GetExecutionThread {
        query: ExecutionThreadQuery,
    },
    ListChildExecutionSessions {
        query: ChildExecutionSessionQuery,
    },
    QueryExecutionTraces {
        #[serde(default)]
        query: ExecutionTraceQuery,
    },
    GetExecutionTimeline {
        #[serde(default)]
        query: ExecutionTraceQuery,
    },
    GetExecutionMetrics {
        #[serde(default)]
        query: ExecutionMetricQuery,
    },
    GetProviderHealth {
        #[serde(default)]
        query: ProviderHealthQuery,
    },
    QueryExecutionLogs {
        #[serde(default)]
        query: ExecutionLogQuery,
    },
    GetExecutionTraceStats {
        task_id: Option<String>,
    },
    GetExecutionTraceById {
        #[serde(alias = "event_id")]
        id: String,
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
    DiscoverAuth,
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

    GetBackgroundAgentHistory {
        id: String,
    },
    CreateBackgroundAgent {
        spec: BackgroundAgentSpec,
        #[serde(default)]
        preview: bool,
        #[serde(default)]
        confirmation_token: Option<String>,
    },
    UpdateBackgroundAgent {
        id: String,
        patch: BackgroundAgentPatch,
        #[serde(default)]
        preview: bool,
        #[serde(default)]
        confirmation_token: Option<String>,
    },
    DeleteBackgroundAgent {
        id: String,
    },
    ControlBackgroundAgent {
        id: String,
        action: String,
        #[serde(default)]
        preview: bool,
        #[serde(default)]
        confirmation_token: Option<String>,
    },
    GetBackgroundAgentProgress {
        id: String,
        event_limit: Option<usize>,
    },
    SendBackgroundAgentMessage {
        id: String,
        message: String,
        source: Option<String>,
    },
    HandleBackgroundAgentApproval {
        id: String,
        approved: bool,
    },
    ListBackgroundAgentMessages {
        id: String,
        limit: Option<usize>,
    },
    SubscribeBackgroundAgentEvents {
        background_agent_id: String,
    },
    SubscribeSessionEvents,

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
    pub model: Option<String>,
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
pub struct InlineSubagentConfig {
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
pub struct SubagentSpawnRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline: Option<InlineSubagentConfig>,
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
    pub parent_execution_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_scope_id: Option<String>,
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
    Cli(CliExecutionConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DurabilityMode {
    Sync,
    #[default]
    Async,
    Exit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CliExecutionConfig {
    pub binary: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default = "defaults::default_cli_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub use_pty: bool,
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
pub struct NotificationConfig {
    #[serde(default)]
    pub notify_on_failure_only: bool,
    #[serde(default = "defaults::default_true")]
    pub include_output: bool,
    #[serde(default)]
    pub broadcast_steps: bool,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            notify_on_failure_only: false,
            include_output: true,
            broadcast_steps: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    #[default]
    SharedAgent,
    #[serde(rename = "per_background_agent")]
    PerBackgroundAgent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryConfig {
    #[serde(default = "defaults::default_memory_max_messages")]
    pub max_messages: usize,
    #[serde(default = "defaults::default_true")]
    pub enable_file_memory: bool,
    #[serde(default)]
    pub persist_on_complete: bool,
    #[serde(default = "defaults::default_memory_scope")]
    pub memory_scope: MemoryScope,
    #[serde(default = "defaults::default_memory_compaction_enabled")]
    pub enable_compaction: bool,
    #[serde(default = "defaults::default_memory_compaction_threshold_ratio")]
    pub compaction_threshold_ratio: f32,
    #[serde(default = "defaults::default_memory_max_summary_tokens")]
    pub max_summary_tokens: usize,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_messages: defaults::default_memory_max_messages(),
            enable_file_memory: true,
            persist_on_complete: false,
            memory_scope: defaults::default_memory_scope(),
            enable_compaction: defaults::default_memory_compaction_enabled(),
            compaction_threshold_ratio: defaults::default_memory_compaction_threshold_ratio(),
            max_summary_tokens: defaults::default_memory_max_summary_tokens(),
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
pub struct BackgroundAgentSpec {
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
    pub notification: Option<NotificationConfig>,
    #[serde(default)]
    pub execution_mode: Option<ExecutionMode>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub memory: Option<MemoryConfig>,
    #[serde(default)]
    pub durability_mode: Option<DurabilityMode>,
    #[serde(default)]
    pub resource_limits: Option<ResourceLimits>,
    #[serde(default)]
    pub prerequisites: Vec<String>,
    #[serde(default)]
    pub continuation: Option<ContinuationConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BackgroundAgentPatch {
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
    pub notification: Option<NotificationConfig>,
    #[serde(default)]
    pub execution_mode: Option<ExecutionMode>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub memory: Option<MemoryConfig>,
    #[serde(default)]
    pub durability_mode: Option<DurabilityMode>,
    #[serde(default)]
    pub resource_limits: Option<ResourceLimits>,
    #[serde(default)]
    pub prerequisites: Option<Vec<String>>,
    #[serde(default)]
    pub continuation: Option<ContinuationConfig>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HookEvent {
    #[serde(rename = "task_started")]
    TaskStarted,
    #[serde(rename = "task_completed")]
    TaskCompleted,
    #[serde(rename = "task_failed")]
    TaskFailed,
    #[serde(rename = "task_interrupted")]
    TaskInterrupted,
    #[serde(rename = "tool_executed")]
    ToolExecuted,
    #[serde(rename = "approval_required")]
    ApprovalRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HookAction {
    Webhook {
        url: String,
        #[serde(default)]
        method: Option<String>,
        #[serde(default)]
        headers: Option<BTreeMap<String, String>>,
    },
    Script {
        path: String,
        #[serde(default)]
        args: Option<Vec<String>>,
        #[serde(default)]
        timeout_secs: Option<u64>,
    },
    SendMessage {
        channel_type: String,
        message_template: String,
    },
    RunTask {
        agent_id: String,
        input_template: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HookFilter {
    #[serde(default)]
    pub task_name_pattern: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub success_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Hook {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub event: HookEvent,
    pub action: HookAction,
    #[serde(default)]
    pub filter: Option<HookFilter>,
    #[serde(default = "defaults::default_true")]
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MemorySource {
    TaskExecution {
        task_id: String,
    },
    Conversation {
        session_id: String,
    },
    #[default]
    ManualNote,
    AgentGenerated {
        tool_name: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryChunk {
    pub id: String,
    pub agent_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    pub content: String,
    pub content_hash: String,
    #[serde(default)]
    pub source: MemorySource,
    pub created_at: i64,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub token_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_dim: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemorySession {
    pub id: String,
    pub agent_id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub chunk_count: u32,
    #[serde(default)]
    pub total_tokens: u32,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    #[default]
    Keyword,
    Phrase,
    Regex,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SourceTypeFilter {
    TaskExecution,
    Conversation,
    ManualNote,
    AgentGenerated,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemorySearchQuery {
    pub agent_id: String,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub search_mode: SearchMode,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub source_type: Option<SourceTypeFilter>,
    #[serde(default)]
    pub from_time: Option<i64>,
    #[serde(default)]
    pub to_time: Option<i64>,
    #[serde(default = "defaults::default_memory_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
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
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionTraceCategory {
    LlmCall,
    ToolCall,
    ModelSwitch,
    Lifecycle,
    Message,
    MetricSample,
    ProviderHealth,
    LogRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionTraceSource {
    AgentExecutor,
    Runtime,
    McpServer,
    Cli,
    Telemetry,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ExecutionTraceQuery {
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub parent_run_id: Option<String>,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub agent_id: Option<String>,
    pub category: Option<ExecutionTraceCategory>,
    pub source: Option<ExecutionTraceSource>,
    pub from_timestamp: Option<i64>,
    pub to_timestamp: Option<i64>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionContainerKind {
    Workspace,
    BackgroundTask,
    ExternalChannel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionContainerRef {
    pub kind: ExecutionContainerKind,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionSessionListQuery {
    pub container: ExecutionContainerRef,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChildExecutionSessionQuery {
    pub parent_run_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionThreadQuery {
    pub session_id: Option<String>,
    pub run_id: Option<String>,
    pub task_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ExecutionMetricQuery {
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub metric_name: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ProviderHealthQuery {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ExecutionLogQuery {
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub level: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    #[default]
    Open,
    InProgress,
    Done,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkItemSpec {
    pub folder: String,
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WorkItemPatch {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub status: Option<ItemStatus>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub folder: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ItemQuery {
    #[serde(default)]
    pub folder: Option<String>,
    #[serde(default)]
    pub status: Option<ItemStatus>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub search: Option<String>,
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
    pub max_iterations: usize,
    pub max_depth: usize,
    pub subagent_timeout_secs: u64,
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
    pub memory_search_limit: u32,
    pub session_list_limit: u32,
    pub background_progress_event_limit: usize,
    pub background_message_list_limit: usize,
    pub background_trace_list_limit: usize,
    pub background_trace_line_limit: usize,
    pub web_search_num_results: usize,
    pub diagnostics_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RuntimeSettings {
    pub background_runner_poll_interval_ms: u64,
    pub background_runner_max_concurrent_tasks: usize,
    pub chat_max_session_history: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ChannelSettings {
    pub telegram_api_timeout_secs: u64,
    pub telegram_polling_timeout_secs: u32,
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
    pub background_api_timeout_seconds: Option<u64>,
    #[serde(default)]
    pub chat_response_timeout_seconds: Option<u64>,
    pub max_retries: u32,
    pub chat_session_retention_days: u32,
    pub background_task_retention_days: u32,
    pub checkpoint_retention_days: u32,
    pub memory_chunk_retention_days: u32,
    pub log_file_retention_days: u32,
    pub experimental_features: Vec<String>,
    #[serde(default)]
    pub agent: AgentSettings,
    #[serde(default)]
    pub api_defaults: ApiSettings,
    #[serde(default)]
    pub runtime_defaults: RuntimeSettings,
    #[serde(default)]
    pub channel_defaults: ChannelSettings,
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
            model: Some("gpt-5".to_string()),
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
    fn subagent_spawn_request_round_trips() {
        let request = SubagentSpawnRequest {
            agent_id: Some("coder".to_string()),
            inline: Some(InlineSubagentConfig {
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
            parent_execution_id: Some("exec-1".to_string()),
            trace_session_id: Some("session-1".to_string()),
            trace_scope_id: Some("scope-1".to_string()),
        };

        assert_roundtrip(&request);
    }

    #[test]
    fn ipc_request_background_agent_round_trips() {
        let request = IpcRequest::CreateBackgroundAgent {
            spec: BackgroundAgentSpec {
                name: "nightly".to_string(),
                agent_id: "agent-1".to_string(),
                chat_session_id: Some("session-1".to_string()),
                description: Some("run nightly".to_string()),
                input: Some("execute".to_string()),
                input_template: Some("{{input}}".to_string()),
                schedule: TaskSchedule::Cron {
                    expression: "0 9 * * *".to_string(),
                    timezone: Some("America/Los_Angeles".to_string()),
                },
                notification: Some(NotificationConfig::default()),
                execution_mode: Some(ExecutionMode::Api),
                timeout_secs: Some(300),
                memory: Some(MemoryConfig::default()),
                durability_mode: Some(DurabilityMode::Async),
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
            preview: false,
            confirmation_token: None,
        };
        assert_roundtrip(&request);
    }

    #[test]
    fn background_agent_contract_defaults_match_expected_semantics() {
        let contract: BackgroundAgentSpec = serde_json::from_value(serde_json::json!({
            "name": "nightly",
            "agent_id": "agent-1",
            "schedule": {
                "type": "interval",
                "interval_ms": 60000,
                "start_at": null
            },
            "execution_mode": {
                "type": "cli",
                "binary": "claude"
            },
            "memory": {},
            "resource_limits": {},
            "continuation": {}
        }))
        .expect("background agent defaults");

        let cli = match contract.execution_mode.expect("execution mode") {
            ExecutionMode::Cli(config) => config,
            ExecutionMode::Api => panic!("expected cli config"),
        };
        assert_eq!(cli.timeout_secs, defaults::default_cli_timeout_secs());

        let memory = contract.memory.expect("memory config");
        assert_eq!(memory.max_messages, defaults::default_memory_max_messages());
        assert!(memory.enable_file_memory);
        assert_eq!(memory.memory_scope, MemoryScope::SharedAgent);
        assert!(memory.enable_compaction);
        assert_eq!(
            memory.compaction_threshold_ratio,
            defaults::default_memory_compaction_threshold_ratio()
        );
        assert_eq!(
            memory.max_summary_tokens,
            defaults::default_memory_max_summary_tokens()
        );

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
    fn ipc_request_memory_round_trips() {
        let request = IpcRequest::SearchMemoryRanked {
            query: MemorySearchQuery {
                agent_id: "agent-1".to_string(),
                query: Some("rust".to_string()),
                search_mode: SearchMode::Phrase,
                session_id: Some("session-1".to_string()),
                tags: vec!["lang".to_string()],
                source_type: Some(SourceTypeFilter::Conversation),
                from_time: Some(1),
                to_time: Some(2),
                limit: 10,
                offset: 5,
            },
            min_score: Some(0.8),
            scoring_preset: Some("balanced".to_string()),
        };
        assert_roundtrip(&request);
    }

    #[test]
    fn ipc_request_work_item_round_trips() {
        let request = IpcRequest::UpdateWorkItem {
            id: "item-1".to_string(),
            patch: WorkItemPatch {
                title: Some("updated".to_string()),
                content: Some("body".to_string()),
                priority: Some("p1".to_string()),
                status: Some(ItemStatus::InProgress),
                tags: Some(vec!["tag".to_string()]),
                assignee: Some("agent".to_string()),
                folder: Some("inbox".to_string()),
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
