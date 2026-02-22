//! Process management tool for AI agents

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::Result;
use crate::{Tool, ToolOutput};
use restflow_ai::tools::store_traits::{ProcessManager, ProcessSessionInfo, ProcessPollResult, ProcessLog};

fn missing_session_message(session_id: &str) -> String {
    format!(
        "Session '{}' not found. Use action 'list' to see active sessions.",
        session_id
    )
}

fn invalid_session_state_message() -> &'static str {
    "Process session is in an invalid state. The session may have crashed. Use 'list' to check status."
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
        let action: ProcessAction = match serde_json::from_value(input) {
            Ok(action) => action,
            Err(e) => {
                return Ok(ToolOutput::error(format!(
                    "Invalid input: {}. Required: action (spawn|poll|write|kill|list|log).",
                    e
                )));
            }
        };

        match action {
            ProcessAction::Spawn { command, cwd, .. } => match self.manager.spawn(command, cwd) {
                Ok(session_id) => Ok(ToolOutput::success(json!({"session_id": session_id}))),
                Err(e) => Ok(ToolOutput::error(format!(
                    "Failed to spawn process: {}. Check that the command exists and the working directory is valid.",
                    e
                ))),
            },
            ProcessAction::Poll { session_id } => match self.manager.poll(&session_id) {
                Ok(result) => Ok(ToolOutput::success(serde_json::to_value(result)?)),
                Err(e) => {
                    if e.to_string().contains("Session not found") {
                        return Ok(ToolOutput::error(missing_session_message(&session_id)));
                    }
                    Ok(ToolOutput::error(format!(
                        "Failed to poll process session '{}': {}",
                        session_id, e
                    )))
                }
            },
            ProcessAction::Write { session_id, data } => {
                match self.manager.write(&session_id, &data) {
                    Ok(()) => Ok(ToolOutput::success(json!({"session_id": session_id}))),
                    Err(e) => {
                        let error_message = e.to_string();
                        if error_message.contains("Session not found") {
                            return Ok(ToolOutput::error(missing_session_message(&session_id)));
                        }
                        if error_message.contains("lock poisoned") {
                            return Ok(ToolOutput::error(invalid_session_state_message()));
                        }
                        Ok(ToolOutput::error(format!(
                            "Failed to write to process session '{}': {}",
                            session_id, e
                        )))
                    }
                }
            }
            ProcessAction::Kill { session_id } => match self.manager.kill(&session_id) {
                Ok(()) => Ok(ToolOutput::success(json!({"session_id": session_id}))),
                Err(e) => {
                    if e.to_string().contains("Session not found") {
                        return Ok(ToolOutput::error(missing_session_message(&session_id)));
                    }
                    Ok(ToolOutput::error(format!(
                        "Failed to kill process session '{}': {}",
                        session_id, e
                    )))
                }
            },
            ProcessAction::List => match self.manager.list() {
                Ok(sessions) => Ok(ToolOutput::success(serde_json::to_value(sessions)?)),
                Err(e) => Ok(ToolOutput::error(format!(
                    "Failed to list process sessions: {}",
                    e
                ))),
            },
            ProcessAction::Log {
                session_id,
                offset,
                limit,
            } => {
                let offset = offset.unwrap_or(0);
                let limit = limit.unwrap_or(10_000);
                match self.manager.log(&session_id, offset, limit) {
                    Ok(log) => Ok(ToolOutput::success(serde_json::to_value(log)?)),
                    Err(e) => {
                        if e.to_string().contains("Session not found") {
                            return Ok(ToolOutput::error(missing_session_message(&session_id)));
                        }
                        Ok(ToolOutput::error(format!(
                            "Failed to read process logs for session '{}': {}",
                            session_id, e
                        )))
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use serde_json::json;

    struct MockProcessManager;

    impl ProcessManager for MockProcessManager {
        fn spawn(&self, _command: String, _cwd: Option<String>) -> anyhow::Result<String> {
            Ok("session-1".to_string())
        }

        fn poll(&self, _session_id: &str) -> anyhow::Result<ProcessPollResult> {
            Err(anyhow!("Session not found: session-404"))
        }

        fn write(&self, _session_id: &str, _data: &str) -> anyhow::Result<()> {
            Ok(())
        }

        fn kill(&self, _session_id: &str) -> anyhow::Result<()> {
            Ok(())
        }

        fn list(&self) -> anyhow::Result<Vec<ProcessSessionInfo>> {
            Ok(vec![])
        }

        fn log(
            &self,
            _session_id: &str,
            _offset: usize,
            _limit: usize,
        ) -> anyhow::Result<ProcessLog> {
            Err(anyhow!("Session not found: session-404"))
        }
    }

    #[tokio::test]
    async fn process_tool_returns_actionable_error_for_invalid_input() {
        let tool = ProcessTool::new(Arc::new(MockProcessManager));
        let output = tool.execute(json!({"command": "echo test"})).await.unwrap();

        assert!(!output.success);
        assert!(
            output
                .error
                .unwrap_or_default()
                .contains("Required: action (spawn|poll|write|kill|list|log).")
        );
    }

    #[tokio::test]
    async fn process_tool_returns_actionable_error_for_missing_session() {
        let tool = ProcessTool::new(Arc::new(MockProcessManager));
        let output = tool
            .execute(json!({"action": "poll", "session_id": "session-404"}))
            .await
            .unwrap();

        assert!(!output.success);
        assert_eq!(
            output.error.unwrap_or_default(),
            "Session 'session-404' not found. Use action 'list' to see active sessions."
        );
    }
}
