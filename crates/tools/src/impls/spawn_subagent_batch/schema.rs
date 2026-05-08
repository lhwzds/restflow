use serde_json::{Value, json};

pub(super) fn parameters_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "operation": {
                "type": "string",
                "enum": ["spawn"],
                "default": "spawn",
                "description": "Operation to perform."
            },
            "specs": {
                "type": "array",
                "description": "Batch member specs. Required for spawn.",
                "items": {
                    "type": "object",
                    "properties": {
                        "agent": {
                            "type": "string",
                            "description": "Optional agent ID or name. Omit for a temporary child run."
                        },
                        "count": {
                            "type": "integer",
                            "minimum": 1,
                            "default": 1,
                            "description": "How many child runs to spawn for this spec."
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
                            "description": "Optional temporary child-run name."
                        },
                        "inline_system_prompt": {
                            "type": "string",
                            "description": "Optional temporary child-run system prompt."
                        },
                        "inline_allowed_tools": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional temporary child-run tool allowlist."
                        },
                        "inline_max_iterations": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "Optional temporary child-run max iterations."
                        }
                    }
                }
            },
            "task": {
                "type": "string",
                "description": "Default task for specs that do not define per-spec 'task' or 'tasks'."
            },
            "tasks": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Per-instance task list for this spawn. Tasks are assigned in spec order."
            },
            "wait": {
                "type": "boolean",
                "default": false,
                "description": "If true, wait for all spawned tasks."
            },
            "timeout_secs": {
                "type": "integer",
                "minimum": 0,
                "description": "Wait timeout and fallback child-run timeout (seconds). Use 0 for no wait timeout."
            },
            "parent_run_id": {
                "type": "string",
                "description": "Optional parent run ID for context propagation (runtime-injected)."
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
