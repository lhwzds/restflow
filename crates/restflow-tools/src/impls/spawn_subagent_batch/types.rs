use serde::{Deserialize, Serialize};

use crate::ToolError;
use restflow_contracts::request::RunSpawnRequest as ContractRunSpawnRequest;
use restflow_traits::SubagentEffectiveLimits;

/// Operation for spawn_subagent_batch tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "snake_case")]
pub enum SpawnSubagentBatchOperation {
    /// Spawn one batch of sub-agents immediately.
    #[default]
    Spawn,
}

fn default_member_count() -> u32 {
    1
}

/// One batch member specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct BatchSubagentSpec {
    /// Optional agent ID or name.
    ///
    /// If omitted, a temporary sub-agent is created from inline fields or defaults.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub agent: Option<String>,

    /// Number of identical sub-agents to spawn for this spec.
    #[serde(default = "default_member_count")]
    pub count: u32,

    /// Optional transient per-spec task override.
    ///
    /// If omitted, top-level `task` is used.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub task: Option<String>,

    /// Optional transient per-instance task list.
    ///
    /// When provided, each spawned instance uses the corresponding entry in this list.
    /// This allows one worker spec to fan out with distinct prompts.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub tasks: Option<Vec<String>>,

    /// Optional per-spec timeout (seconds) passed to sub-agent execution.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub timeout_secs: Option<u64>,

    /// Optional model override.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub model: Option<String>,

    /// Optional provider override paired with model.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub provider: Option<String>,

    /// Optional name for temporary sub-agent creation.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub inline_name: Option<String>,

    /// Optional system prompt for temporary sub-agent creation.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub inline_system_prompt: Option<String>,

    /// Optional allowlist for temporary sub-agent tools.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub inline_allowed_tools: Option<Vec<String>>,

    /// Optional max iterations override for temporary sub-agent creation.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub inline_max_iterations: Option<u32>,
}

/// Parameters for spawn_subagent_batch tool.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct SpawnSubagentBatchParams {
    /// Operation to perform.
    #[serde(default)]
    pub operation: SpawnSubagentBatchOperation,

    /// Batch member specs.
    ///
    /// For `spawn`, `specs` is required.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub specs: Option<Vec<BatchSubagentSpec>>,

    /// Default transient task for all specs that do not set per-spec `task`.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub task: Option<String>,

    /// Transient per-instance task list for this spawn.
    ///
    /// When provided, tasks are assigned across all instances in spec order.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub tasks: Option<Vec<String>>,

    /// If true, wait for all spawned tasks to complete.
    #[serde(default)]
    pub wait: bool,

    /// Timeout in seconds for wait and as fallback spawn timeout.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub timeout_secs: Option<u64>,

    /// Optional parent run ID for context propagation (runtime-injected).
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub parent_run_id: Option<String>,

    /// Optional trace session ID for context propagation (runtime-injected).
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub trace_session_id: Option<String>,

    /// Optional trace scope ID for context propagation (runtime-injected).
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub trace_scope_id: Option<String>,

    /// If true, validate and preview capability warnings/blockers without executing.
    #[serde(default)]
    pub preview: bool,

    /// Approval ID returned by preview when warnings require explicit confirmation.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub approval_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawSpawnSubagentBatchParams {
    #[serde(default)]
    operation: SpawnSubagentBatchOperation,
    #[serde(default)]
    specs: Option<Vec<BatchSubagentSpec>>,
    #[serde(default)]
    task: Option<String>,
    #[serde(default)]
    tasks: Option<Vec<String>>,
    #[serde(default)]
    wait: bool,
    #[serde(default)]
    timeout_secs: Option<u64>,
    #[serde(default)]
    parent_run_id: Option<String>,
    #[serde(default)]
    parent_execution_id: Option<String>,
    #[serde(default)]
    trace_session_id: Option<String>,
    #[serde(default)]
    trace_scope_id: Option<String>,
    #[serde(default)]
    preview: bool,
    #[serde(default)]
    approval_id: Option<String>,
}

impl<'de> Deserialize<'de> for SpawnSubagentBatchParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawSpawnSubagentBatchParams::deserialize(deserializer)?;
        Ok(Self {
            operation: raw.operation,
            specs: raw.specs,
            task: raw.task,
            tasks: raw.tasks,
            wait: raw.wait,
            timeout_secs: raw.timeout_secs,
            parent_run_id: raw.parent_run_id.or(raw.parent_execution_id),
            trace_session_id: raw.trace_session_id,
            trace_scope_id: raw.trace_scope_id,
            preview: raw.preview,
            approval_id: raw.approval_id,
        })
    }
}

#[derive(Debug, Clone)]
pub(super) struct SpawnedTask {
    pub(super) task_id: String,
    pub(super) agent_name: String,
    pub(super) spec_index: usize,
    pub(super) instance_index: u32,
    pub(super) effective_limits: SubagentEffectiveLimits,
}

#[derive(Debug, Clone)]
pub(super) struct PreparedSpawnRequest {
    pub(super) spec_index: usize,
    pub(super) instance_index: u32,
    pub(super) request: ContractRunSpawnRequest,
}

#[derive(Debug)]
pub(super) struct SpawnFailure {
    pub(super) spec_index: usize,
    pub(super) instance_index: u32,
    pub(super) error: ToolError,
}
