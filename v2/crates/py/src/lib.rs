//! # codocia
//!
//! Py owns the PyO3 module boundary for the V2 core.
//!
//! ## Owns
//! - restflow_native Python module
//! - CoreCommand/CoreResponse JSON ABI entrypoint
//! - Python-to-Rust error conversion
//! - module-local Core state for prototype bindings
//!
//! ## Must Not
//! - duplicate core business logic in Python
//! - expose database internals
//! - render UI
//!
//! ## Inputs
//! - CoreCommand JSON
//!
//! ## Outputs
//! - CoreResponse JSON
//!
//! ## Depends On
//! - restflow-v2
//! - server
//! - model
//!
//! ## Used By
//! - python/restflow CoreClient.native
//!
//! ## Verify
//! - cargo check -p restflow-native

use anyhow::Result;
#[cfg(feature = "python-module")]
use pyo3::exceptions::PyRuntimeError;
#[cfg(feature = "python-module")]
use pyo3::prelude::*;
use restflow::Core;
use std::future::Future;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll, Wake, Waker};

#[cfg(feature = "python-module")]
use std::sync::{Mutex, OnceLock};

#[cfg(feature = "python-module")]
static CORE: OnceLock<Mutex<Core>> = OnceLock::new();

#[cfg(feature = "python-module")]
#[pyclass(name = "Core")]
struct PyCore {
    inner: Mutex<Core>,
}

#[cfg(feature = "python-module")]
#[pymethods]
impl PyCore {
    #[new]
    fn new() -> Self {
        Self {
            inner: Mutex::new(default_core()),
        }
    }

    fn handle_json(&self, py: Python<'_>, command_json: &str) -> PyResult<String> {
        py.detach(|| run_json_locked(&self.inner, command_json))
            .map_err(to_py_error)
    }

    fn reset(&self) -> PyResult<()> {
        let mut core = self
            .inner
            .lock()
            .map_err(|_| PyRuntimeError::new_err("core state lock was poisoned"))?;
        *core = default_core();
        Ok(())
    }
}

pub fn default_core() -> Core {
    Core::new(model::Model::new("openai", "gpt-5.5"))
}

pub fn handle_json_with_core(core: &mut Core, command_json: &str) -> Result<String> {
    block_on_ready(server::dispatch_json(core, command_json))?
}

#[cfg(feature = "python-module")]
fn global_core() -> &'static Mutex<Core> {
    CORE.get_or_init(|| Mutex::new(default_core()))
}

#[cfg(feature = "python-module")]
#[pyfunction]
fn handle_json(py: Python<'_>, command_json: &str) -> PyResult<String> {
    py.detach(|| run_json_locked(global_core(), command_json))
        .map_err(to_py_error)
}

#[cfg(feature = "python-module")]
#[pyfunction]
fn reset() -> PyResult<()> {
    let mut core = global_core()
        .lock()
        .map_err(|_| PyRuntimeError::new_err("core state lock was poisoned"))?;
    *core = default_core();
    Ok(())
}

#[cfg(feature = "python-module")]
#[pymodule]
fn restflow_native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyCore>()?;
    module.add_function(wrap_pyfunction!(handle_json, module)?)?;
    module.add_function(wrap_pyfunction!(reset, module)?)?;
    Ok(())
}

#[cfg(feature = "python-module")]
fn to_py_error(error: anyhow::Error) -> PyErr {
    PyRuntimeError::new_err(error.to_string())
}

#[cfg(feature = "python-module")]
fn run_json_locked(core: &Mutex<Core>, command_json: &str) -> Result<String> {
    let mut core = core
        .lock()
        .map_err(|_| anyhow::anyhow!("core state lock was poisoned"))?;
    handle_json_with_core(&mut core, command_json)
}

fn block_on_ready<T>(future: impl Future<Output = T>) -> Result<T> {
    let waker = Waker::from(Arc::new(NoopWake));
    let mut context = TaskContext::from_waker(&waker);
    let mut future = std::pin::pin!(future);

    match future.as_mut().poll(&mut context) {
        Poll::Ready(output) => Ok(output),
        Poll::Pending => anyhow::bail!("core future unexpectedly yielded"),
    }
}

struct NoopWake;

impl Wake for NoopWake {
    fn wake(self: Arc<Self>) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn handle_json_switches_model() {
        let mut core = default_core();
        let response = handle_json_with_core(
            &mut core,
            r#"{"type":"switch_model","model":{"provider":{"id":"openai"},"id":"gpt-5.4"}}"#,
        )
        .unwrap();

        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&response).unwrap(),
            json!({
                "type": "model_switched",
                "model": {
                    "provider": { "id": "openai" },
                    "id": "gpt-5.4"
                }
            })
        );
    }

    #[test]
    fn handle_json_reuses_core_state() {
        let mut core = default_core();
        handle_json_with_core(
            &mut core,
            r#"{"type":"save_skill","skill":{"id":"team","name":"Team","source":"system","source_ref":null,"read_only":true,"description":null,"content":"Use parallel workers.","suggested_tools":[]}}"#,
        )
        .unwrap();

        let response = handle_json_with_core(
            &mut core,
            r#"{"type":"chat_turn","session_id":"session-1","message":"use @team","assigned_skills":[]}"#,
        )
        .unwrap();
        let decoded: serde_json::Value = serde_json::from_str(&response).unwrap();

        assert_eq!(decoded["type"], "chat_turn");
        assert!(
            decoded["events"][0]["value"]
                .as_str()
                .unwrap()
                .contains("Mentioned skill: @team")
        );
    }
}
