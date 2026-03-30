use restflow_contracts::request::{
    DurabilityMode as ContractDurabilityMode, ExecutionMode as ContractExecutionMode,
    MemoryConfig as ContractMemoryConfig, NotificationConfig as ContractNotificationConfig,
    ResourceLimits as ContractResourceLimits, TaskSchedule as ContractTaskSchedule,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

fn default_worker_count() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct BackgroundBatchWorkerSpec {
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub inputs: Option<Vec<String>>,
    #[serde(default = "default_worker_count")]
    pub count: u32,
    #[serde(default)]
    pub chat_session_id: Option<String>,
    #[serde(default)]
    pub schedule: Option<ContractTaskSchedule>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct StoredBackgroundBatchWorkerSpec {
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default = "default_worker_count")]
    pub count: u32,
    #[serde(default)]
    pub chat_session_id: Option<String>,
    #[serde(default)]
    pub schedule: Option<ContractTaskSchedule>,
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
}

pub(super) fn workers_schema() -> Value {
    json!({
        "type": "array",
        "description": "Worker specs for run_batch and save_team.",
        "items": {
            "type": "object",
            "properties": {
                "agent_id": { "type": "string", "description": "Optional per-worker agent ID override." },
                "name": { "type": "string", "description": "Optional per-worker background task name." },
                "input": { "type": "string", "description": "Optional per-worker input text." },
                "inputs": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional per-instance input list for distinct prompts."
                },
                "count": { "type": "integer", "minimum": 1, "default": 1, "description": "Number of instances for this worker when inputs is not set." },
                "chat_session_id": { "type": "string", "description": "Optional bound chat session ID for worker-created tasks." },
                "schedule": { "type": "object", "description": "Optional per-worker schedule payload." },
                "timeout_secs": { "type": "integer", "minimum": 1, "description": "Optional per-worker timeout override." },
                "durability_mode": { "type": "string", "enum": ["sync", "async", "exit"], "description": "Optional per-worker durability mode." },
                "memory": { "type": "object", "description": "Optional per-worker memory payload." },
                "memory_scope": { "type": "string", "enum": ["shared_agent", "per_background_agent"], "description": "Optional per-worker memory scope override." },
                "resource_limits": { "type": "object", "description": "Optional per-worker resource limits payload." }
            }
        }
    })
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub(super) enum BackgroundAgentAction {
    Create {
        name: String,
        agent_id: String,
        #[serde(default)]
        chat_session_id: Option<String>,
        schedule: ContractTaskSchedule,
        #[serde(default)]
        input: Option<String>,
        #[serde(default)]
        input_template: Option<String>,
        #[serde(default)]
        timeout_secs: Option<u64>,
        #[serde(default)]
        durability_mode: Option<ContractDurabilityMode>,
        #[serde(default)]
        memory: Option<ContractMemoryConfig>,
        #[serde(default)]
        memory_scope: Option<String>,
        #[serde(default)]
        resource_limits: Option<ContractResourceLimits>,
        #[serde(default)]
        preview: bool,
        #[serde(default)]
        confirmation_token: Option<String>,
    },
    ConvertSession {
        session_id: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        schedule: Option<ContractTaskSchedule>,
        #[serde(default)]
        input: Option<String>,
        #[serde(default)]
        timeout_secs: Option<u64>,
        #[serde(default)]
        durability_mode: Option<ContractDurabilityMode>,
        #[serde(default)]
        memory: Option<ContractMemoryConfig>,
        #[serde(default)]
        memory_scope: Option<String>,
        #[serde(default)]
        resource_limits: Option<ContractResourceLimits>,
        #[serde(default)]
        run_now: Option<bool>,
        #[serde(default)]
        preview: bool,
        #[serde(default)]
        confirmation_token: Option<String>,
    },
    PromoteToBackground {
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        schedule: Option<ContractTaskSchedule>,
        #[serde(default)]
        input: Option<String>,
        #[serde(default)]
        timeout_secs: Option<u64>,
        #[serde(default)]
        durability_mode: Option<ContractDurabilityMode>,
        #[serde(default)]
        memory: Option<ContractMemoryConfig>,
        #[serde(default)]
        memory_scope: Option<String>,
        #[serde(default)]
        resource_limits: Option<ContractResourceLimits>,
        #[serde(default)]
        run_now: Option<bool>,
        #[serde(default)]
        preview: bool,
        #[serde(default)]
        confirmation_token: Option<String>,
    },
    Update {
        id: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        agent_id: Option<String>,
        #[serde(default)]
        chat_session_id: Option<String>,
        #[serde(default)]
        input: Option<String>,
        #[serde(default)]
        input_template: Option<String>,
        #[serde(default)]
        schedule: Option<ContractTaskSchedule>,
        #[serde(default)]
        notification: Option<ContractNotificationConfig>,
        #[serde(default)]
        execution_mode: Option<ContractExecutionMode>,
        #[serde(default)]
        timeout_secs: Option<u64>,
        #[serde(default)]
        durability_mode: Option<ContractDurabilityMode>,
        #[serde(default)]
        memory: Option<ContractMemoryConfig>,
        #[serde(default)]
        memory_scope: Option<String>,
        #[serde(default)]
        resource_limits: Option<ContractResourceLimits>,
        #[serde(default)]
        preview: bool,
        #[serde(default)]
        confirmation_token: Option<String>,
    },
    Delete {
        id: String,
    },
    List {
        #[serde(default)]
        status: Option<String>,
    },
    RunBatch {
        #[serde(default)]
        agent_id: Option<String>,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        input: Option<String>,
        #[serde(default)]
        inputs: Option<Vec<String>>,
        #[serde(default)]
        workers: Option<Vec<BackgroundBatchWorkerSpec>>,
        #[serde(default)]
        team: Option<String>,
        #[serde(default)]
        save_as_team: Option<String>,
        #[serde(default)]
        input_template: Option<String>,
        #[serde(default)]
        chat_session_id: Option<String>,
        #[serde(default)]
        schedule: Option<ContractTaskSchedule>,
        #[serde(default)]
        timeout_secs: Option<u64>,
        #[serde(default)]
        durability_mode: Option<ContractDurabilityMode>,
        #[serde(default)]
        memory: Option<ContractMemoryConfig>,
        #[serde(default)]
        memory_scope: Option<String>,
        #[serde(default)]
        resource_limits: Option<ContractResourceLimits>,
        #[serde(default)]
        run_now: Option<bool>,
        #[serde(default)]
        preview: bool,
        #[serde(default)]
        confirmation_token: Option<String>,
    },
    SaveTeam {
        team: String,
        workers: Vec<BackgroundBatchWorkerSpec>,
        #[serde(default)]
        preview: bool,
        #[serde(default)]
        confirmation_token: Option<String>,
    },
    ListTeams,
    GetTeam {
        team: String,
    },
    DeleteTeam {
        team: String,
    },
    Control {
        id: String,
        action: String,
        #[serde(default)]
        preview: bool,
        #[serde(default)]
        confirmation_token: Option<String>,
    },
    Progress {
        id: String,
        #[serde(default)]
        event_limit: Option<usize>,
    },
    SendMessage {
        id: String,
        message: String,
        #[serde(default)]
        source: Option<String>,
    },
    ListMessages {
        id: String,
        #[serde(default)]
        limit: Option<usize>,
    },
    ListDeliverables {
        id: String,
    },
    ListTraces {
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        limit: Option<usize>,
    },
    ReadTrace {
        trace_id: String,
        #[serde(default)]
        line_limit: Option<usize>,
    },
    Pause {
        id: String,
    },
    Start {
        id: String,
    },
    Resume {
        id: String,
    },
    Stop {
        id: String,
    },
    Run {
        id: String,
        #[serde(default)]
        preview: bool,
        #[serde(default)]
        confirmation_token: Option<String>,
    },
}
