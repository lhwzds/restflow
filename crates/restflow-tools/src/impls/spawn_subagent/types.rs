use serde::{Deserialize, Serialize};

use super::super::spawn_subagent_batch::{BatchSubagentSpec, SpawnSubagentBatchOperation};

/// Parameters for spawn_subagent tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnSubagentParams {
    /// Operation to perform. Defaults to `spawn`.
    #[serde(default)]
    pub operation: SpawnSubagentBatchOperation,

    /// Agent type to spawn (researcher, coder, reviewer, writer, analyst).
    ///
    /// When omitted, runtime creates a temporary sub-agent from inline config.
    #[serde(default)]
    pub agent: Option<String>,

    /// Task description for single spawn, or transient fallback task for batch spawn.
    ///
    /// Required for single spawn. Optional when per-worker tasks are provided.
    #[serde(default)]
    pub task: Option<String>,

    /// Transient per-instance task list for batch spawn.
    ///
    /// Tasks are assigned in worker order.
    #[serde(default)]
    pub tasks: Option<Vec<String>>,

    /// If true, wait for completion. If false (default), run concurrently.
    #[serde(default)]
    pub wait: bool,

    /// Timeout in seconds. If omitted, uses sub-agent manager default timeout.
    #[serde(default)]
    pub timeout_secs: Option<u64>,

    /// Optional model override for this spawn (e.g., "minimax/coding-plan").
    #[serde(default)]
    pub model: Option<String>,

    /// Optional provider selector paired with model (e.g., "openai-codex").
    #[serde(default)]
    pub provider: Option<String>,

    /// Optional parent run ID (runtime-injected, internal use).
    #[serde(default)]
    pub parent_run_id: Option<String>,

    /// Optional trace session ID (runtime-injected, internal use).
    #[serde(default)]
    pub trace_session_id: Option<String>,

    /// Optional trace scope ID (runtime-injected, internal use).
    #[serde(default)]
    pub trace_scope_id: Option<String>,

    /// Optional name for temporary sub-agent creation.
    #[serde(default)]
    pub inline_name: Option<String>,

    /// Optional system prompt for temporary sub-agent creation.
    #[serde(default)]
    pub inline_system_prompt: Option<String>,

    /// Optional allowlist for temporary sub-agent tools.
    #[serde(default)]
    pub inline_allowed_tools: Option<Vec<String>>,

    /// Optional max iterations override for temporary sub-agent creation.
    #[serde(default)]
    pub inline_max_iterations: Option<u32>,

    /// Optional list-based worker specs for unified single/multi spawn.
    ///
    /// When provided, this tool enters batch mode and spawns one or more workers.
    #[serde(default)]
    pub workers: Option<Vec<BatchSubagentSpec>>,

    /// If true, validate and preview capability warnings/blockers without executing.
    #[serde(default)]
    pub preview: bool,

    /// Approval ID returned by preview when warnings require explicit confirmation.
    #[serde(default)]
    pub approval_id: Option<String>,
}
