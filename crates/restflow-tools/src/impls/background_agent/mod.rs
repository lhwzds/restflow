//! Background agent management tool.

#[cfg(test)]
mod tests;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::Result;
use crate::impls::team_template::{
    delete_team_document, list_team_entries, load_team_document, save_team_document,
};
use crate::{Tool, ToolError, ToolOutput};
use restflow_traits::store::{
    BackgroundAgentControlRequest, BackgroundAgentConvertSessionRequest,
    BackgroundAgentCreateRequest, BackgroundAgentDeliverableListRequest,
    BackgroundAgentMessageListRequest, BackgroundAgentMessageRequest,
    BackgroundAgentProgressRequest, BackgroundAgentStore, BackgroundAgentTraceListRequest,
    BackgroundAgentTraceReadRequest, BackgroundAgentUpdateRequest, KvStore,
    MANAGE_BACKGROUND_AGENT_OPERATIONS, MANAGE_BACKGROUND_AGENT_OPERATIONS_CSV,
    MANAGE_BACKGROUND_AGENTS_TOOL_DESCRIPTION,
};
use restflow_traits::{RuntimeTaskPayload, TeamTemplateDocument};

const BACKGROUND_AGENT_TEAM_NAMESPACE: &str = "background_agent_team";
const BACKGROUND_AGENT_TEAM_TYPE_HINT: &str = "background_agent_team";
const BACKGROUND_AGENT_TEAM_VERSION: u32 = 2;

fn default_worker_count() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackgroundBatchWorkerSpec {
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    input: Option<String>,
    #[serde(default)]
    inputs: Option<Vec<String>>,
    #[serde(default = "default_worker_count")]
    count: u32,
    #[serde(default)]
    chat_session_id: Option<String>,
    #[serde(default)]
    schedule: Option<Value>,
    #[serde(default)]
    timeout_secs: Option<u64>,
    #[serde(default)]
    durability_mode: Option<String>,
    #[serde(default)]
    memory: Option<Value>,
    #[serde(default)]
    memory_scope: Option<String>,
    #[serde(default)]
    resource_limits: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredBackgroundBatchWorkerSpec {
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default = "default_worker_count")]
    count: u32,
    #[serde(default)]
    chat_session_id: Option<String>,
    #[serde(default)]
    schedule: Option<Value>,
    #[serde(default)]
    timeout_secs: Option<u64>,
    #[serde(default)]
    durability_mode: Option<String>,
    #[serde(default)]
    memory: Option<Value>,
    #[serde(default)]
    memory_scope: Option<String>,
    #[serde(default)]
    resource_limits: Option<Value>,
}

#[derive(Clone)]
pub struct BackgroundAgentTool {
    store: Arc<dyn BackgroundAgentStore>,
    kv_store: Option<Arc<dyn KvStore>>,
    allow_write: bool,
}

impl BackgroundAgentTool {
    pub fn new(store: Arc<dyn BackgroundAgentStore>) -> Self {
        Self {
            store,
            kv_store: None,
            allow_write: false,
        }
    }

    pub fn with_write(mut self, allow_write: bool) -> Self {
        self.allow_write = allow_write;
        self
    }

    pub fn with_kv_store(mut self, kv_store: Arc<dyn KvStore>) -> Self {
        self.kv_store = Some(kv_store);
        self
    }

    fn write_guard(&self) -> Result<()> {
        if self.allow_write {
            Ok(())
        } else {
            Err(crate::ToolError::Tool(
                "Write access to background agents is disabled. Available read-only operations: list, progress, list_messages, list_deliverables, list_traces, read_trace, list_teams, get_team. To modify background agents, the user must grant write permissions.".to_string(),
            ))
        }
    }

    fn team_store(&self) -> Result<Arc<dyn KvStore>> {
        self.kv_store.clone().ok_or_else(|| {
            ToolError::Tool(
                "Team storage is unavailable in this runtime. Use 'workers' directly.".to_string(),
            )
        })
    }

    fn now_unix_seconds() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs() as i64)
    }

    fn normalize_structural_worker(
        worker: &BackgroundBatchWorkerSpec,
        worker_index: usize,
        strict_runtime_inputs: bool,
    ) -> Result<StoredBackgroundBatchWorkerSpec> {
        if strict_runtime_inputs && (worker.input.is_some() || worker.inputs.is_some()) {
            return Err(ToolError::Tool(format!(
                "save_team stores worker structure only. Remove 'input'/'inputs' from worker index {} and pass runtime input during run_batch.",
                worker_index
            )));
        }
        if worker.count == 0 {
            return Err(ToolError::Tool(format!(
                "Worker index {} count must be >= 1.",
                worker_index
            )));
        }
        Ok(StoredBackgroundBatchWorkerSpec {
            agent_id: worker.agent_id.clone(),
            name: worker.name.clone(),
            count: worker
                .inputs
                .as_ref()
                .map(|items| items.len() as u32)
                .unwrap_or(worker.count),
            chat_session_id: worker.chat_session_id.clone(),
            schedule: worker.schedule.clone(),
            timeout_secs: worker.timeout_secs,
            durability_mode: worker.durability_mode.clone(),
            memory: worker.memory.clone(),
            memory_scope: worker.memory_scope.clone(),
            resource_limits: worker.resource_limits.clone(),
        })
    }

    fn runtime_worker_from_stored(
        worker: StoredBackgroundBatchWorkerSpec,
    ) -> BackgroundBatchWorkerSpec {
        BackgroundBatchWorkerSpec {
            agent_id: worker.agent_id,
            name: worker.name,
            input: None,
            inputs: None,
            count: worker.count,
            chat_session_id: worker.chat_session_id,
            schedule: worker.schedule,
            timeout_secs: worker.timeout_secs,
            durability_mode: worker.durability_mode,
            memory: worker.memory,
            memory_scope: worker.memory_scope,
            resource_limits: worker.resource_limits,
        }
    }

    fn save_team_workers(
        &self,
        team_name: &str,
        workers: &[BackgroundBatchWorkerSpec],
        strict_runtime_inputs: bool,
    ) -> Result<Value> {
        if workers.is_empty() {
            return Err(ToolError::Tool(
                "save_team requires non-empty 'workers'.".to_string(),
            ));
        }
        let store = self.team_store()?;
        let members = workers
            .iter()
            .enumerate()
            .map(|(worker_index, worker)| {
                Self::normalize_structural_worker(worker, worker_index, strict_runtime_inputs)
            })
            .collect::<Result<Vec<_>>>()?;
        let persisted = save_team_document(
            store.as_ref(),
            BACKGROUND_AGENT_TEAM_NAMESPACE,
            BACKGROUND_AGENT_TEAM_TYPE_HINT,
            BACKGROUND_AGENT_TEAM_VERSION,
            team_name,
            members,
            Some(vec!["background_agent".to_string(), "team".to_string()]),
        )?;
        Ok(json!({
            "team": persisted.document.name,
            "workers": workers.len(),
            "created_at": persisted.document.created_at,
            "updated_at": persisted.document.updated_at,
            "storage": persisted.storage
        }))
    }

    fn load_team_workers(&self, team_name: &str) -> Result<Vec<BackgroundBatchWorkerSpec>> {
        let store = self.team_store()?;
        let team: TeamTemplateDocument<StoredBackgroundBatchWorkerSpec> =
            load_team_document(store.as_ref(), BACKGROUND_AGENT_TEAM_NAMESPACE, team_name)?;
        Ok(team
            .members
            .into_iter()
            .map(Self::runtime_worker_from_stored)
            .collect())
    }

    fn delete_team(&self, team_name: &str) -> Result<Value> {
        let store = self.team_store()?;
        delete_team_document(store.as_ref(), BACKGROUND_AGENT_TEAM_NAMESPACE, team_name)
    }

    fn list_teams(&self) -> Result<Value> {
        let store = self.team_store()?;
        let mut teams = Vec::new();
        for item in list_team_entries(store.as_ref(), BACKGROUND_AGENT_TEAM_NAMESPACE)? {
            let Some(key) = item.get("key").and_then(Value::as_str) else {
                continue;
            };
            let team_name = key
                .strip_prefix(&format!("{BACKGROUND_AGENT_TEAM_NAMESPACE}:"))
                .unwrap_or(key)
                .to_string();
            let (member_groups, total_instances, updated_at) =
                load_team_document::<StoredBackgroundBatchWorkerSpec>(
                    store.as_ref(),
                    BACKGROUND_AGENT_TEAM_NAMESPACE,
                    &team_name,
                )
                .ok()
                .map(|team| {
                    let total_instances = team
                        .members
                        .iter()
                        .map(|worker| worker.count as usize)
                        .sum::<usize>();
                    (team.members.len(), total_instances, team.updated_at)
                })
                .unwrap_or((0, 0, 0));
            teams.push(json!({
                "team": team_name,
                "member_groups": member_groups,
                "total_instances": total_instances,
                "updated_at": updated_at
            }));
        }
        teams.sort_by(|left, right| {
            right["updated_at"]
                .as_i64()
                .unwrap_or_default()
                .cmp(&left["updated_at"].as_i64().unwrap_or_default())
                .then_with(|| {
                    left["team"]
                        .as_str()
                        .unwrap_or_default()
                        .cmp(right["team"].as_str().unwrap_or_default())
                })
        });
        Ok(Value::Array(teams))
    }

    fn expand_worker_specs(
        workers: &[BackgroundBatchWorkerSpec],
        fallback_input: Option<&str>,
        fallback_inputs: Option<&[String]>,
    ) -> Result<Vec<(usize, String, BackgroundBatchWorkerSpec)>> {
        RuntimeTaskPayload {
            task: fallback_input.map(str::to_string),
            tasks: fallback_inputs.map(|items| items.to_vec()),
        }
        .validate("input", "inputs")
        .map_err(ToolError::Tool)?;

        if let Some(inputs) = fallback_inputs {
            if inputs.is_empty() {
                return Err(ToolError::Tool(
                    "Top-level 'inputs' must not be empty.".to_string(),
                ));
            }

            for (spec_index, spec) in workers.iter().enumerate() {
                if spec.input.is_some() || spec.inputs.is_some() {
                    return Err(ToolError::Tool(format!(
                        "Top-level 'inputs' cannot be combined with per-worker 'input' or 'inputs' (worker index {}).",
                        spec_index
                    )));
                }
                if spec.count == 0 {
                    return Err(ToolError::Tool(format!(
                        "Worker index {} count must be >= 1.",
                        spec_index
                    )));
                }
            }

            let expected = workers
                .iter()
                .map(|worker| worker.count as usize)
                .sum::<usize>();
            if inputs.len() != expected {
                return Err(ToolError::Tool(format!(
                    "Top-level 'inputs' length {} does not match total requested instances {}.",
                    inputs.len(),
                    expected
                )));
            }

            let mut expanded = Vec::with_capacity(expected);
            let mut offset = 0usize;
            for (spec_index, spec) in workers.iter().enumerate() {
                for instance_index in 0..spec.count as usize {
                    let input = inputs[offset + instance_index].trim();
                    if input.is_empty() {
                        return Err(ToolError::Tool(format!(
                            "Top-level 'inputs' has empty input at index {}.",
                            offset + instance_index
                        )));
                    }
                    expanded.push((spec_index, input.to_string(), spec.clone()));
                }
                offset += spec.count as usize;
            }
            return Ok(expanded);
        }

        let mut expanded = Vec::new();
        for (spec_index, spec) in workers.iter().enumerate() {
            if spec.input.is_some() && spec.inputs.is_some() {
                return Err(ToolError::Tool(format!(
                    "Worker index {} cannot set both 'input' and 'inputs'.",
                    spec_index
                )));
            }
            if let Some(inputs) = &spec.inputs {
                if inputs.is_empty() {
                    return Err(ToolError::Tool(format!(
                        "Worker index {} has empty 'inputs'.",
                        spec_index
                    )));
                }
                if spec.count != 1 && spec.count as usize != inputs.len() {
                    return Err(ToolError::Tool(format!(
                        "Worker index {} has count={} but inputs.len()={}. Set count to 1 (default) or match inputs length.",
                        spec_index,
                        spec.count,
                        inputs.len()
                    )));
                }
                for (instance_index, input) in inputs.iter().enumerate() {
                    let trimmed = input.trim();
                    if trimmed.is_empty() {
                        return Err(ToolError::Tool(format!(
                            "Worker index {} has empty input at inputs[{}].",
                            spec_index, instance_index
                        )));
                    }
                    expanded.push((spec_index, trimmed.to_string(), spec.clone()));
                }
                continue;
            }

            if spec.count == 0 {
                return Err(ToolError::Tool(
                    "Each worker count must be >= 1.".to_string(),
                ));
            }
            let resolved_input = spec
                .input
                .as_deref()
                .map(str::trim)
                .filter(|input| !input.is_empty())
                .or_else(|| {
                    fallback_input
                        .map(str::trim)
                        .filter(|input| !input.is_empty())
                })
                .ok_or_else(|| {
                    ToolError::Tool(format!(
                        "Worker index {} requires non-empty 'input' or top-level input.",
                        spec_index
                    ))
                })?;
            for _ in 0..spec.count {
                expanded.push((spec_index, resolved_input.to_string(), spec.clone()));
            }
        }
        if expanded.is_empty() {
            return Err(ToolError::Tool(
                "No background workers requested.".to_string(),
            ));
        }
        Ok(expanded)
    }

    fn extract_task_id(value: &Value) -> Option<String> {
        value
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                value
                    .get("task")
                    .and_then(|task| task.get("id"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
    }

    fn workers_schema() -> Value {
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
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum BackgroundAgentAction {
    Create {
        name: String,
        agent_id: String,
        #[serde(default)]
        chat_session_id: Option<String>,
        #[serde(default)]
        schedule: Option<Value>,
        #[serde(default)]
        input: Option<String>,
        #[serde(default)]
        input_template: Option<String>,
        #[serde(default)]
        timeout_secs: Option<u64>,
        #[serde(default)]
        durability_mode: Option<String>,
        #[serde(default)]
        memory: Option<Value>,
        #[serde(default)]
        memory_scope: Option<String>,
        #[serde(default)]
        resource_limits: Option<Value>,
    },
    ConvertSession {
        session_id: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        schedule: Option<Value>,
        #[serde(default)]
        input: Option<String>,
        #[serde(default)]
        timeout_secs: Option<u64>,
        #[serde(default)]
        durability_mode: Option<String>,
        #[serde(default)]
        memory: Option<Value>,
        #[serde(default)]
        memory_scope: Option<String>,
        #[serde(default)]
        resource_limits: Option<Value>,
        #[serde(default)]
        run_now: Option<bool>,
    },
    PromoteToBackground {
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        schedule: Option<Value>,
        #[serde(default)]
        input: Option<String>,
        #[serde(default)]
        timeout_secs: Option<u64>,
        #[serde(default)]
        durability_mode: Option<String>,
        #[serde(default)]
        memory: Option<Value>,
        #[serde(default)]
        memory_scope: Option<String>,
        #[serde(default)]
        resource_limits: Option<Value>,
        #[serde(default)]
        run_now: Option<bool>,
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
        schedule: Option<Value>,
        #[serde(default)]
        notification: Option<Value>,
        #[serde(default)]
        execution_mode: Option<Value>,
        #[serde(default)]
        timeout_secs: Option<u64>,
        #[serde(default)]
        durability_mode: Option<String>,
        #[serde(default)]
        memory: Option<Value>,
        #[serde(default)]
        memory_scope: Option<String>,
        #[serde(default)]
        resource_limits: Option<Value>,
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
        schedule: Option<Value>,
        #[serde(default)]
        timeout_secs: Option<u64>,
        #[serde(default)]
        durability_mode: Option<String>,
        #[serde(default)]
        memory: Option<Value>,
        #[serde(default)]
        memory_scope: Option<String>,
        #[serde(default)]
        resource_limits: Option<Value>,
        #[serde(default)]
        run_now: Option<bool>,
    },
    SaveTeam {
        team: String,
        workers: Vec<BackgroundBatchWorkerSpec>,
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
    },
}

#[async_trait]
impl Tool for BackgroundAgentTool {
    fn name(&self) -> &str {
        "manage_background_agents"
    }

    fn description(&self) -> &str {
        MANAGE_BACKGROUND_AGENTS_TOOL_DESCRIPTION
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": MANAGE_BACKGROUND_AGENT_OPERATIONS,
                    "description": "Background agent operation to perform"
                },
                "id": {
                    "type": "string"
                },
                "name": {
                    "type": "string",
                    "description": "Background agent name (for create/update)"
                },
                "agent_id": {
                    "type": "string",
                    "description": "Agent ID (for create/update)"
                },
                "session_id": {
                    "type": "string",
                    "description": "Source chat session ID (for convert_session/promote_to_background). For promote_to_background this is auto-injected from chat context when available."
                },
                "description": {
                    "type": "string",
                    "description": "Background agent description (for update)"
                },
                "chat_session_id": {
                    "type": "string",
                    "description": "Optional bound chat session ID (for create/update). If omitted on create, backend creates one."
                },
                "schedule": {
                    "type": "object",
                    "description": "Background agent schedule object (for create/update)"
                },
                "notification": {
                    "type": "object",
                    "description": "Notification configuration (for update)"
                },
                "execution_mode": {
                    "type": "object",
                    "description": "Execution mode payload (for update)"
                },
                "memory": {
                    "type": "object",
                    "description": "Memory configuration payload (for create/update)"
                },
                "timeout_secs": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Optional per-task timeout in seconds for API execution mode (for create/update)"
                },
                "durability_mode": {
                    "type": "string",
                    "enum": ["sync", "async", "exit"],
                    "description": "Checkpoint durability mode (for create/update)"
                },
                "input": {
                    "type": "string",
                    "description": "Optional input for the background agent (for create/update)"
                },
                "inputs": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional per-instance input list for run_batch. Inputs are assigned in worker order and are never persisted in saved teams."
                },
                "input_template": {
                    "type": "string",
                    "description": "Optional runtime template for background agent input (for create/update)"
                },
                "memory_scope": {
                    "type": "string",
                    "enum": ["shared_agent", "per_background_agent"],
                    "description": "Memory namespace scope (for create/update)"
                },
                "resource_limits": {
                    "type": "object",
                    "description": "Resource limits payload (for create/update/convert_session/promote_to_background)"
                },
                "run_now": {
                    "type": "boolean",
                    "description": "Whether to trigger immediate run after convert_session/promote_to_background (default: true)"
                },
                "team": {
                    "type": "string",
                    "description": "Team name for save_team/get_team/delete_team, or run_batch from saved team."
                },
                "save_as_team": {
                    "type": "string",
                    "description": "Optionally save provided workers as a team during run_batch."
                },
                "workers": Self::workers_schema(),
                "status": {
                    "type": "string",
                    "description": "Filter list by status (for list)"
                },
                "action": {
                    "type": "string",
                    "enum": ["start", "pause", "resume", "stop", "run_now"],
                    "description": "Control action (for control)"
                },
                "event_limit": {
                    "type": "integer",
                    "description": "Recent event count for progress"
                },
                "message": {
                    "type": "string",
                    "description": "Message content for send_message"
                },
                "source": {
                    "type": "string",
                    "enum": ["user", "agent", "system"],
                    "description": "Message source for send_message"
                },
                "limit": {
                    "type": "integer",
                    "description": "Message list limit for list_messages"
                },
                "trace_id": {
                    "type": "string",
                    "description": "Trace ID returned by list_traces (for read_trace)"
                },
                "line_limit": {
                    "type": "integer",
                    "description": "Maximum number of trailing lines returned by read_trace"
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let action: BackgroundAgentAction = match serde_json::from_value(input) {
            Ok(action) => action,
            Err(e) => {
                return Ok(ToolOutput::error(format!(
                    "Invalid input: {e}. Supported operations: {}.",
                    MANAGE_BACKGROUND_AGENT_OPERATIONS_CSV
                )));
            }
        };

        let output = match action {
            BackgroundAgentAction::List { status } => {
                let result = self.store.list_background_agents(status).map_err(|e| {
                    ToolError::Tool(format!("Failed to list background agent: {e}."))
                })?;
                ToolOutput::success(result)
            }
            BackgroundAgentAction::RunBatch {
                agent_id,
                name,
                input,
                inputs,
                workers,
                team,
                save_as_team,
                input_template,
                chat_session_id,
                schedule,
                timeout_secs,
                durability_mode,
                memory,
                memory_scope,
                resource_limits,
                run_now,
            } => {
                self.write_guard()?;
                if input_template.is_some() {
                    return Err(ToolError::Tool(
                        "run_batch does not support 'input_template'. Pass runtime 'input' or 'inputs' instead.".to_string(),
                    ));
                }

                let run_group_id = format!("bg-batch-{}", Self::now_unix_seconds());
                let resolved_workers = match (workers, team.as_deref()) {
                    (Some(_), Some(_)) => {
                        return Err(ToolError::Tool(
                            "run_batch accepts either 'workers' or 'team', not both.".to_string(),
                        ));
                    }
                    (Some(specs), None) => specs,
                    (None, Some(team_name)) => self.load_team_workers(team_name)?,
                    (None, None) => {
                        return Err(ToolError::Tool(
                            "run_batch requires 'workers' or 'team'.".to_string(),
                        ));
                    }
                };

                if let Some(team_name) = save_as_team.as_deref() {
                    self.save_team_workers(team_name, &resolved_workers, false)?;
                }

                let expanded_workers = Self::expand_worker_specs(
                    &resolved_workers,
                    input.as_deref(),
                    inputs.as_deref(),
                )?;
                let should_run_now = run_now.unwrap_or(true);
                let default_name_prefix =
                    name.unwrap_or_else(|| format!("Background Batch {}", run_group_id));
                let mut tasks = Vec::with_capacity(expanded_workers.len());

                for (worker_index, (spec_index, worker_input, worker_spec)) in
                    expanded_workers.into_iter().enumerate()
                {
                    let resolved_agent_id = worker_spec
                        .agent_id
                        .clone()
                        .or_else(|| agent_id.clone())
                        .ok_or_else(|| {
                            ToolError::Tool(format!(
                                "Worker index {} requires agent_id (set per worker or top-level).",
                                spec_index
                            ))
                        })?;
                    let worker_name = worker_spec.name.clone().unwrap_or_else(|| {
                        format!("{} - {}", default_name_prefix, worker_index + 1)
                    });
                    let created = self
                        .store
                        .create_background_agent(BackgroundAgentCreateRequest {
                            name: worker_name,
                            agent_id: resolved_agent_id,
                            chat_session_id: worker_spec
                                .chat_session_id
                                .clone()
                                .or_else(|| chat_session_id.clone()),
                            schedule: worker_spec.schedule.clone().or_else(|| schedule.clone()),
                            input: Some(worker_input),
                            input_template: None,
                            timeout_secs: worker_spec.timeout_secs.or(timeout_secs),
                            durability_mode: worker_spec
                                .durability_mode
                                .clone()
                                .or_else(|| durability_mode.clone()),
                            memory: worker_spec.memory.clone().or_else(|| memory.clone()),
                            memory_scope: worker_spec
                                .memory_scope
                                .clone()
                                .or_else(|| memory_scope.clone()),
                            resource_limits: worker_spec
                                .resource_limits
                                .clone()
                                .or_else(|| resource_limits.clone()),
                        })
                        .map_err(|e| {
                            ToolError::Tool(format!(
                                "Failed to create background agent for worker {}: {e}.",
                                worker_index + 1
                            ))
                        })?;

                    let task_id = Self::extract_task_id(&created).ok_or_else(|| {
                        ToolError::Tool(format!(
                            "Failed to extract task id from worker {} create result.",
                            worker_index + 1
                        ))
                    })?;

                    if should_run_now {
                        self.store
                            .control_background_agent(BackgroundAgentControlRequest {
                                id: task_id.clone(),
                                action: "run_now".to_string(),
                            })
                            .map_err(|e| {
                                ToolError::Tool(format!(
                                    "Failed to run background agent {}: {e}.",
                                    task_id
                                ))
                            })?;
                    }

                    tasks.push(json!({
                        "run_group_id": run_group_id.clone(),
                        "worker_index": worker_index,
                        "spec_index": spec_index,
                        "task_id": task_id,
                        "run_now": should_run_now,
                        "task": created
                    }));
                }

                ToolOutput::success(json!({
                    "operation": "run_batch",
                    "run_group_id": run_group_id,
                    "total": tasks.len(),
                    "run_now": should_run_now,
                    "team": team,
                    "tasks": tasks
                }))
            }
            BackgroundAgentAction::SaveTeam { team, workers } => {
                self.write_guard()?;
                let payload = self.save_team_workers(&team, &workers, true)?;
                ToolOutput::success(json!({
                    "operation": "save_team",
                    "result": payload
                }))
            }
            BackgroundAgentAction::ListTeams => {
                let payload = self.list_teams()?;
                ToolOutput::success(json!({
                    "operation": "list_teams",
                    "teams": payload
                }))
            }
            BackgroundAgentAction::GetTeam { team } => {
                let store = self.team_store()?;
                let document: TeamTemplateDocument<StoredBackgroundBatchWorkerSpec> =
                    load_team_document(store.as_ref(), BACKGROUND_AGENT_TEAM_NAMESPACE, &team)?;
                let members = document
                    .members
                    .clone()
                    .into_iter()
                    .map(Self::runtime_worker_from_stored)
                    .collect::<Vec<_>>();
                ToolOutput::success(json!({
                    "operation": "get_team",
                    "team": document.name,
                    "version": document.version,
                    "created_at": document.created_at,
                    "updated_at": document.updated_at,
                    "member_groups": members.len(),
                    "total_instances": document.members.iter().map(|worker| worker.count as usize).sum::<usize>(),
                    "members": members
                }))
            }
            BackgroundAgentAction::DeleteTeam { team } => {
                self.write_guard()?;
                let payload = self.delete_team(&team)?;
                ToolOutput::success(json!({
                    "operation": "delete_team",
                    "result": payload
                }))
            }
            BackgroundAgentAction::Create {
                name,
                agent_id,
                chat_session_id,
                schedule,
                input,
                input_template,
                timeout_secs,
                durability_mode,
                memory,
                memory_scope,
                resource_limits,
            } => {
                self.write_guard()?;
                let result = self
                    .store
                    .create_background_agent(BackgroundAgentCreateRequest {
                        name,
                        agent_id,
                        chat_session_id,
                        schedule,
                        input,
                        input_template,
                        timeout_secs,
                        durability_mode,
                        memory,
                        memory_scope,
                        resource_limits,
                    })
                    .map_err(|e| {
                        ToolError::Tool(format!("Failed to create background agent: {e}."))
                    })?;
                ToolOutput::success(result)
            }
            BackgroundAgentAction::ConvertSession {
                session_id,
                name,
                schedule,
                input,
                timeout_secs,
                durability_mode,
                memory,
                memory_scope,
                resource_limits,
                run_now,
            } => {
                self.write_guard()?;
                let result = self
                    .store
                    .convert_session_to_background_agent(BackgroundAgentConvertSessionRequest {
                        session_id,
                        name,
                        schedule,
                        input,
                        timeout_secs,
                        durability_mode,
                        memory,
                        memory_scope,
                        resource_limits,
                        run_now,
                    })
                    .map_err(|e| {
                        ToolError::Tool(format!(
                            "Failed to convert session into background agent: {e}."
                        ))
                    })?;
                ToolOutput::success(result)
            }
            BackgroundAgentAction::PromoteToBackground {
                session_id,
                name,
                schedule,
                input,
                timeout_secs,
                durability_mode,
                memory,
                memory_scope,
                resource_limits,
                run_now,
            } => {
                self.write_guard()?;
                let session_id = session_id.ok_or_else(|| {
                    ToolError::Tool(
                        "promote_to_background requires session_id (runtime should auto-inject it for interactive chat sessions)"
                            .to_string(),
                    )
                })?;
                let result = self
                    .store
                    .convert_session_to_background_agent(BackgroundAgentConvertSessionRequest {
                        session_id,
                        name,
                        schedule,
                        input,
                        timeout_secs,
                        durability_mode,
                        memory,
                        memory_scope,
                        resource_limits,
                        run_now,
                    })
                    .map_err(|e| {
                        ToolError::Tool(format!(
                            "Failed to promote session into background agent: {e}."
                        ))
                    })?;
                ToolOutput::success(result)
            }
            BackgroundAgentAction::Update {
                id,
                name,
                description,
                agent_id,
                chat_session_id,
                input,
                input_template,
                schedule,
                notification,
                execution_mode,
                timeout_secs,
                durability_mode,
                memory,
                memory_scope,
                resource_limits,
            } => {
                self.write_guard()?;
                let result = self
                    .store
                    .update_background_agent(BackgroundAgentUpdateRequest {
                        id,
                        name,
                        description,
                        agent_id,
                        chat_session_id,
                        input,
                        input_template,
                        schedule,
                        notification,
                        execution_mode,
                        timeout_secs,
                        durability_mode,
                        memory,
                        memory_scope,
                        resource_limits,
                    })
                    .map_err(|e| {
                        ToolError::Tool(format!("Failed to update background agent: {e}."))
                    })?;
                ToolOutput::success(result)
            }
            BackgroundAgentAction::Delete { id } => {
                self.write_guard()?;
                ToolOutput::success(self.store.delete_background_agent(&id).map_err(|e| {
                    ToolError::Tool(format!("Failed to delete background agent: {e}."))
                })?)
            }
            BackgroundAgentAction::Pause { id } => {
                self.write_guard()?;
                ToolOutput::success(
                    self.store
                        .control_background_agent(BackgroundAgentControlRequest {
                            id,
                            action: "pause".to_string(),
                        })
                        .map_err(|e| {
                            ToolError::Tool(format!("Failed to pause background agent: {e}."))
                        })?,
                )
            }
            BackgroundAgentAction::Start { id } => {
                self.write_guard()?;
                ToolOutput::success(
                    self.store
                        .control_background_agent(BackgroundAgentControlRequest {
                            id,
                            action: "start".to_string(),
                        })
                        .map_err(|e| {
                            ToolError::Tool(format!("Failed to start background agent: {e}."))
                        })?,
                )
            }
            BackgroundAgentAction::Resume { id } => {
                self.write_guard()?;
                ToolOutput::success(
                    self.store
                        .control_background_agent(BackgroundAgentControlRequest {
                            id,
                            action: "resume".to_string(),
                        })
                        .map_err(|e| {
                            ToolError::Tool(format!("Failed to resume background agent: {e}."))
                        })?,
                )
            }
            BackgroundAgentAction::Stop { id } => {
                self.write_guard()?;
                ToolOutput::success(
                    self.store
                        .control_background_agent(BackgroundAgentControlRequest {
                            id,
                            action: "stop".to_string(),
                        })
                        .map_err(|e| {
                            ToolError::Tool(format!("Failed to stop background agent: {e}."))
                        })?,
                )
            }
            BackgroundAgentAction::Run { id } => {
                self.write_guard()?;
                ToolOutput::success(
                    self.store
                        .control_background_agent(BackgroundAgentControlRequest {
                            id,
                            action: "run_now".to_string(),
                        })
                        .map_err(|e| {
                            ToolError::Tool(format!("Failed to run background agent: {e}."))
                        })?,
                )
            }
            BackgroundAgentAction::Control { id, action } => {
                self.write_guard()?;
                ToolOutput::success(
                    self.store
                        .control_background_agent(BackgroundAgentControlRequest { id, action })
                        .map_err(|e| {
                            ToolError::Tool(format!("Failed to control background agent: {e}."))
                        })?,
                )
            }
            BackgroundAgentAction::Progress { id, event_limit } => ToolOutput::success(
                self.store
                    .get_background_agent_progress(BackgroundAgentProgressRequest {
                        id,
                        event_limit,
                    })
                    .map_err(|e| {
                        ToolError::Tool(format!("Failed to get background agent: {e}."))
                    })?,
            ),
            BackgroundAgentAction::SendMessage {
                id,
                message,
                source,
            } => {
                self.write_guard()?;
                ToolOutput::success(
                    self.store
                        .send_background_agent_message(BackgroundAgentMessageRequest {
                            id,
                            message,
                            source,
                        })
                        .map_err(|e| {
                            ToolError::Tool(format!(
                                "Failed to send message background agent: {e}."
                            ))
                        })?,
                )
            }
            BackgroundAgentAction::ListMessages { id, limit } => ToolOutput::success(
                self.store
                    .list_background_agent_messages(BackgroundAgentMessageListRequest { id, limit })
                    .map_err(|e| {
                        ToolError::Tool(format!("Failed to list messages background agent: {e}."))
                    })?,
            ),
            BackgroundAgentAction::ListDeliverables { id } => ToolOutput::success(
                self.store
                    .list_background_agent_deliverables(BackgroundAgentDeliverableListRequest {
                        id,
                    })
                    .map_err(|e| {
                        ToolError::Tool(format!(
                            "Failed to list deliverables background agent: {e}."
                        ))
                    })?,
            ),
            BackgroundAgentAction::ListTraces { id, limit } => ToolOutput::success(
                self.store
                    .list_background_agent_traces(BackgroundAgentTraceListRequest { id, limit })
                    .map_err(|e| {
                        ToolError::Tool(format!("Failed to list traces for background agent: {e}."))
                    })?,
            ),
            BackgroundAgentAction::ReadTrace {
                trace_id,
                line_limit,
            } => ToolOutput::success(
                self.store
                    .read_background_agent_trace(BackgroundAgentTraceReadRequest {
                        trace_id,
                        line_limit,
                    })
                    .map_err(|e| {
                        ToolError::Tool(format!("Failed to read trace for background agent: {e}."))
                    })?,
            ),
        };

        Ok(output)
    }
}
