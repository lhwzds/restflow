use anyhow::{Context, Result, bail};
use assert_cmd::Command;
use restflow_core::daemon::is_daemon_available;
use restflow_core::paths;
use serde_json::Value;
use std::fs::File;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;
use tempfile::tempdir;
use tokio::time::{Instant, sleep};

fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct DaemonChild {
    child: Child,
    log_path: PathBuf,
}

impl DaemonChild {
    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        self.child.try_wait()
    }

    fn diagnostics(&self) -> String {
        std::fs::read_to_string(&self.log_path)
            .unwrap_or_else(|error| format!("<failed to read {}: {error}>", self.log_path.display()))
    }
}

impl Drop for DaemonChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn reserve_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind port");
    let port = listener.local_addr().expect("local addr").port();
    drop(listener);
    port
}

fn spawn_daemon(db_path: &str, state_dir: &Path, port: u16) -> Result<DaemonChild> {
    let log_path = state_dir.join("hook-daemon-test.log");
    let log_file = File::create(&log_path).context("create daemon log")?;
    let stderr_file = log_file.try_clone().context("clone daemon log")?;
    let child = std::process::Command::new(assert_cmd::cargo::cargo_bin!("restflow"))
        .args([
            "--db-path",
            db_path,
            "daemon",
            "start",
            "--foreground",
            "--mcp-port",
            &port.to_string(),
        ])
        .env("RESTFLOW_DIR", state_dir)
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .context("spawn daemon")?;
    Ok(DaemonChild { child, log_path })
}

async fn wait_for_daemon_ready(state_dir: &Path, daemon: &mut DaemonChild) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(12);
    while Instant::now() < deadline {
        if let Some(status) = daemon.try_wait().context("poll daemon")? {
            bail!(
                "daemon exited before socket was ready: {}\n{}",
                status,
                daemon.diagnostics()
            );
        }

        let prev = std::env::var_os("RESTFLOW_DIR");
        unsafe { std::env::set_var("RESTFLOW_DIR", state_dir) };
        let socket_path = paths::socket_path().context("socket path")?;
        let ready = is_daemon_available(&socket_path).await;
        match prev {
            Some(value) => unsafe { std::env::set_var("RESTFLOW_DIR", value) },
            None => unsafe { std::env::remove_var("RESTFLOW_DIR") },
        }
        if ready {
            return Ok(());
        }

        sleep(Duration::from_millis(100)).await;
    }

    bail!("timed out waiting for daemon readiness\n{}", daemon.diagnostics())
}

fn parse_json_output(output: &[u8]) -> Value {
    serde_json::from_slice(output).expect("valid json output")
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn hook_commands_route_through_daemon_path() -> Result<()> {
    let _guard = env_lock();
    let temp = tempdir().context("tempdir")?;
    let db_path = temp.path().join("restflow.db");
    let mut daemon = spawn_daemon(
        db_path.to_str().expect("db path"),
        temp.path(),
        reserve_port(),
    )?;
    wait_for_daemon_ready(temp.path(), &mut daemon).await?;

    let list = Command::new(assert_cmd::cargo::cargo_bin!("restflow"))
        .env("RESTFLOW_DIR", temp.path())
        .args(["--format", "json", "hook", "list"])
        .output()
        .context("run hook list")?;
    assert!(list.status.success(), "{}", String::from_utf8_lossy(&list.stderr));
    let listed = parse_json_output(&list.stdout);
    assert_eq!(listed, Value::Array(Vec::new()));

    let created = Command::new(assert_cmd::cargo::cargo_bin!("restflow"))
        .env("RESTFLOW_DIR", temp.path())
        .args([
            "--format",
            "json",
            "hook",
            "create",
            "--name",
            "daemon-hook",
            "--event",
            "task_started",
            "--action",
            "script",
            "--script",
            "/usr/bin/true",
        ])
        .output()
        .context("run hook create")?;
    assert!(created.status.success(), "{}", String::from_utf8_lossy(&created.stderr));
    let created_value = parse_json_output(&created.stdout);
    let hook_id = created_value["id"]
        .as_str()
        .context("hook create should return id")?
        .to_string();

    let tested = Command::new(assert_cmd::cargo::cargo_bin!("restflow"))
        .env("RESTFLOW_DIR", temp.path())
        .args(["--format", "json", "hook", "test", &hook_id])
        .output()
        .context("run hook test")?;
    assert!(tested.status.success(), "{}", String::from_utf8_lossy(&tested.stderr));
    let tested_value = parse_json_output(&tested.stdout);
    assert_eq!(tested_value["id"], hook_id);
    assert_eq!(tested_value["tested"], true);

    Ok(())
}
