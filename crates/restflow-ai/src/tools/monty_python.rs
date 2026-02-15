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
    pub runtime: Option<PythonRuntime>,
    #[serde(default)]
    pub limits: Option<PythonExecutionLimits>,
    #[serde(default)]
    pub fallback: bool,
}

#[derive(Clone)]
struct PythonExecutor {
    monty_backend: Arc<dyn PythonExecutionBackend>,
    cpython_backend: Arc<dyn PythonExecutionBackend>,
}

fn should_auto_fallback_to_cpython(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("no such file")
        || lower.contains("not found")
        || lower.contains("cannot find the file")
}

impl PythonExecutor {
    fn new(
        monty_backend: Arc<dyn PythonExecutionBackend>,
        cpython_backend: Arc<dyn PythonExecutionBackend>,
    ) -> Self {
        Self {
            monty_backend,
            cpython_backend,
        }
    }

    async fn execute(&self, input: &RunPythonInput) -> ToolOutput {
        let requested_runtime = input.runtime.clone().unwrap_or_default();
        let request = PythonExecutionRequest {
            code: input.code.clone(),
            timeout_seconds: input
                .timeout_seconds
                .unwrap_or(DEFAULT_TIMEOUT_SECONDS)
                .max(1),
            runtime: requested_runtime.clone(),
            limits: input.limits.clone(),
        };

        let primary_backend = match requested_runtime {
            PythonRuntime::Monty => self.monty_backend.clone(),
            PythonRuntime::Cpython => self.cpython_backend.clone(),
        };

        match primary_backend.execute(request.clone()).await {
            Ok(output) => ToolOutput {
                success: output.exit_code == 0 && !output.timed_out,
                result: serde_json::to_value(output).unwrap_or(Value::Null),
                error: None,
                error_category: None,
                retryable: None,
                retry_after_ms: None,
            },
            Err(primary_error) => {
                let allow_fallback =
                    input.fallback || should_auto_fallback_to_cpython(&primary_error);
                if requested_runtime == PythonRuntime::Monty && allow_fallback {
                    let mut fallback_request = request.clone();
                    fallback_request.runtime = PythonRuntime::Cpython;
                    match self.cpython_backend.execute(fallback_request).await {
                        Ok(output) => ToolOutput {
                            success: output.exit_code == 0 && !output.timed_out,
                            result: serde_json::to_value(output).unwrap_or(Value::Null),
                            error: Some(format!(
                                "Monty backend failed, used cpython fallback: {}",
                                primary_error
                            )),
                            error_category: None,
                            retryable: None,
                            retry_after_ms: None,
                        },
                        Err(fallback_error) => ToolOutput::error(format!(
                            "Monty backend failed ({}) and cpython fallback failed ({})",
                            primary_error, fallback_error
                        )),
                    }
                } else {
                    ToolOutput::error(primary_error)
                }
            }
        }
    }
}

#[derive(Clone)]
struct MontyPythonTool {
    name: &'static str,
    executor: PythonExecutor,
    default_runtime: PythonRuntime,
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: Option<String>,
    task_id: Option<String>,
}

impl MontyPythonTool {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            executor: PythonExecutor::new(
                Arc::new(ProcessPythonBackend::monty()),
                Arc::new(ProcessPythonBackend::cpython()),
            ),
            default_runtime: PythonRuntime::Monty,
            security_gate: None,
            agent_id: None,
            task_id: None,
        }
    }

    fn with_backends(
        mut self,
        monty_backend: Arc<dyn PythonExecutionBackend>,
        cpython_backend: Arc<dyn PythonExecutionBackend>,
    ) -> Self {
        self.executor = PythonExecutor::new(monty_backend, cpython_backend);
        self
    }

    fn with_security(
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

    fn with_default_runtime(mut self, runtime: PythonRuntime) -> Self {
        self.default_runtime = runtime;
        self
    }
}

#[derive(Clone)]
pub struct RunPythonTool {
    inner: MontyPythonTool,
}

impl Default for RunPythonTool {
    fn default() -> Self {
        Self::new()
    }
}

impl RunPythonTool {
    pub fn new() -> Self {
        Self {
            inner: MontyPythonTool::new("run_python"),
        }
    }

    pub fn with_default_runtime(self, runtime: PythonRuntime) -> Self {
        Self {
            inner: self.inner.with_default_runtime(runtime),
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

#[derive(Clone)]
pub struct PythonTool {
    inner: MontyPythonTool,
}

impl Default for PythonTool {
    fn default() -> Self {
        Self::new()
    }
}

impl PythonTool {
    pub fn new() -> Self {
        Self {
            inner: MontyPythonTool::new("python"),
        }
    }

    pub fn with_default_runtime(self, runtime: PythonRuntime) -> Self {
        Self {
            inner: self.inner.with_default_runtime(runtime),
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
            "runtime": {
                "type": "string",
                "enum": ["monty", "cpython"],
                "description": "Runtime backend policy (defaults to tool policy)"
            },
            "limits": {
                "type": "object",
                "properties": {
                    "max_time_ms": { "type": "integer", "description": "Maximum runtime in milliseconds (enforced)" },
                    "max_memory_mb": { "type": "integer", "description": "Reserved for future support; currently rejected by process backend" },
                    "max_steps": { "type": "integer", "description": "Reserved for future support; currently rejected by process backend" }
                }
            },
            "fallback": {
                "type": "boolean",
                "description": "Fallback to cpython when monty backend fails (also auto-fallbacks if monty binary is unavailable)"
            }
        },
        "required": ["code"]
    })
}

#[async_trait]
impl Tool for RunPythonTool {
    fn name(&self) -> &str {
        self.inner.name
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
        let mut parsed: RunPythonInput = serde_json::from_value(input)?;
        let effective_runtime = parsed
            .runtime
            .clone()
            .unwrap_or_else(|| self.inner.default_runtime.clone());
        parsed.runtime = Some(effective_runtime.clone());
        if let Some(security_gate) = self.inner.security_gate.as_deref() {
            let action = ToolAction {
                tool_name: self.name().to_string(),
                operation: "execute".to_string(),
                target: effective_runtime.as_str().to_string(),
                summary: "Execute Python code".to_string(),
            };
            if let Some(message) = check_security(
                Some(security_gate),
                action,
                self.inner.agent_id.as_deref(),
                self.inner.task_id.as_deref(),
            )
            .await?
            {
                return Ok(ToolOutput::error(message));
            }
        }
        Ok(self.inner.executor.execute(&parsed).await)
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
        let mut parsed: RunPythonInput = serde_json::from_value(input)?;
        let effective_runtime = parsed
            .runtime
            .clone()
            .unwrap_or_else(|| self.inner.default_runtime.clone());
        parsed.runtime = Some(effective_runtime.clone());
        if let Some(security_gate) = self.inner.security_gate.as_deref() {
            let action = ToolAction {
                tool_name: self.name().to_string(),
                operation: "execute".to_string(),
                target: effective_runtime.as_str().to_string(),
                summary: "Execute Python code".to_string(),
            };
            if let Some(message) = check_security(
                Some(security_gate),
                action,
                self.inner.agent_id.as_deref(),
                self.inner.task_id.as_deref(),
            )
            .await?
            {
                return Ok(ToolOutput::error(message));
            }
        }
        Ok(self.inner.executor.execute(&parsed).await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    #[derive(Clone)]
    struct MockBackend {
        fail: bool,
        runtime: PythonRuntime,
    }

    #[async_trait]
    impl PythonExecutionBackend for MockBackend {
        async fn execute(
            &self,
            request: PythonExecutionRequest,
        ) -> std::result::Result<super::super::python_backend::PythonExecutionResult, String>
        {
            if self.fail {
                return Err(format!("{} backend failure", self.runtime.as_str()));
            }
            Ok(super::super::python_backend::PythonExecutionResult {
                stdout: request.code,
                stderr: String::new(),
                exit_code: 0,
                runtime: self.runtime.as_str().to_string(),
                timed_out: false,
                limits: request.limits,
            })
        }
    }

    #[tokio::test]
    async fn run_python_success_path() {
        let tool = RunPythonTool::new();
        let output = tool
            .execute(json!({
                "code": "print('ok')",
                "timeout_seconds": 2,
                "runtime": "cpython"
            }))
            .await
            .expect("tool execute should succeed");
        assert!(output.result.is_object());
    }

    #[tokio::test]
    async fn run_python_handles_syntax_error() {
        let tool = RunPythonTool::new();
        let output = tool
            .execute(json!({
                "code": "def broken(:\n pass",
                "timeout_seconds": 2,
                "runtime": "cpython"
            }))
            .await
            .expect("tool execute should return output");
        assert!(!output.success);
    }

    #[tokio::test]
    async fn run_python_timeout_is_reported() {
        let tool = RunPythonTool::new();
        let output = tool
            .execute(json!({
                "code": "while True:\n  pass",
                "timeout_seconds": 1,
                "runtime": "cpython"
            }))
            .await
            .expect("tool execute should return output");
        let timed_out = output
            .result
            .get("timed_out")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        assert!(timed_out || !output.success);
    }

    #[tokio::test]
    async fn fallback_to_cpython_when_monty_fails() {
        let monty = Arc::new(MockBackend {
            fail: true,
            runtime: PythonRuntime::Monty,
        });
        let cpython = Arc::new(MockBackend {
            fail: false,
            runtime: PythonRuntime::Cpython,
        });

        let tool = RunPythonTool {
            inner: MontyPythonTool::new("run_python").with_backends(monty, cpython),
        };

        let output = tool
            .execute(json!({
                "code": "print('fallback')",
                "runtime": "monty",
                "fallback": true
            }))
            .await
            .expect("tool execute should return output");
        assert!(output.success);
        assert_eq!(
            output
                .result
                .get("runtime")
                .and_then(|value| value.as_str()),
            Some("cpython")
        );
    }

    #[tokio::test]
    async fn uses_tool_default_runtime_when_input_runtime_missing() {
        let monty = Arc::new(MockBackend {
            fail: false,
            runtime: PythonRuntime::Monty,
        });
        let cpython = Arc::new(MockBackend {
            fail: false,
            runtime: PythonRuntime::Cpython,
        });

        let tool = RunPythonTool {
            inner: MontyPythonTool::new("run_python")
                .with_backends(monty, cpython)
                .with_default_runtime(PythonRuntime::Cpython),
        };

        let output = tool
            .execute(json!({
                "code": "print('default-runtime')"
            }))
            .await
            .expect("tool execute should return output");
        assert_eq!(
            output
                .result
                .get("runtime")
                .and_then(|value| value.as_str()),
            Some("cpython")
        );
    }

    #[tokio::test]
    async fn rejects_unsupported_limits() {
        let tool = RunPythonTool::new();
        let output = tool
            .execute(json!({
                "code": "print('x')",
                "runtime": "cpython",
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
