//! Process management tool for AI agents

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

use crate::error::{AiError, Result};
use crate::tools::traits::{Tool, ToolOutput};

/// Convert anyhow::Error to AiError::Tool
fn to_tool_error(e: anyhow::Error) -> AiError {
    AiError::Tool(e.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessSessionInfo {
    pub session_id: String,
    pub command: String,
    pub cwd: Option<String>,
    pub started_at: i64,
    pub status: String,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessPollResult {
    pub session_id: String,
    pub output: String,
    pub status: String,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessLog {
    pub session_id: String,
    pub output: String,
    pub offset: usize,
    pub limit: usize,
    pub total: usize,
    pub truncated: bool,
}

pub trait ProcessManager: Send + Sync {
    fn spawn(&self, command: String, cwd: Option<String>) -> anyhow::Result<String>;
    fn poll(&self, session_id: &str) -> anyhow::Result<ProcessPollResult>;
    fn write(&self, session_id: &str, data: &str) -> anyhow::Result<()>;
    fn kill(&self, session_id: &str) -> anyhow::Result<()>;
    fn list(&self) -> anyhow::Result<Vec<ProcessSessionInfo>>;
    fn log(&self, session_id: &str, offset: usize, limit: usize) -> anyhow::Result<ProcessLog>;
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum ProcessAction {
    Spawn {
        command: String,
        cwd: Option<String>,
        /// Reserved for future background mode support
        #[serde(rename = "yield")]
        #[allow(dead_code)]
        yield_mode: Option<bool>,
    },
    Poll {
        session_id: String,
    },
    Write {
        session_id: String,
        data: String,
    },
    Kill {
        session_id: String,
    },
    List,
    Log {
        session_id: String,
        offset: Option<usize>,
        limit: Option<usize>,
    },
}

/// Process management tool
pub struct ProcessTool {
    manager: Arc<dyn ProcessManager>,
}

impl ProcessTool {
    pub fn new(manager: Arc<dyn ProcessManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for ProcessTool {
    fn name(&self) -> &str {
        "process"
    }

    fn description(&self) -> &str {
        "Manage process sessions: spawn commands, poll status, write stdin, read logs, list, and kill."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "Action to perform: spawn, poll, write, kill, list, log",
                    "enum": ["spawn", "poll", "write", "kill", "list", "log"]
                },
                "command": { "type": "string", "description": "Command to execute" },
                "cwd": { "type": "string", "description": "Working directory" },
                "yield": { "type": "boolean", "description": "Run in background" },
                "session_id": { "type": "string", "description": "Process session id" },
                "data": { "type": "string", "description": "Input to write to the process" },
                "offset": { "type": "integer", "description": "Log offset" },
                "limit": { "type": "integer", "description": "Log limit" }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let action: ProcessAction = serde_json::from_value(input)?;

        match action {
            ProcessAction::Spawn { command, cwd, .. } => {
                let session_id = self.manager.spawn(command, cwd).map_err(to_tool_error)?;
                Ok(ToolOutput::success(json!({"session_id": session_id})))
            }
            ProcessAction::Poll { session_id } => {
                let result = self.manager.poll(&session_id).map_err(to_tool_error)?;
                Ok(ToolOutput::success(serde_json::to_value(result)?))
            }
            ProcessAction::Write { session_id, data } => {
                self.manager
                    .write(&session_id, &data)
                    .map_err(to_tool_error)?;
                Ok(ToolOutput::success(json!({"session_id": session_id})))
            }
            ProcessAction::Kill { session_id } => {
                self.manager.kill(&session_id).map_err(to_tool_error)?;
                Ok(ToolOutput::success(json!({"session_id": session_id})))
            }
            ProcessAction::List => {
                let sessions = self.manager.list().map_err(to_tool_error)?;
                Ok(ToolOutput::success(serde_json::to_value(sessions)?))
            }
            ProcessAction::Log {
                session_id,
                offset,
                limit,
            } => {
                let offset = offset.unwrap_or(0);
                let limit = limit.unwrap_or(10_000);
                let log = self
                    .manager
                    .log(&session_id, offset, limit)
                    .map_err(to_tool_error)?;
                Ok(ToolOutput::success(serde_json::to_value(log)?))
            }
        }
    }
}
