use serde::{Deserialize, Deserializer, Serialize};

use super::super::spawn_subagent_batch::{BatchSubagentSpec, SpawnSubagentBatchOperation};

/// Parameters for spawn_subagent tool.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct SpawnSubagentParams {
    /// Operation to perform. Defaults to `spawn`.
    #[serde(default)]
    pub operation: SpawnSubagentBatchOperation,

    /// Agent type to spawn (researcher, coder, reviewer, writer, analyst).
    ///
    /// When omitted, runtime creates a temporary sub-agent from inline config.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub agent: Option<String>,

    /// Task description for single spawn, or transient fallback task for batch spawn.
    ///
    /// Required for single spawn. Optional when per-worker tasks are provided.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub task: Option<String>,

    /// Transient per-instance task list for batch spawn.
    ///
    /// Tasks are assigned in worker order.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub tasks: Option<Vec<String>>,

    /// If true, wait for completion. If false (default), run concurrently.
    #[serde(default)]
    pub wait: bool,

    /// Timeout in seconds. If omitted, uses sub-agent manager default timeout.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub timeout_secs: Option<u64>,

    /// Optional model override for this spawn (e.g., "minimax/coding-plan").
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub model: Option<String>,

    /// Optional provider selector paired with model (e.g., "openai-codex").
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub provider: Option<String>,

    /// Optional parent run ID (runtime-injected, internal use).
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(default)]
    pub parent_run_id: Option<String>,

    /// Optional trace session ID (runtime-injected, internal use).
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(default)]
    pub trace_session_id: Option<String>,

    /// Optional trace scope ID (runtime-injected, internal use).
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(default)]
    pub trace_scope_id: Option<String>,

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

    /// Optional list-based worker specs for unified single/multi spawn.
    ///
    /// When provided, this tool enters batch mode and spawns one or more workers.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub workers: Option<Vec<BatchSubagentSpec>>,

    /// If true, validate and preview capability warnings/blockers without executing.
    #[serde(default)]
    pub preview: bool,

    /// Approval ID returned by preview when warnings require explicit confirmation.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub approval_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawSpawnSubagentParams {
    #[serde(default)]
    operation: SpawnSubagentBatchOperation,
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    task: Option<String>,
    #[serde(default)]
    tasks: Option<Vec<String>>,
    #[serde(default)]
    wait: bool,
    #[serde(default)]
    timeout_secs: Option<u64>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    parent_run_id: Option<String>,
    #[serde(default)]
    parent_execution_id: Option<String>,
    #[serde(default)]
    trace_session_id: Option<String>,
    #[serde(default)]
    trace_scope_id: Option<String>,
    #[serde(default)]
    inline_name: Option<String>,
    #[serde(default)]
    inline_system_prompt: Option<String>,
    #[serde(default)]
    inline_allowed_tools: Option<Vec<String>>,
    #[serde(default)]
    inline_max_iterations: Option<u32>,
    #[serde(default)]
    workers: Option<Vec<BatchSubagentSpec>>,
    #[serde(default)]
    preview: bool,
    #[serde(default)]
    approval_id: Option<String>,
}

impl<'de> Deserialize<'de> for SpawnSubagentParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawSpawnSubagentParams::deserialize(deserializer)?;
        Ok(Self {
            operation: raw.operation,
            agent: raw.agent,
            task: raw.task,
            tasks: raw.tasks,
            wait: raw.wait,
            timeout_secs: raw.timeout_secs,
            model: raw.model,
            provider: raw.provider,
            parent_run_id: raw.parent_run_id.or(raw.parent_execution_id),
            trace_session_id: raw.trace_session_id,
            trace_scope_id: raw.trace_scope_id,
            inline_name: raw.inline_name,
            inline_system_prompt: raw.inline_system_prompt,
            inline_allowed_tools: raw.inline_allowed_tools,
            inline_max_iterations: raw.inline_max_iterations,
            workers: raw.workers,
            preview: raw.preview,
            approval_id: raw.approval_id,
        })
    }
}
