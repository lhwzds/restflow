use serde_json::{Value, json};

pub(super) fn parameters_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "operation": {
                "type": "string",
                "enum": ["spawn", "save_team", "list_teams", "get_team", "delete_team"],
                "default": "spawn",
                "description": "Operation to perform."
            },
            "team": {
                "type": "string",
                "description": "Team name for save_team/get_team/delete_team, or spawn from saved team."
            },
            "specs": {
                "type": "array",
                "description": "Batch member specs. Required for save_team, optional for spawn when team is provided.",
                "items": {
                    "type": "object",
                    "properties": {
                        "agent": {
                            "type": "string",
                            "description": "Optional agent ID or name. Omit for temporary sub-agent."
                        },
                        "count": {
                            "type": "integer",
                            "minimum": 1,
                            "default": 1,
                            "description": "How many sub-agents to spawn for this spec."
                        },
                        "task": {
                            "type": "string",
                            "description": "Optional per-spec task override."
                        },
                        "tasks": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional per-instance task list. When set, each spawned instance uses one prompt from this list."
                        },
                        "timeout_secs": {
                            "type": "integer",
                            "minimum": 0,
                            "description": "Optional per-spec timeout in seconds."
                        },
                        "model": {
                            "type": "string",
                            "description": "Optional model override."
                        },
                        "provider": {
                            "type": "string",
                            "description": "Optional provider paired with model."
                        },
                        "inline_name": {
                            "type": "string",
                            "description": "Optional temporary sub-agent name."
                        },
                        "inline_system_prompt": {
                            "type": "string",
                            "description": "Optional temporary sub-agent system prompt."
                        },
                        "inline_allowed_tools": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional temporary sub-agent tool allowlist."
                        },
                        "inline_max_iterations": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "Optional temporary sub-agent max iterations."
                        }
                    }
                }
            },
            "task": {
                "type": "string",
                "description": "Transient default task for specs that do not define per-spec 'task' or 'tasks'. Saved teams never persist this field."
            },
            "tasks": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Transient per-instance task list for this spawn. Tasks are assigned in spec order and are never persisted in saved teams."
            },
            "wait": {
                "type": "boolean",
                "default": false,
                "description": "If true, wait for all spawned tasks."
            },
            "timeout_secs": {
                "type": "integer",
                "minimum": 0,
                "description": "Wait timeout and fallback sub-agent timeout (seconds). Use 0 for no wait timeout."
            },
            "save_as_team": {
                "type": "string",
                "description": "Optionally save provided specs as a structural team during spawn. Prompt fields are not persisted."
            },
            "parent_execution_id": {
                "type": "string",
                "description": "Optional parent execution ID for context propagation (runtime-injected)."
            },
            "trace_session_id": {
                "type": "string",
                "description": "Optional trace session ID for context propagation (runtime-injected)."
            },
            "trace_scope_id": {
                "type": "string",
                "description": "Optional trace scope ID for context propagation (runtime-injected)."
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
