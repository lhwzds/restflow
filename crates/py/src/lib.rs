//! # codocia
//!
//! Py owns the PyO3 module boundary for the core.
//!
//! ## Owns
//! - restflow_native Python module
//! - CoreCommand/CoreResponse JSON ABI entrypoint
//! - executable skill runtime JSON ABI entrypoints
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
//! - executable skill artifact directories
//! - executable skill JSON input
//!
//! ## Outputs
//! - CoreResponse JSON
//! - executable skill runtime JSON values
//!
//! ## Depends On
//! - engine
//! - server
//! - model
//! - runtime
//!
//! ## Used By
//! - python/restflow CoreClient.native
//!
//! ## Verify
//! - cargo check -p restflow-native

use anyhow::Result;
use engine::Core;
#[cfg(feature = "python-module")]
use pyo3::exceptions::PyRuntimeError;
#[cfg(feature = "python-module")]
use pyo3::prelude::*;
use serde_json::Value;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll, Wake, Waker};
use std::time::Duration;

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
#[pyfunction]
fn runtime_load_artifact_json(py: Python<'_>, root: &str) -> PyResult<String> {
    py.detach(|| load_artifact_json(root)).map_err(to_py_error)
}

#[cfg(feature = "python-module")]
#[pyfunction]
fn runtime_list_skills_json(py: Python<'_>, root: Option<String>) -> PyResult<String> {
    py.detach(|| list_skills_json(root)).map_err(to_py_error)
}

#[cfg(feature = "python-module")]
#[pyfunction]
fn runtime_build_skill_json(
    py: Python<'_>,
    root: &str,
    target_dir: Option<String>,
) -> PyResult<String> {
    py.detach(|| build_skill_json(root, target_dir))
        .map_err(to_py_error)
}

#[cfg(feature = "python-module")]
#[pyfunction]
fn runtime_run_skill_json(
    py: Python<'_>,
    root: &str,
    input_json: &str,
    timeout_seconds: u64,
) -> PyResult<String> {
    py.detach(|| run_skill_json(root, input_json, timeout_seconds))
        .map_err(to_py_error)
}

#[cfg(feature = "python-module")]
#[pyfunction]
fn runtime_install_local_skill_json(
    py: Python<'_>,
    source: &str,
    root: Option<String>,
    skill_id: Option<String>,
    overwrite: bool,
) -> PyResult<String> {
    py.detach(|| install_local_skill_json(source, root, skill_id, overwrite))
        .map_err(to_py_error)
}

#[cfg(feature = "python-module")]
#[pymodule]
fn restflow_native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyCore>()?;
    module.add_function(wrap_pyfunction!(handle_json, module)?)?;
    module.add_function(wrap_pyfunction!(reset, module)?)?;
    module.add_function(wrap_pyfunction!(runtime_load_artifact_json, module)?)?;
    module.add_function(wrap_pyfunction!(runtime_list_skills_json, module)?)?;
    module.add_function(wrap_pyfunction!(runtime_build_skill_json, module)?)?;
    module.add_function(wrap_pyfunction!(runtime_run_skill_json, module)?)?;
    module.add_function(wrap_pyfunction!(runtime_install_local_skill_json, module)?)?;
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

pub fn load_artifact_json(root: &str) -> Result<String> {
    Ok(serde_json::to_string(&runtime::load_artifact(root)?)?)
}

pub fn list_skills_json(root: Option<String>) -> Result<String> {
    let root = runtime_root(root)?;
    Ok(serde_json::to_string(&runtime::list_installed_skills(
        root,
    )?)?)
}

pub fn build_skill_json(root: &str, target_dir: Option<String>) -> Result<String> {
    let options = runtime::BuildOptions {
        target_dir: target_dir.map(PathBuf::from),
        ..runtime::BuildOptions::default()
    };
    Ok(serde_json::to_string(&runtime::build_skill(
        root, &options,
    )?)?)
}

pub fn run_skill_json(root: &str, input_json: &str, timeout_seconds: u64) -> Result<String> {
    let input: Value = serde_json::from_str(input_json)?;
    let options = runtime::RunOptions {
        timeout: Duration::from_secs(timeout_seconds),
        ..runtime::RunOptions::default()
    };
    Ok(serde_json::to_string(
        &runtime::run_skill(root, input, &options)?.value,
    )?)
}

pub fn install_local_skill_json(
    source: &str,
    root: Option<String>,
    skill_id: Option<String>,
    overwrite: bool,
) -> Result<String> {
    let mut options = runtime::InstallOptions::new(runtime_root(root)?).with_overwrite(overwrite);
    if let Some(skill_id) = skill_id {
        options = options.with_skill_id(skill_id);
    }
    Ok(serde_json::to_string(&runtime::install_local_skill(
        source, &options,
    )?)?)
}

fn runtime_root(root: Option<String>) -> Result<PathBuf> {
    match root {
        Some(root) if !root.is_empty() => Ok(PathBuf::from(root)),
        _ => runtime::default_skills_dir(),
    }
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

    #[test]
    fn runtime_load_artifact_json_returns_artifact() {
        let root = temp_dir("runtime-artifact");
        runtime::scaffold_skill(
            &root,
            runtime::ScaffoldOptions::python_uv("py-echo", "Python Echo", "0.1.0"),
        )
        .unwrap();

        let response = load_artifact_json(root.to_str().unwrap()).unwrap();
        let decoded: serde_json::Value = serde_json::from_str(&response).unwrap();

        assert_eq!(decoded["id"], "py-echo");
        assert_eq!(decoded["kind"], "python_uv");
    }

    #[test]
    fn runtime_install_and_list_skill_json_round_trip() {
        let source = temp_dir("runtime-install-source");
        let target = temp_dir("runtime-install-target");
        runtime::scaffold_skill(
            &source,
            runtime::ScaffoldOptions::python_uv("py-echo", "Python Echo", "0.1.0"),
        )
        .unwrap();

        let installed = install_local_skill_json(
            source.to_str().unwrap(),
            Some(target.to_str().unwrap().to_string()),
            None,
            false,
        )
        .unwrap();
        let listed = list_skills_json(Some(target.to_str().unwrap().to_string())).unwrap();

        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&installed).unwrap()["id"],
            "py-echo"
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&listed).unwrap()[0]["id"],
            "py-echo"
        );
    }

    #[cfg(unix)]
    #[test]
    fn runtime_run_skill_json_returns_output_value() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let root = temp_dir("runtime-run");
        let artifact = runtime::SkillArtifact::rust_binary("echo", "Echo", "0.1.0");
        runtime::save_artifact(&root, &artifact).unwrap();
        let entry = artifact.entry_path(&root);
        fs::create_dir_all(entry.parent().unwrap()).unwrap();
        fs::write(&entry, "#!/bin/sh\ncat\n").unwrap();
        let mut permissions = fs::metadata(&entry).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&entry, permissions).unwrap();

        let response =
            run_skill_json(root.to_str().unwrap(), r#"{"message":"hello"}"#, 10).unwrap();

        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&response).unwrap(),
            json!({ "message": "hello" })
        );
    }

    fn temp_dir(name: &str) -> PathBuf {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "restflow-native-{name}-{}-{now}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }
}
