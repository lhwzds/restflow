use std::time::Duration;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::wrap_pyfunction;
use serde::Serialize;

use crate::AgentConfig;

#[pyclass(name = "AgentConfig")]
#[derive(Clone, Debug)]
pub struct PyAgentConfig {
    #[pyo3(get, set)]
    goal: String,
    #[pyo3(get, set)]
    system_prompt: Option<String>,
    #[pyo3(get, set)]
    max_iterations: usize,
    #[pyo3(get, set)]
    temperature: Option<f32>,
    #[pyo3(get, set)]
    tool_timeout_secs: u64,
    #[pyo3(get, set)]
    llm_timeout_secs: Option<u64>,
    #[pyo3(get, set)]
    max_tool_result_length: usize,
    #[pyo3(get, set)]
    context_window: usize,
    #[pyo3(get, set)]
    max_output_tokens: Option<u32>,
    #[pyo3(get, set)]
    yolo_mode: bool,
}

impl PyAgentConfig {
    fn from_agent_config(config: AgentConfig) -> Self {
        Self {
            goal: config.goal,
            system_prompt: config.system_prompt,
            max_iterations: config.max_iterations,
            temperature: config.temperature,
            tool_timeout_secs: config.tool_timeout.as_secs(),
            llm_timeout_secs: config.llm_timeout.map(|timeout| timeout.as_secs()),
            max_tool_result_length: config.max_tool_result_length,
            context_window: config.context_window,
            max_output_tokens: config.max_output_tokens,
            yolo_mode: config.yolo_mode,
        }
    }

    fn to_agent_config(&self) -> AgentConfig {
        let mut config = AgentConfig::new(self.goal.clone());
        config.system_prompt = self.system_prompt.clone();
        config.max_iterations = self.max_iterations;
        config.temperature = self.temperature;
        config.tool_timeout = Duration::from_secs(self.tool_timeout_secs);
        config.llm_timeout = self.llm_timeout_secs.map(Duration::from_secs);
        config.max_tool_result_length = self.max_tool_result_length;
        config.context_window = self.context_window;
        config.max_output_tokens = self.max_output_tokens;
        config.yolo_mode = self.yolo_mode;
        config
    }
}

#[pymethods]
impl PyAgentConfig {
    #[new]
    #[pyo3(signature = (goal, system_prompt=None, max_iterations=None))]
    fn new(goal: String, system_prompt: Option<String>, max_iterations: Option<usize>) -> Self {
        let mut config = AgentConfig::new(goal);
        config.system_prompt = system_prompt;
        if let Some(max_iterations) = max_iterations {
            config.max_iterations = max_iterations;
        }
        Self::from_agent_config(config)
    }

    fn to_json(&self) -> PyResult<String> {
        let config = self.to_agent_config();
        serde_json::to_string(&AgentConfigSnapshot::from(&config))
            .map_err(|err| PyValueError::new_err(err.to_string()))
    }
}

#[derive(Serialize)]
struct AgentConfigSnapshot {
    goal: String,
    system_prompt: Option<String>,
    max_iterations: usize,
    temperature: Option<f32>,
    tool_timeout_secs: u64,
    llm_timeout_secs: Option<u64>,
    max_tool_result_length: usize,
    context_window: usize,
    max_output_tokens: Option<u32>,
    yolo_mode: bool,
}

impl From<&AgentConfig> for AgentConfigSnapshot {
    fn from(config: &AgentConfig) -> Self {
        Self {
            goal: config.goal.clone(),
            system_prompt: config.system_prompt.clone(),
            max_iterations: config.max_iterations,
            temperature: config.temperature,
            tool_timeout_secs: config.tool_timeout.as_secs(),
            llm_timeout_secs: config.llm_timeout.map(|timeout| timeout.as_secs()),
            max_tool_result_length: config.max_tool_result_length,
            context_window: config.context_window,
            max_output_tokens: config.max_output_tokens,
            yolo_mode: config.yolo_mode,
        }
    }
}

#[pyfunction]
#[pyo3(signature = (goal, system_prompt=None, max_iterations=None))]
fn agent_config(
    goal: String,
    system_prompt: Option<String>,
    max_iterations: Option<usize>,
) -> PyAgentConfig {
    PyAgentConfig::new(goal, system_prompt, max_iterations)
}

#[pyfunction]
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_class::<PyAgentConfig>()?;
    m.add_function(wrap_pyfunction!(agent_config, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::PyAgentConfig;

    #[test]
    fn serializes_agent_config_snapshot() {
        let config = PyAgentConfig::new(
            "ship the minimal runtime".to_string(),
            Some("Use a small tool surface.".to_string()),
            Some(8),
        );

        let json = config.to_json().expect("config should serialize");

        assert!(json.contains("\"goal\":\"ship the minimal runtime\""));
        assert!(json.contains("\"max_iterations\":8"));
    }
}
