//! Browser automation tool for AI agents.
//!
//! This tool provides session-based Chromium automation with two interaction
//! styles:
//! - Direct JS/TS code execution (AI can write browser code)
//! - Structured action plans for common web automation steps

use async_trait::async_trait;
use restflow_browser::{
    BrowserAction, BrowserService, NewSessionRequest, RunActionsRequest, RunScriptRequest,
    ScriptLanguage, ScriptRuntime,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::Result;
use crate::{Tool, ToolErrorCategory, ToolOutput};

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum BrowserInput {
    ProbeRuntime,
    NewSession {
        #[serde(default)]
        headless: Option<bool>,
    },
    ListSessions,
    CloseSession {
        session_id: String,
    },
    RunScript {
        session_id: String,
        code: String,
        #[serde(default)]
        language: Option<ScriptLanguage>,
        #[serde(default)]
        runtime: Option<ScriptRuntime>,
        #[serde(default)]
        timeout_secs: Option<u64>,
        #[serde(default)]
        cwd: Option<String>,
    },
    RunActions {
        session_id: String,
        actions: Vec<BrowserAction>,
        #[serde(default)]
        runtime: Option<ScriptRuntime>,
        #[serde(default)]
        timeout_secs: Option<u64>,
        #[serde(default)]
        cwd: Option<String>,
    },
}

/// Browser automation tool.
pub struct BrowserTool {
    service: Arc<BrowserService>,
}

impl BrowserTool {
    pub fn new() -> Result<Self> {
        Ok(Self {
            service: Arc::new(BrowserService::new()?),
        })
    }

    pub fn with_service(service: Arc<BrowserService>) -> Self {
        Self { service }
    }

    fn format_execution_failure(message: String, details: Value) -> ToolOutput {
        let mut output = ToolOutput::non_retryable_error(message, ToolErrorCategory::Execution);
        output.result = details;
        output
    }
}

#[async_trait]
impl Tool for BrowserTool {
    fn name(&self) -> &str {
        "browser"
    }

    fn description(&self) -> &str {
        "Chromium browser automation for AI agents using native CDP. Supports session lifecycle, JS script execution in page context (TS not yet supported), and structured action plans (navigate/click/fill/extract/screenshot)."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "Operation to perform",
                    "enum": [
                        "probe_runtime",
                        "new_session",
                        "list_sessions",
                        "close_session",
                        "run_script",
                        "run_actions"
                    ]
                },
                "session_id": { "type": "string", "description": "Browser session ID" },
                "headless": { "type": "boolean", "description": "Run Chromium in headless mode for new_session" },
                "code": { "type": "string", "description": "JavaScript/TypeScript code for run_script" },
                "language": { "type": "string", "enum": ["js", "ts"], "description": "Script language for run_script" },
                "runtime": { "type": "string", "enum": ["auto", "node"], "description": "Execution runtime" },
                "timeout_secs": { "type": "integer", "description": "Execution timeout in seconds" },
                "cwd": { "type": "string", "description": "Optional working directory" },
                "actions": {
                    "type": "array",
                    "description": "Structured browser action list for run_actions",
                    "items": {
                        "type": "object",
                        "properties": {
                            "type": { "type": "string" }
                        },
                        "required": ["type"]
                    }
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let action: BrowserInput = match serde_json::from_value(input) {
            Ok(value) => value,
            Err(error) => {
                return Ok(ToolOutput::non_retryable_error(
                    format!(
                        "Invalid input: {}. Required: action plus fields for selected operation.",
                        error
                    ),
                    ToolErrorCategory::Config,
                ));
            }
        };

        match action {
            BrowserInput::ProbeRuntime => {
                let probe = self.service.probe_runtime().await?;
                Ok(ToolOutput::success(serde_json::to_value(probe)?))
            }
            BrowserInput::NewSession { headless } => {
                let session = self
                    .service
                    .new_session(NewSessionRequest {
                        headless: headless.unwrap_or(true),
                        ..Default::default()
                    })
                    .await?;
                Ok(ToolOutput::success(serde_json::to_value(session)?))
            }
            BrowserInput::ListSessions => {
                let sessions = self.service.list_sessions().await;
                Ok(ToolOutput::success(serde_json::to_value(sessions)?))
            }
            BrowserInput::CloseSession { session_id } => {
                let closed = self.service.close_session(&session_id).await?;
                Ok(ToolOutput::success(json!({
                    "session_id": session_id,
                    "closed": closed
                })))
            }
            BrowserInput::RunScript {
                session_id,
                code,
                language,
                runtime,
                timeout_secs,
                cwd,
            } => {
                let execution = self
                    .service
                    .run_script(&RunScriptRequest {
                        session_id,
                        code,
                        language: language.unwrap_or_default(),
                        runtime: runtime.unwrap_or_default(),
                        timeout_secs: timeout_secs.unwrap_or(120),
                        cwd,
                    })
                    .await?;

                if execution.exit_code != 0 {
                    let details = serde_json::to_value(&execution)?;
                    return Ok(Self::format_execution_failure(
                        execution.failed_message(),
                        details,
                    ));
                }

                Ok(ToolOutput::success(serde_json::to_value(execution)?))
            }
            BrowserInput::RunActions {
                session_id,
                actions,
                runtime,
                timeout_secs,
                cwd,
            } => {
                let execution = self
                    .service
                    .run_actions(&RunActionsRequest {
                        session_id,
                        actions,
                        runtime: runtime.unwrap_or_default(),
                        timeout_secs: timeout_secs.unwrap_or(120),
                        cwd,
                    })
                    .await?;

                if execution.exit_code != 0 {
                    let details = serde_json::to_value(&execution)?;
                    return Ok(Self::format_execution_failure(
                        execution.failed_message(),
                        details,
                    ));
                }

                Ok(ToolOutput::success(serde_json::to_value(execution)?))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_browser::{BrowserExecutionResult, RuntimeProbe};
    use restflow_browser::{BrowserExecutor, RunActionsRequest, RunScriptRequest};
    use restflow_browser::{BrowserKind, BrowserSession};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::tempdir;

    #[derive(Default)]
    struct MockExecutor {
        script_calls: AtomicUsize,
        action_calls: AtomicUsize,
    }

    #[async_trait]
    impl BrowserExecutor for MockExecutor {
        async fn probe_runtime(&self) -> anyhow::Result<RuntimeProbe> {
            Ok(RuntimeProbe {
                node_available: true,
                node_version: Some("v25.0.0".to_string()),
                node_typescript_available: true,
                playwright_package_available: true,
                chromium_cache_detected: true,
                ready: true,
                notes: vec![],
            })
        }

        async fn run_script(
            &self,
            _session: &BrowserSession,
            _request: &RunScriptRequest,
        ) -> anyhow::Result<BrowserExecutionResult> {
            self.script_calls.fetch_add(1, Ordering::Relaxed);
            Ok(BrowserExecutionResult {
                runtime: "mock".to_string(),
                exit_code: 0,
                duration_ms: 10,
                stdout: String::new(),
                stderr: String::new(),
                payload: Some(json!({"success": true})),
            })
        }

        async fn run_actions(
            &self,
            _session: &BrowserSession,
            _request: &RunActionsRequest,
        ) -> anyhow::Result<BrowserExecutionResult> {
            self.action_calls.fetch_add(1, Ordering::Relaxed);
            Ok(BrowserExecutionResult {
                runtime: "mock".to_string(),
                exit_code: 0,
                duration_ms: 20,
                stdout: String::new(),
                stderr: String::new(),
                payload: Some(json!({"success": true, "result": []})),
            })
        }
    }

    fn test_tool() -> BrowserTool {
        let temp = tempdir().unwrap();
        let service = BrowserService::new_with_executor(
            temp.path().join("browser"),
            Arc::new(MockExecutor::default()),
        )
        .unwrap();
        BrowserTool::with_service(Arc::new(service))
    }

    #[tokio::test]
    async fn new_list_and_close_session_flow() {
        let tool = test_tool();

        let created = tool
            .execute(json!({ "action": "new_session", "headless": true }))
            .await
            .unwrap();
        assert!(created.success);

        let session_id = created
            .result
            .get("id")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();

        let list = tool
            .execute(json!({ "action": "list_sessions" }))
            .await
            .unwrap();
        assert!(list.success);
        assert_eq!(list.result.as_array().map(|v| v.len()), Some(1));

        let closed = tool
            .execute(json!({ "action": "close_session", "session_id": session_id }))
            .await
            .unwrap();
        assert!(closed.success);
        assert_eq!(closed.result["closed"], json!(true));
    }

    #[tokio::test]
    async fn run_script_returns_success() {
        let tool = test_tool();

        let created = tool
            .execute(json!({ "action": "new_session" }))
            .await
            .unwrap();
        let session_id = created.result["id"].as_str().unwrap();

        let output = tool
            .execute(json!({
                "action": "run_script",
                "session_id": session_id,
                "code": "setRestflowResult({ ok: true });",
                "language": "js"
            }))
            .await
            .unwrap();

        assert!(output.success);
        assert_eq!(output.result["runtime"], json!("mock"));
    }

    #[tokio::test]
    async fn probe_runtime_returns_structured_result() {
        let tool = test_tool();

        let output = tool
            .execute(json!({ "action": "probe_runtime" }))
            .await
            .unwrap();

        assert!(output.success);
        assert_eq!(output.result["ready"], json!(true));
    }

    #[tokio::test]
    async fn invalid_input_returns_config_error() {
        let tool = test_tool();

        let output = tool
            .execute(json!({ "action": "run_script" }))
            .await
            .unwrap();
        assert!(!output.success);
        assert_eq!(output.error_category, Some(ToolErrorCategory::Config));
    }

    #[test]
    fn browser_kind_is_chromium_only() {
        let session = BrowserSession {
            id: "s1".to_string(),
            browser: BrowserKind::Chromium,
            headless: true,
            created_at_ms: 0,
            session_dir: "/tmp/s1".to_string(),
            profile_dir: "/tmp/s1/profile".to_string(),
            artifacts_dir: "/tmp/s1/artifacts".to_string(),
        };

        assert_eq!(session.browser, BrowserKind::Chromium);
    }

    #[tokio::test]
    async fn run_actions_requires_valid_session() {
        let tool = test_tool();
        let output = tool
            .execute(json!({
                "action": "run_actions",
                "session_id": "missing",
                "actions": [{ "type": "navigate", "url": "https://example.com" }]
            }))
            .await;

        assert!(output.is_err());
    }
}
