use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

use super::python_backend::{
    ProcessPythonBackend, PythonExecutionBackend, PythonExecutionLimits, PythonExecutionRequest,
    PythonRuntime,
};
use super::traits::{Tool, ToolOutput, check_security};
use crate::ToolAction;
use crate::error::Result;
use crate::security::SecurityGate;

const DEFAULT_TIMEOUT_SECONDS: u64 = 30;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RunPythonInput {
    pub code: String,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    #[serde(default)]
    pub limits: Option<PythonExecutionLimits>,
}

#[derive(Clone)]
pub struct RunPythonTool {
    name: &'static str,
    backend: Arc<dyn PythonExecutionBackend>,
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: Option<String>,
    task_id: Option<String>,
}

impl Default for RunPythonTool {
    fn default() -> Self {
        Self::new()
    }
}

impl RunPythonTool {
    pub fn new() -> Self {
        Self {
            name: "run_python",
            backend: Arc::new(ProcessPythonBackend::monty()),
            security_gate: None,
            agent_id: None,
            task_id: None,
        }
    }

    /// Create a tool with a custom name (used for the "python" alias).
    pub fn with_name(name: &'static str) -> Self {
        Self {
            name,
            backend: Arc::new(ProcessPythonBackend::monty()),
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

    #[cfg(test)]
    fn with_backend(mut self, backend: Arc<dyn PythonExecutionBackend>) -> Self {
        self.backend = backend;
        self
    }
}

/// Backward-compatible alias for `RunPythonTool` registered as "python".
#[derive(Clone)]
pub struct PythonTool {
    inner: RunPythonTool,
}

impl Default for PythonTool {
    fn default() -> Self {
        Self::new()
    }
}

impl PythonTool {
    pub fn new() -> Self {
        Self {
            inner: RunPythonTool::with_name("python"),
        }
    }

    pub fn with_security(
        self,
        security_gate: Arc<dyn SecurityGate>,
        agent_id: impl Into<String>,
        task_id: impl Into<String>,
    ) -> Self {
        Self {
            inner: self.inner.with_security(security_gate, agent_id, task_id),
        }
    }
}

fn python_parameters_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "code": {
                "type": "string",
                "description": "Python code to execute"
            },
            "timeout_seconds": {
                "type": "integer",
                "description": "Execution timeout in seconds (default: 30)"
            },
            "limits": {
                "type": "object",
                "properties": {
                    "max_time_ms": { "type": "integer", "description": "Maximum runtime in milliseconds (enforced)" },
                    "max_memory_mb": { "type": "integer", "description": "Reserved for future support; currently rejected by process backend" },
                    "max_steps": { "type": "integer", "description": "Reserved for future support; currently rejected by process backend" }
                }
            }
        },
        "required": ["code"]
    })
}

async fn execute_python(
    tool: &RunPythonTool,
    input: Value,
) -> Result<ToolOutput> {
    let parsed: RunPythonInput = serde_json::from_value(input)?;
    if let Some(security_gate) = tool.security_gate.as_deref() {
        let action = ToolAction {
            tool_name: tool.name.to_string(),
            operation: "execute".to_string(),
            target: "monty".to_string(),
            summary: "Execute Python code".to_string(),
        };
        if let Some(message) = check_security(
            Some(security_gate),
            action,
            tool.agent_id.as_deref(),
            tool.task_id.as_deref(),
        )
        .await?
        {
            return Ok(ToolOutput::error(message));
        }
    }

    let request = PythonExecutionRequest {
        code: parsed.code,
        timeout_seconds: parsed
            .timeout_seconds
            .unwrap_or(DEFAULT_TIMEOUT_SECONDS)
            .max(1),
        runtime: PythonRuntime::Monty,
        limits: parsed.limits,
    };

    match tool.backend.execute(request).await {
        Ok(output) => Ok(ToolOutput {
            success: output.exit_code == 0 && !output.timed_out,
            result: serde_json::to_value(output).unwrap_or(Value::Null),
            error: None,
            error_category: None,
            retryable: None,
            retry_after_ms: None,
        }),
        Err(err) => Ok(ToolOutput::error(err)),
    }
}

#[async_trait]
impl Tool for RunPythonTool {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        "Execute inline Python code with a Monty-first runtime backend."
    }

    fn parameters_schema(&self) -> Value {
        python_parameters_schema()
    }

    fn supports_parallel(&self) -> bool {
        false
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        execute_python(self, input).await
    }
}

#[async_trait]
impl Tool for PythonTool {
    fn name(&self) -> &str {
        self.inner.name
    }

    fn description(&self) -> &str {
        "Alias of run_python for backward compatibility."
    }

    fn parameters_schema(&self) -> Value {
        python_parameters_schema()
    }

    fn supports_parallel(&self) -> bool {
        false
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        execute_python(&self.inner, input).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    #[derive(Clone)]
    struct MockBackend {
        fail: bool,
    }

    #[async_trait]
    impl PythonExecutionBackend for MockBackend {
        async fn execute(
            &self,
            request: PythonExecutionRequest,
        ) -> std::result::Result<super::super::python_backend::PythonExecutionResult, String>
        {
            if self.fail {
                return Err("monty backend failure".to_string());
            }
            Ok(super::super::python_backend::PythonExecutionResult {
                stdout: request.code,
                stderr: String::new(),
                exit_code: 0,
                runtime: "monty".to_string(),
                timed_out: false,
                limits: request.limits,
            })
        }
    }

    #[tokio::test]
    async fn run_python_success_path() {
        let tool = RunPythonTool::new().with_backend(Arc::new(MockBackend { fail: false }));
        let output = tool
            .execute(json!({
                "code": "print('ok')",
                "timeout_seconds": 2
            }))
            .await
            .expect("tool execute should succeed");
        assert!(output.success);
        assert_eq!(
            output
                .result
                .get("runtime")
                .and_then(|v| v.as_str()),
            Some("monty")
        );
    }

    #[tokio::test]
    async fn run_python_backend_failure() {
        let tool = RunPythonTool::new().with_backend(Arc::new(MockBackend { fail: true }));
        let output = tool
            .execute(json!({
                "code": "print('fail')"
            }))
            .await
            .expect("tool execute should return output");
        assert!(!output.success);
    }

    #[tokio::test]
    async fn rejects_unsupported_limits() {
        let tool = RunPythonTool::new();
        let output = tool
            .execute(json!({
                "code": "print('x')",
                "limits": {
                    "max_memory_mb": 128
                }
            }))
            .await
            .expect("tool execute should return output");
        assert!(!output.success);
        let error = output.error.unwrap_or_default();
        assert!(error.contains("max_memory_mb"));
    }
}
