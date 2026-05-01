//! Process management tool for AI agents

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::Result;
use crate::check_security;
use crate::security::SecurityGate;
use crate::{Tool, ToolAction, ToolOutput};
use restflow_traits::store::ProcessManager;

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
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: Option<String>,
    task_id: Option<String>,
}

impl ProcessTool {
    pub fn new(manager: Arc<dyn ProcessManager>) -> Self {
        Self {
            manager,
            security_gate: None,
            agent_id: None,
            task_id: None,
        }
    }

    pub fn with_security(
        mut self,
        security_gate: Arc<dyn SecurityGate>,
        agent_id: impl Into<String>,
        task_id: impl Into<String>,
    ) -> Self {
        self.security_gate = Some(security_gate);
        self.agent_id = Some(agent_id.into());
        self.task_id = Some(task_id.into());
        self
    }

    async fn check_action_allowed(
        &self,
        operation: &str,
        target: String,
        summary: String,
    ) -> Result<Option<String>> {
        let action = ToolAction {
            tool_name: self.name().to_string(),
            operation: operation.to_string(),
            target,
            summary,
        };

        check_security(
            self.security_gate.as_deref(),
            action,
            self.agent_id.as_deref(),
            self.task_id.as_deref(),
        )
        .await
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
            ProcessAction::Spawn { command, cwd, .. } => {
                if let Some(message) = self
                    .check_action_allowed(
                        "spawn",
                        command.clone(),
                        format!("Spawn process command: {}", command),
                    )
                    .await?
                {
                    return Ok(ToolOutput::error(message));
                }

                match self.manager.spawn(command, cwd) {
                    Ok(session_id) => Ok(ToolOutput::success(json!({"session_id": session_id}))),
                    Err(e) => Ok(ToolOutput::error(format!(
                        "Failed to spawn process: {}. Check that the command exists and the working directory is valid.",
                        e
                    ))),
                }
            }
            ProcessAction::Poll { session_id } => {
                if let Some(message) = self
                    .check_action_allowed(
                        "poll",
                        session_id.clone(),
                        format!("Poll process session {}", session_id),
                    )
                    .await?
                {
                    return Ok(ToolOutput::error(message));
                }

                match self.manager.poll(&session_id) {
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
                }
            }
            ProcessAction::Write { session_id, data } => {
                if let Some(message) = self
                    .check_action_allowed(
                        "write",
                        session_id.clone(),
                        format!("Write stdin to process session {}", session_id),
                    )
                    .await?
                {
                    return Ok(ToolOutput::error(message));
                }

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
            ProcessAction::Kill { session_id } => {
                if let Some(message) = self
                    .check_action_allowed(
                        "kill",
                        session_id.clone(),
                        format!("Kill process session {}", session_id),
                    )
                    .await?
                {
                    return Ok(ToolOutput::error(message));
                }

                match self.manager.kill(&session_id) {
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
                }
            }
            ProcessAction::List => {
                if let Some(message) = self
                    .check_action_allowed(
                        "list",
                        "process_sessions".to_string(),
                        "List process sessions".to_string(),
                    )
                    .await?
                {
                    return Ok(ToolOutput::error(message));
                }

                match self.manager.list() {
                    Ok(sessions) => Ok(ToolOutput::success(serde_json::to_value(sessions)?)),
                    Err(e) => Ok(ToolOutput::error(format!(
                        "Failed to list process sessions: {}",
                        e
                    ))),
                }
            }
            ProcessAction::Log {
                session_id,
                offset,
                limit,
            } => {
                if let Some(message) = self
                    .check_action_allowed(
                        "log",
                        session_id.clone(),
                        format!("Read process logs for session {}", session_id),
                    )
                    .await?
                {
                    return Ok(ToolOutput::error(message));
                }

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
    use async_trait::async_trait;
    use restflow_traits::security::{SecurityDecision, ToolAction};
    use restflow_traits::store::{ProcessLog, ProcessPollResult, ProcessSessionInfo};
    use serde_json::json;
    use std::sync::Mutex;

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

    #[derive(Default)]
    struct ProcessCallCounts {
        spawn: usize,
        poll: usize,
        write: usize,
        kill: usize,
        list: usize,
        log: usize,
    }

    struct CountingProcessManager {
        calls: Arc<Mutex<ProcessCallCounts>>,
    }

    impl CountingProcessManager {
        fn new(calls: Arc<Mutex<ProcessCallCounts>>) -> Self {
            Self { calls }
        }
    }

    impl ProcessManager for CountingProcessManager {
        fn spawn(&self, _command: String, _cwd: Option<String>) -> anyhow::Result<String> {
            self.calls.lock().expect("calls lock poisoned").spawn += 1;
            Ok("session-1".to_string())
        }

        fn poll(&self, session_id: &str) -> anyhow::Result<ProcessPollResult> {
            self.calls.lock().expect("calls lock poisoned").poll += 1;
            Ok(ProcessPollResult {
                session_id: session_id.to_string(),
                output: String::new(),
                status: "running".to_string(),
                exit_code: None,
            })
        }

        fn write(&self, _session_id: &str, _data: &str) -> anyhow::Result<()> {
            self.calls.lock().expect("calls lock poisoned").write += 1;
            Ok(())
        }

        fn kill(&self, _session_id: &str) -> anyhow::Result<()> {
            self.calls.lock().expect("calls lock poisoned").kill += 1;
            Ok(())
        }

        fn list(&self) -> anyhow::Result<Vec<ProcessSessionInfo>> {
            self.calls.lock().expect("calls lock poisoned").list += 1;
            Ok(Vec::new())
        }

        fn log(&self, session_id: &str, offset: usize, limit: usize) -> anyhow::Result<ProcessLog> {
            self.calls.lock().expect("calls lock poisoned").log += 1;
            Ok(ProcessLog {
                session_id: session_id.to_string(),
                output: String::new(),
                offset,
                limit,
                total: 0,
                truncated: false,
            })
        }
    }

    enum GateMode {
        Allow,
        Deny(String),
    }

    struct MockSecurityGate {
        mode: GateMode,
        actions: Arc<Mutex<Vec<ToolAction>>>,
    }

    impl MockSecurityGate {
        fn allow(actions: Arc<Mutex<Vec<ToolAction>>>) -> Self {
            Self {
                mode: GateMode::Allow,
                actions,
            }
        }

        fn deny(reason: impl Into<String>, actions: Arc<Mutex<Vec<ToolAction>>>) -> Self {
            Self {
                mode: GateMode::Deny(reason.into()),
                actions,
            }
        }
    }

    #[async_trait]
    impl SecurityGate for MockSecurityGate {
        async fn check_command(
            &self,
            _command: &str,
            _task_id: &str,
            _agent_id: &str,
            _workdir: Option<&str>,
        ) -> restflow_traits::error::Result<SecurityDecision> {
            Ok(SecurityDecision::allowed(None))
        }

        async fn check_tool_action(
            &self,
            action: &ToolAction,
            _agent_id: Option<&str>,
            _task_id: Option<&str>,
        ) -> restflow_traits::error::Result<SecurityDecision> {
            self.actions
                .lock()
                .expect("actions lock poisoned")
                .push(action.clone());

            match &self.mode {
                GateMode::Allow => Ok(SecurityDecision::allowed(None)),
                GateMode::Deny(reason) => Ok(SecurityDecision::blocked(Some(reason.clone()))),
            }
        }
    }

    #[tokio::test]
    async fn process_tool_defaults_to_open_when_security_gate_is_absent() {
        let calls = Arc::new(Mutex::new(ProcessCallCounts::default()));
        let manager = Arc::new(CountingProcessManager::new(calls.clone()));
        let tool = ProcessTool::new(manager);

        let spawn = tool
            .execute(json!({"action": "spawn", "command": "echo test"}))
            .await
            .unwrap();
        let write = tool
            .execute(json!({"action": "write", "session_id": "session-1", "data": "input"}))
            .await
            .unwrap();
        let poll = tool
            .execute(json!({"action": "poll", "session_id": "session-1"}))
            .await
            .unwrap();
        let kill = tool
            .execute(json!({"action": "kill", "session_id": "session-1"}))
            .await
            .unwrap();
        let list = tool.execute(json!({"action": "list"})).await.unwrap();
        let log = tool
            .execute(json!({"action": "log", "session_id": "session-1"}))
            .await
            .unwrap();

        assert!(spawn.success);
        assert!(write.success);
        assert!(poll.success);
        assert!(kill.success);
        assert!(list.success);
        assert!(log.success);

        let calls = calls.lock().expect("calls lock poisoned");
        assert_eq!(calls.spawn, 1);
        assert_eq!(calls.poll, 1);
        assert_eq!(calls.write, 1);
        assert_eq!(calls.kill, 1);
        assert_eq!(calls.list, 1);
        assert_eq!(calls.log, 1);
    }

    #[tokio::test]
    async fn process_tool_applies_security_gate_to_all_operations() {
        let calls = Arc::new(Mutex::new(ProcessCallCounts::default()));
        let manager = Arc::new(CountingProcessManager::new(calls.clone()));
        let actions = Arc::new(Mutex::new(Vec::new()));
        let gate = Arc::new(MockSecurityGate::deny(
            "process operation blocked",
            actions.clone(),
        ));
        let tool = ProcessTool::new(manager).with_security(gate, "agent-1", "task-1");

        let spawn = tool
            .execute(json!({"action": "spawn", "command": "echo test"}))
            .await
            .unwrap();
        let write = tool
            .execute(json!({"action": "write", "session_id": "session-1", "data": "input"}))
            .await
            .unwrap();
        let poll = tool
            .execute(json!({"action": "poll", "session_id": "session-1"}))
            .await
            .unwrap();
        let kill = tool
            .execute(json!({"action": "kill", "session_id": "session-1"}))
            .await
            .unwrap();
        let list = tool.execute(json!({"action": "list"})).await.unwrap();
        let log = tool
            .execute(json!({"action": "log", "session_id": "session-1"}))
            .await
            .unwrap();

        assert!(!spawn.success);
        assert!(!write.success);
        assert!(!poll.success);
        assert!(!kill.success);
        assert!(!list.success);
        assert!(!log.success);
        assert_eq!(
            spawn.error.as_deref(),
            Some("Action blocked: process operation blocked")
        );
        assert_eq!(
            write.error.as_deref(),
            Some("Action blocked: process operation blocked")
        );
        assert_eq!(
            poll.error.as_deref(),
            Some("Action blocked: process operation blocked")
        );
        assert_eq!(
            kill.error.as_deref(),
            Some("Action blocked: process operation blocked")
        );
        assert_eq!(
            list.error.as_deref(),
            Some("Action blocked: process operation blocked")
        );
        assert_eq!(
            log.error.as_deref(),
            Some("Action blocked: process operation blocked")
        );

        let calls = calls.lock().expect("calls lock poisoned");
        assert_eq!(calls.spawn, 0);
        assert_eq!(calls.poll, 0);
        assert_eq!(calls.write, 0);
        assert_eq!(calls.kill, 0);
        assert_eq!(calls.list, 0);
        assert_eq!(calls.log, 0);
        drop(calls);

        let actions = actions.lock().expect("actions lock poisoned");
        let operations: Vec<&str> = actions
            .iter()
            .map(|action| action.operation.as_str())
            .collect();
        assert_eq!(
            operations,
            vec!["spawn", "write", "poll", "kill", "list", "log"]
        );
    }

    #[tokio::test]
    async fn process_tool_executes_when_security_gate_allows_actions() {
        let calls = Arc::new(Mutex::new(ProcessCallCounts::default()));
        let manager = Arc::new(CountingProcessManager::new(calls.clone()));
        let actions = Arc::new(Mutex::new(Vec::new()));
        let gate = Arc::new(MockSecurityGate::allow(actions.clone()));
        let tool = ProcessTool::new(manager).with_security(gate, "agent-1", "task-1");

        let output = tool
            .execute(json!({"action": "spawn", "command": "echo allowed"}))
            .await
            .unwrap();

        assert!(output.success);

        let calls = calls.lock().expect("calls lock poisoned");
        assert_eq!(calls.spawn, 1);
        drop(calls);

        let actions = actions.lock().expect("actions lock poisoned");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].operation, "spawn");
    }
}
