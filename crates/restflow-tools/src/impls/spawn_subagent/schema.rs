use serde_json::{Value, json};

use restflow_traits::{DEFAULT_SUBAGENT_TIMEOUT_SECS, subagent::SubagentDefSummary};

pub(super) fn parameters_schema(available: &[SubagentDefSummary]) -> Value {
    let agent_property = if available.is_empty() {
        json!({
            "type": "string",
            "description": "Optional agent ID or name. Omit to create a temporary sub-agent. Call list_subagents to discover available agents."
        })
    } else {
        let enum_values: Vec<String> = available.iter().map(|agent| agent.id.clone()).collect();
        let enum_labels: Vec<String> = available
            .iter()
            .map(|agent| format!("{} ({})", agent.name, agent.id))
            .collect();
        json!({
            "type": "string",
            "enum": enum_values,
            "x-enumNames": enum_labels,
            "description": "Optional agent ID. You can also pass agent name at runtime. Omit to create a temporary sub-agent."
        })
    };

    json!({
        "type": "object",
        "properties": {
            "operation": {
                "type": "string",
                "enum": ["spawn", "save_team", "list_teams", "get_team", "delete_team"],
                "default": "spawn",
                "description": "Operation to perform. Use team management operations to save/list/read/delete teams without spawning."
            },
            "agent": agent_property,
            "task": {
                "type": "string",
                "description": "Detailed task description for single spawn, or transient fallback task for batch worker specs. Required for single spawn."
            },
            "tasks": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Transient per-instance task list for batch/team spawn. Tasks are assigned in worker order and are never persisted in saved teams."
            },
            "wait": {
                "type": "boolean",
                "default": false,
                "description": "If true, wait for completion. Applies to spawn only."
            },
            "timeout_secs": {
                "type": "integer",
                "default": DEFAULT_SUBAGENT_TIMEOUT_SECS,
                "description": format!(
                    "Timeout in seconds for single spawn or batch spawn (default: {})",
                    DEFAULT_SUBAGENT_TIMEOUT_SECS
                )
            },
            "model": {
                "type": "string",
                "description": "Optional model override for this sub-agent (e.g., 'minimax/coding-plan')"
            },
            "provider": {
                "type": "string",
                "description": "Provider selector paired with model override (e.g., 'openai-codex'). Required when model is set."
            },
            "parent_execution_id": {
                "type": "string",
                "description": "Optional parent execution ID for context propagation (runtime-injected)"
            },
            "trace_session_id": {
                "type": "string",
                "description": "Optional trace session ID for context propagation (runtime-injected)"
            },
            "trace_scope_id": {
                "type": "string",
                "description": "Optional trace scope ID for context propagation (runtime-injected)"
            },
            "inline_name": {
                "type": "string",
                "description": "Optional temporary sub-agent name when 'agent' is omitted."
            },
            "inline_system_prompt": {
                "type": "string",
                "description": "Optional system prompt for temporary sub-agent when 'agent' is omitted."
            },
            "inline_allowed_tools": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Optional tool allowlist for temporary sub-agent when 'agent' is omitted."
            },
            "inline_max_iterations": {
                "type": "integer",
                "minimum": 1,
                "description": "Optional max iterations for temporary sub-agent when 'agent' is omitted."
            },
            "workers": {
                "type": "array",
                "description": "Optional unified list-based batch specs. Use for batch spawn or save_team.",
                "items": {
                    "type": "object",
                    "properties": {
                        "agent": { "type": "string", "description": "Optional agent ID or name." },
                        "count": { "type": "integer", "minimum": 1, "default": 1, "description": "Number of instances for this worker spec." },
                        "task": { "type": "string", "description": "Optional transient per-worker task override." },
                        "tasks": { "type": "array", "items": { "type": "string" }, "description": "Optional transient per-instance task list for distinct prompts." },
                        "timeout_secs": { "type": "integer", "minimum": 0, "description": "Optional per-worker timeout." },
                        "model": { "type": "string", "description": "Optional model override for this worker." },
                        "provider": { "type": "string", "description": "Optional provider paired with model." },
                        "inline_name": { "type": "string", "description": "Optional temporary sub-agent name." },
                        "inline_system_prompt": { "type": "string", "description": "Optional temporary sub-agent system prompt." },
                        "inline_allowed_tools": { "type": "array", "items": { "type": "string" }, "description": "Optional temporary sub-agent tool allowlist." },
                        "inline_max_iterations": { "type": "integer", "minimum": 1, "description": "Optional temporary sub-agent max iterations." }
                    }
                }
            },
            "team": {
                "type": "string",
                "description": "Team name for save_team/get_team/delete_team, or spawn from a saved team."
            },
            "save_as_team": {
                "type": "string",
                "description": "Spawn-only convenience flag to save provided workers as a structural team during spawn. For save-only, use operation='save_team'."
            },
            "preview": {
                "type": "boolean",
                "description": "If true, validate capability warnings/blockers without executing."
            },
            "approval_id": {
                "type": "string",
                "description": "Approval ID returned by preview when warnings require explicit confirmation."
            }
        }
    })
}
