use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{Duration, timeout};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PythonRuntime {
    Monty,
    Cpython,
}

impl Default for PythonRuntime {
    fn default() -> Self {
        Self::Monty
    }
}

impl PythonRuntime {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Monty => "monty",
            Self::Cpython => "cpython",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PythonExecutionLimits {
    #[serde(default)]
    pub max_time_ms: Option<u64>,
    #[serde(default)]
    pub max_memory_mb: Option<u64>,
    #[serde(default)]
    pub max_steps: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct PythonExecutionRequest {
    pub code: String,
    pub timeout_seconds: u64,
    pub runtime: PythonRuntime,
    pub limits: Option<PythonExecutionLimits>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonExecutionResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub runtime: String,
    pub timed_out: bool,
    pub limits: Option<PythonExecutionLimits>,
}

#[async_trait]
pub trait PythonExecutionBackend: Send + Sync {
    async fn execute(
        &self,
        request: PythonExecutionRequest,
    ) -> std::result::Result<PythonExecutionResult, String>;
}

#[derive(Clone)]
pub struct ProcessPythonBackend {
    runtime: PythonRuntime,
}

impl ProcessPythonBackend {
    pub fn monty() -> Self {
        Self {
            runtime: PythonRuntime::Monty,
        }
    }

    pub fn cpython() -> Self {
        Self {
            runtime: PythonRuntime::Cpython,
        }
    }

    fn resolve_python_executable(&self) -> &'static str {
        "python3"
    }
}

#[async_trait]
impl PythonExecutionBackend for ProcessPythonBackend {
    async fn execute(
        &self,
        request: PythonExecutionRequest,
    ) -> std::result::Result<PythonExecutionResult, String> {
        let mut cmd = Command::new(self.resolve_python_executable());
        cmd.arg("-c")
            .arg(&request.code)
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let execution = timeout(Duration::from_secs(request.timeout_seconds), cmd.output()).await;
        match execution {
            Ok(Ok(output)) => {
                let exit_code = output.status.code().unwrap_or(-1);
                Ok(PythonExecutionResult {
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                    exit_code,
                    runtime: request.runtime.as_str().to_string(),
                    timed_out: false,
                    limits: request.limits,
                })
            }
            Ok(Err(err)) => Err(format!(
                "{} runtime execution failed: {}",
                self.runtime.as_str(),
                err
            )),
            Err(_) => Ok(PythonExecutionResult {
                stdout: String::new(),
                stderr: format!(
                    "Python execution timed out after {} seconds",
                    request.timeout_seconds
                ),
                exit_code: 124,
                runtime: request.runtime.as_str().to_string(),
                timed_out: true,
                limits: request.limits,
            }),
        }
    }
}
