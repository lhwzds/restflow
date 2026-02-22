//! Terminal session management tool.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::Result;
use crate::{Tool, ToolOutput};
use restflow_ai::tools::store_traits::TerminalStore;

pub struct TerminalTool {
    store: Arc<dyn TerminalStore>,
}

impl TerminalTool {
    pub fn new(store: Arc<dyn TerminalStore>) -> Self {
        Self { store }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum TerminalOperation {
    Create {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        working_directory: Option<String>,
        #[serde(default)]
        startup_command: Option<String>,
    },
    List,
    SendInput {
        session_id: String,
        data: String,
    },
    ReadOutput {
        session_id: String,
    },
    Close {
        session_id: String,
    },
}

#[async_trait]
impl Tool for TerminalTool {
    fn name(&self) -> &str {
        "manage_terminal"
    }

    fn description(&self) -> &str {
        "Manage persistent terminal session metadata. Interactive PTY streaming is not available in this runtime."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["create", "list", "send_input", "read_output", "close"]
                },
                "session_id": { "type": "string" },
                "name": { "type": "string" },
                "working_directory": { "type": "string" },
                "startup_command": { "type": "string" },
                "data": { "type": "string" }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let operation: TerminalOperation = serde_json::from_value(input)?;
        match operation {
            TerminalOperation::Create {
                name,
                working_directory,
                startup_command,
            } => {
                let result = self.store.create_session(
                    name.as_deref(),
                    working_directory.as_deref(),
                    startup_command.as_deref(),
                )?;
                Ok(ToolOutput::success(result))
            }
            TerminalOperation::List => {
                let result = self.store.list_sessions()?;
                Ok(ToolOutput::success(result))
            }
            TerminalOperation::SendInput { session_id, data } => {
                let result = self.store.send_input(&session_id, &data)?;
                Ok(ToolOutput::success(result))
            }
            TerminalOperation::ReadOutput { session_id } => {
                let result = self.store.read_output(&session_id)?;
                Ok(ToolOutput::success(result))
            }
            TerminalOperation::Close { session_id } => {
                let result = self.store.close_session(&session_id)?;
                Ok(ToolOutput::success(result))
            }
        }
    }
}
