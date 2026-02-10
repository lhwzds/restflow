use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{Duration, timeout};

const MONTY_EXECUTABLE_ENV: &str = "RESTFLOW_MONTY_EXECUTABLE";
const CPYTHON_EXECUTABLE_ENV: &str = "RESTFLOW_CPYTHON_EXECUTABLE";

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

    fn resolve_python_executable(&self) -> String {
        match self.runtime {
            PythonRuntime::Monty => std::env::var(MONTY_EXECUTABLE_ENV)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "monty".to_string()),
            PythonRuntime::Cpython => std::env::var(CPYTHON_EXECUTABLE_ENV)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "python3".to_string()),
        }
    }

    fn validate_limits(request: &PythonExecutionRequest) -> std::result::Result<(), String> {
        let Some(limits) = request.limits.as_ref() else {
            return Ok(());
        };

        if limits.max_memory_mb.is_some() || limits.max_steps.is_some() {
            return Err(
                "max_memory_mb and max_steps are not supported by process backend yet".to_string(),
            );
        }

        Ok(())
    }

    fn effective_timeout_duration(request: &PythonExecutionRequest) -> Duration {
        let timeout_ms_from_seconds = request.timeout_seconds.saturating_mul(1000);
        let timeout_ms = request
            .limits
            .as_ref()
            .and_then(|limits| limits.max_time_ms)
            .map(|max_time_ms| max_time_ms.min(timeout_ms_from_seconds))
            .unwrap_or(timeout_ms_from_seconds)
            .max(1);

        Duration::from_millis(timeout_ms)
    }
}

#[async_trait]
impl PythonExecutionBackend for ProcessPythonBackend {
    async fn execute(
        &self,
        request: PythonExecutionRequest,
    ) -> std::result::Result<PythonExecutionResult, String> {
        Self::validate_limits(&request)?;

        let executable = self.resolve_python_executable();
        let mut cmd = Command::new(&executable);
        cmd.arg("-c")
            .arg(&request.code)
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let timeout_duration = Self::effective_timeout_duration(&request);
        let execution = timeout(timeout_duration, cmd.output()).await;
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
                "{} runtime execution failed ({}): {}",
                self.runtime.as_str(),
                executable,
                err
            )),
            Err(_) => Ok(PythonExecutionResult {
                stdout: String::new(),
                stderr: format!(
                    "Python execution timed out after {} ms",
                    timeout_duration.as_millis()
                ),
                exit_code: 124,
                runtime: request.runtime.as_str().to_string(),
                timed_out: true,
                limits: request.limits,
            }),
        }
    }
}
