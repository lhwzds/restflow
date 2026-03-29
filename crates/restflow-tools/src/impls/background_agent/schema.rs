use serde_json::{Value, json};

use restflow_traits::store::{
    MANAGE_BACKGROUND_AGENT_OPERATIONS, MANAGE_BACKGROUND_AGENTS_TOOL_DESCRIPTION,
};

use super::types::workers_schema;

pub(super) fn tool_description() -> &'static str {
    MANAGE_BACKGROUND_AGENTS_TOOL_DESCRIPTION
}

pub(super) fn parameters_schema() -> Value {
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
                "description": "Whether to trigger immediate run after convert_session/promote_to_background (default: false)"
            },
            "preview": {
                "type": "boolean",
                "description": "If true, validate capability warnings/blockers without applying changes."
            },
            "confirmation_token": {
                "type": "string",
                "description": "Confirmation token returned by preview when warnings require explicit confirmation."
            },
            "team": {
                "type": "string",
                "description": "Team name for save_team/get_team/delete_team, or run_batch from saved team."
            },
            "save_as_team": {
                "type": "string",
                "description": "Optionally save provided workers as a team during run_batch."
            },
            "workers": workers_schema(),
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
