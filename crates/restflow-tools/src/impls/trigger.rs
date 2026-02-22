//! Trigger management tool for creating, listing, and disabling workflow triggers.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::Result;
use crate::{Tool, ToolOutput};
use restflow_ai::tools::store_traits::TriggerStore;

pub struct TriggerTool {
    store: Arc<dyn TriggerStore>,
}

impl TriggerTool {
    pub fn new(store: Arc<dyn TriggerStore>) -> Self {
        Self { store }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum TriggerOperation {
    Create {
        workflow_id: String,
        trigger_config: Value,
        #[serde(default)]
        id: Option<String>,
    },
    List,
    Delete {
        id: String,
    },
    Enable {
        workflow_id: String,
        trigger_config: Value,
        #[serde(default)]
        id: Option<String>,
    },
    Disable {
        id: String,
    },
}

#[async_trait]
impl Tool for TriggerTool {
    fn name(&self) -> &str {
        "manage_triggers"
    }

    fn description(&self) -> &str {
        "Create/list/enable/disable workflow triggers."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["create", "list", "delete", "enable", "disable"]
                },
                "id": { "type": "string" },
                "workflow_id": { "type": "string" },
                "trigger_config": {
                    "type": "object",
                    "description": "TriggerConfig payload with a `type` discriminator (manual/webhook/schedule)."
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let operation: TriggerOperation = serde_json::from_value(input)?;
        match operation {
            TriggerOperation::Create {
                workflow_id,
                trigger_config,
                id,
            }
            | TriggerOperation::Enable {
                workflow_id,
                trigger_config,
                id,
            } => {
                let result =
                    self.store
                        .create_trigger(&workflow_id, trigger_config, id.as_deref())?;
                Ok(ToolOutput::success(result))
            }
            TriggerOperation::List => {
                let result = self.store.list_triggers()?;
                Ok(ToolOutput::success(result))
            }
            TriggerOperation::Delete { id } | TriggerOperation::Disable { id } => {
                let result = self.store.delete_trigger(&id)?;
                Ok(ToolOutput::success(result))
            }
        }
    }
}
