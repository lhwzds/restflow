use serde::{Deserialize, Serialize};

use super::super::spawn_subagent_batch::{BatchSubagentSpec, SpawnSubagentBatchOperation};

#[cfg(feature = "ts")]
const TS_EXPORT_TO_WEB_TYPES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../web/src/types/generated/"
);

/// Parameters for spawn_subagent tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = TS_EXPORT_TO_WEB_TYPES))]
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
    /// Required for single spawn. Optional for team management operations.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub task: Option<String>,

    /// Transient per-instance task list for batch or team spawn.
    ///
    /// Tasks are assigned in worker order and are never persisted in saved teams.
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

    /// Optional parent execution ID (runtime-injected, internal use).
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(default)]
    pub parent_execution_id: Option<String>,

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

    /// Optional team name for batch mode spawn.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub team: Option<String>,

    /// Optionally persist the provided workers as a named team during spawn.
    ///
    /// To save a team without spawning, use `operation = "save_team"` and `team`.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub save_as_team: Option<String>,

    /// If true, validate and preview capability warnings/blockers without executing.
    #[serde(default)]
    pub preview: bool,

    /// Approval ID returned by preview when warnings require explicit confirmation.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub approval_id: Option<String>,
}
