use super::*;

/// Parameters for get_skill tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetSkillParams {
    /// The ID of the skill to retrieve
    pub id: String,
}

/// Parameters for get_skill_reference tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetSkillReferenceParams {
    /// The skill ID that owns the reference
    pub skill_id: String,
    /// The reference ID to load
    pub ref_id: String,
}

/// Parameters for create_skill tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreateSkillParams {
    /// Display name of the skill
    pub name: String,
    /// Optional description of what the skill does
    #[serde(default)]
    pub description: Option<String>,
    /// Optional tags for categorization
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// The markdown content of the skill (instructions for the AI)
    pub content: String,
}

/// Parameters for update_skill tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct UpdateSkillParams {
    /// The ID of the skill to update
    pub id: String,
    /// New display name (optional)
    #[serde(default)]
    pub name: Option<String>,
    /// New description (optional)
    #[serde(default)]
    pub description: Option<String>,
    /// New tags (optional)
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// New content (optional)
    #[serde(default)]
    pub content: Option<String>,
}

/// Parameters for delete_skill tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DeleteSkillParams {
    /// The ID of the skill to delete
    pub id: String,
}

/// Parameters for get_agent tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetAgentParams {
    /// The ID of the agent to retrieve
    pub id: String,
}

/// Parameters for memory_search tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MemorySearchParams {
    /// Search query string
    pub query: String,
    /// Agent ID to scope the search
    pub agent_id: String,
    /// Maximum number of results to return.
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Parameters for memory_store tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MemoryStoreParams {
    /// Agent ID to store memory under
    pub agent_id: String,
    /// Memory content to store
    pub content: String,
    /// Optional tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Parameters for memory_stats tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MemoryStatsParams {
    /// Agent ID to fetch stats for
    pub agent_id: String,
}

/// Parameters for get_skill_context tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetSkillContextParams {
    /// Skill ID to get context for
    pub skill_id: String,
    /// Optional input provided to the skill
    #[serde(default)]
    pub input: Option<String>,
}

/// Parameters for list_skills tool
#[derive(Debug, Serialize, Deserialize, JsonSchema, Default)]
pub struct ListSkillsParams {
    /// Optional status filter: active/completed/archived/draft
    #[serde(default)]
    pub status: Option<String>,
}

/// Parameters for chat_session_list tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChatSessionListParams {
    /// Optional agent ID filter
    #[serde(default)]
    pub agent_id: Option<String>,
    /// Maximum number of sessions to return.
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Parameters for chat_session_get tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChatSessionGetParams {
    /// Session ID to retrieve
    pub session_id: String,
}

/// Parameters for manage_background_agents tool
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ManageBackgroundAgentsParams {
    /// Operation to perform
    pub operation: String,
    /// Source chat session ID (for convert_session/promote_to_background)
    #[serde(default)]
    pub session_id: Option<String>,
    /// Task/background agent ID
    #[serde(default)]
    pub id: Option<String>,
    /// Task name
    #[serde(default)]
    pub name: Option<String>,
    /// Agent ID for execution
    #[serde(default)]
    pub agent_id: Option<String>,
    /// Optional task ID selector for trace queries (list_traces)
    #[serde(default)]
    pub task_id: Option<String>,
    /// Optional bound chat session ID
    #[serde(default)]
    pub chat_session_id: Option<String>,
    /// Optional description
    #[serde(default)]
    pub description: Option<String>,
    /// Optional task input
    #[serde(default)]
    pub input: Option<String>,
    /// Optional per-instance inputs for run_batch
    #[serde(default)]
    pub inputs: Option<Vec<String>>,
    /// Optional batch/team name for run_batch/save_team/get_team/delete_team
    #[serde(default)]
    pub team: Option<String>,
    /// Optional worker specs payload for run_batch/save_team
    #[serde(default)]
    pub workers: Option<Value>,
    /// Optional team name to persist during run_batch
    #[serde(default)]
    pub save_as_team: Option<String>,
    /// Optional task input template
    #[serde(default)]
    pub input_template: Option<String>,
    /// Optional schedule payload
    #[serde(default)]
    pub schedule: Option<Value>,
    /// Optional notification payload
    #[serde(default)]
    pub notification: Option<Value>,
    /// Optional execution mode payload
    #[serde(default)]
    pub execution_mode: Option<Value>,
    /// Optional per-task timeout (seconds) for API execution mode
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    /// Optional checkpoint durability mode
    #[serde(default)]
    pub durability_mode: Option<String>,
    /// Optional memory payload
    #[serde(default)]
    pub memory: Option<Value>,
    /// Optional memory scope override
    #[serde(default)]
    pub memory_scope: Option<String>,
    /// Optional resource limits payload
    #[serde(default)]
    pub resource_limits: Option<Value>,
    /// Optional prerequisite task IDs
    #[serde(default)]
    pub prerequisites: Option<Vec<String>>,
    /// Optional list status filter
    #[serde(default)]
    pub status: Option<String>,
    /// Optional control action
    #[serde(default)]
    pub action: Option<String>,
    /// Optional progress event limit
    #[serde(default)]
    pub event_limit: Option<usize>,
    /// Optional message body
    #[serde(default)]
    pub message: Option<String>,
    /// Optional message source for send_message, or trace source filter for list_traces
    #[serde(default)]
    pub source: Option<String>,
    /// Optional message list limit
    #[serde(default)]
    pub limit: Option<usize>,
    /// Optional pagination offset (for list_traces)
    #[serde(default)]
    pub offset: Option<usize>,
    /// Optional trace category filter (turn/tool/event type) for list_traces
    #[serde(default)]
    pub category: Option<String>,
    /// Optional lower bound for trace event timestamp (Unix milliseconds)
    #[serde(default)]
    pub from_time_ms: Option<i64>,
    /// Optional upper bound for trace event timestamp (Unix milliseconds)
    #[serde(default)]
    pub to_time_ms: Option<i64>,
    /// Whether to include trace stats payload in list_traces response
    #[serde(default)]
    pub include_stats: Option<bool>,
    /// Trace ID for read_trace
    #[serde(default)]
    pub trace_id: Option<String>,
    /// Optional trailing line limit for read_trace
    #[serde(default)]
    pub line_limit: Option<usize>,
    /// Whether to trigger immediate run after convert_session/promote_to_background (default false)
    #[serde(default)]
    pub run_now: Option<bool>,
    /// Whether to return assessment preview instead of executing
    #[serde(default)]
    pub preview: Option<bool>,
    /// Approval ID returned by a prior preview/confirmation_required response
    #[serde(default)]
    pub approval_id: Option<String>,
}

/// Parameters for manage_hooks tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ManageHooksParams {
    /// Operation to perform: list, create, update, delete, test
    pub operation: String,
    /// Hook ID (required for update/delete/test)
    #[serde(default)]
    pub id: Option<String>,
    /// Hook name (required for create)
    #[serde(default)]
    pub name: Option<String>,
    /// Optional description
    #[serde(default)]
    pub description: Option<Option<String>>,
    /// Hook event trigger (required for create): task_started, task_completed, task_failed, task_interrupted
    #[serde(default)]
    pub event: Option<String>,
    /// Hook action payload (required for create)
    #[serde(default)]
    pub action: Option<Value>,
    /// Optional filter to limit when the hook fires
    #[serde(default)]
    pub filter: Option<Option<Value>>,
    /// Whether the hook is enabled (default: true)
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// Skill summary for list_skills response
#[derive(Debug, Serialize, Deserialize)]
pub struct SkillSummary {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub status: SkillStatus,
}

/// Agent summary for list_agents response
#[derive(Debug, Serialize, Deserialize)]
pub struct AgentSummary {
    pub id: String,
    pub name: String,
    pub model: String,
    pub provider: String,
}

/// Empty parameters (for tools with no parameters)
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct EmptyParams {}
