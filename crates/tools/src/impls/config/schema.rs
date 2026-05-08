use serde_json::{Value, json};

pub(crate) fn parameters_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "operation": {
                "type": "string",
                "enum": ["get", "show", "list", "set", "reset"],
                "description": "Config operation to perform"
            },
            "config": {
                "type": "object",
                "description": "Full config object (for set)"
            },
            "key": {
                "type": "string",
                "description": "Config field to update (for set)"
            },
            "value": {
                "description": "Value for the config field (for set)"
            }
        },
        "required": ["operation"]
    })
}
