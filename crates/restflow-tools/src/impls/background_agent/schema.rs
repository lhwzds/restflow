use serde_json::{Value, json};

use restflow_traits::store::{MANAGE_TASK_OPERATIONS, MANAGE_TASKS_TOOL_DESCRIPTION};

use super::types::workers_schema;

const LEGACY_BACKGROUND_AGENT_TOOL_DESCRIPTION: &str = "Compatibility alias for manage_tasks. Manage tasks. CRITICAL: create only defines the task, to immediately execute use 'run' operation. Operations: create (define new task, does NOT run), convert_session (convert an existing chat session into a task), promote_to_background (promote current interactive session into a task), run_batch (create multiple tasks from workers/team and optionally trigger run_now), save_team/list_teams/get_team/delete_team (manage reusable batch templates), run (trigger now), pause/resume (toggle schedule), stop (interrupt current/future execution without deleting the definition), delete (remove definition; auto-created bound chat session is archived when safe), list (browse tasks), progress (execution history), send_message/list_messages (interact with running tasks), list_deliverables (read typed outputs), list_traces/read_trace (diagnose execution traces).";

pub(super) fn tool_description() -> &'static str {
    MANAGE_TASKS_TOOL_DESCRIPTION
}

pub(super) fn legacy_tool_description() -> &'static str {
    LEGACY_BACKGROUND_AGENT_TOOL_DESCRIPTION
}

pub(super) fn parameters_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "operation": {
                "type": "string",
                "enum": MANAGE_TASK_OPERATIONS,
                "description": "Task operation to perform"
            },
            "id": {
                "type": "string"
            },
            "name": {
                "type": "string",
                "description": "Task name (for create/update)"
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
                "description": "Task description (for update)"
            },
            "chat_session_id": {
                "type": "string",
                "description": "Optional bound chat session ID (for create/update). If omitted on create, backend creates one."
            },
            "schedule": {
                "type": "object",
                "description": "Task schedule object (required for create, optional for update/convert_session/promote_to_background)"
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
                "description": "Optional input for the task (for create/update)"
            },
            "inputs": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Optional per-instance input list for run_batch. Inputs are assigned in worker order and are never persisted in saved teams."
            },
            "input_template": {
                "type": "string",
                "description": "Optional runtime template for task input (for create/update)"
            },
            "memory_scope": {
                "type": "string",
                "enum": ["shared_agent", "per_task"],
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
            "approval_id": {
                "type": "string",
                "description": "Approval ID returned by preview when warnings require explicit confirmation."
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
        "required": ["operation"],
        "allOf": [
            {
                "if": {
                    "properties": {
                        "operation": { "const": "create" }
                    },
                    "required": ["operation"]
                },
                "then": {
                    "required": ["operation", "name", "agent_id", "schedule"]
                }
            },
            {
                "if": {
                    "properties": {
                        "operation": { "const": "convert_session" }
                    },
                    "required": ["operation"]
                },
                "then": {
                    "required": ["operation", "session_id"]
                }
            },
            {
                "if": {
                    "properties": {
                        "operation": { "const": "promote_to_background" }
                    },
                    "required": ["operation"]
                },
                "then": {
                    "required": ["operation"]
                }
            }
        ]
    })
}
