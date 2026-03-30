use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde_json::{Value, json};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;
use tempfile::tempdir;
use tokio::time::sleep;

fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn reserve_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    drop(listener);
    port
}

struct DaemonChild {
    child: Child,
}

impl DaemonChild {
    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        self.child.try_wait()
    }
}

impl Drop for DaemonChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn_daemon(db_path: &str, state_dir: &str, port: u16) -> Result<DaemonChild> {
    let web_dist_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/dist");
    let child = Command::new(assert_cmd::cargo::cargo_bin!("restflow"))
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
        .env("RESTFLOW_WEB_DIST_DIR", web_dist_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to spawn restflow daemon")?;
    Ok(DaemonChild { child })
}

async fn post_json_rpc(client: &Client, url: &str, payload: Value) -> Result<Value> {
    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&payload)
        .send()
        .await
        .with_context(|| format!("request failed for payload: {}", payload))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read response body")?;
    if !status.is_success() {
        bail!("HTTP {}: {}", status, body);
    }
    parse_json_rpc_response(&body)
}

fn parse_json_rpc_response(body: &str) -> Result<Value> {
    let trimmed = body.trim();
    if trimmed.starts_with("data:") {
        let payload = trimmed
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if payload.is_empty() {
            bail!("empty SSE MCP response: {}", body);
        }
        return serde_json::from_str(&payload)
            .with_context(|| format!("invalid SSE JSON response: {}", body));
    }
    serde_json::from_str(trimmed).with_context(|| format!("invalid JSON response: {}", body))
}

async fn wait_for_mcp_ready(client: &Client, url: &str, daemon: &mut DaemonChild) -> Result<()> {
    let initialize = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": {
                "name": "restflow-cli-test",
                "version": "0.1.0"
            }
        }
    });

    for _ in 0..120 {
        if let Some(status) = daemon
            .try_wait()
            .context("failed to poll daemon child status")?
        {
            bail!("daemon exited before MCP was ready: {}", status);
        }

        match post_json_rpc(client, url, initialize.clone()).await {
            Ok(response) if response.get("result").is_some() => return Ok(()),
            Ok(_) | Err(_) => sleep(Duration::from_millis(100)).await,
        }
    }

    bail!("timed out waiting for MCP HTTP server readiness")
}

fn tool_call_text(response: &Value) -> String {
    let items = response["result"]["content"]
        .as_array()
        .expect("tool response should contain content array");

    items.iter()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .find(|text| {
            let trimmed = text.trim();
            trimmed.starts_with('{') || trimmed.starts_with('[')
        })
        .or_else(|| items.iter().find_map(|item| item.get("text").and_then(Value::as_str)))
        .expect("tool response should contain text content")
        .to_string()
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_daemon_mcp_manage_background_agents_team_contract() -> Result<()> {
    let _lock = env_lock();
    let temp = tempdir().context("tempdir")?;
    let state_dir = temp.path().join("state");
    std::fs::create_dir_all(&state_dir).context("create state dir")?;
    let db_path = temp.path().join("restflow.db");
    let port = reserve_port();
    let url = format!("http://127.0.0.1:{}/mcp", port);

    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .context("build reqwest client")?;
    let mut daemon = spawn_daemon(
        db_path.to_str().expect("db path utf8"),
        state_dir.to_str().expect("state dir utf8"),
        port,
    )?;

    wait_for_mcp_ready(&client, &url, &mut daemon).await?;

    let tools = post_json_rpc(
        &client,
        &url,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }),
    )
    .await?;
    let tools_list = tools["result"]["tools"]
        .as_array()
        .context("tools/list should return tool array")?;
    assert!(
        tools_list
            .iter()
            .any(|tool| tool["name"].as_str() == Some("manage_background_agents")),
        "manage_background_agents must be exposed over MCP"
    );

    let save_team = post_json_rpc(
        &client,
        &url,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "manage_background_agents",
                "arguments": {
                    "operation": "save_team",
                    "team": "daemon-bg-team",
                    "workers": [
                        {
                            "count": 2,
                            "agent_id": "default"
                        }
                    ]
                }
            }
        }),
    )
    .await?;
    let save_text = tool_call_text(&save_team);
    let save_value: Value =
        serde_json::from_str(&save_text).with_context(|| format!("parse save_team tool text: {save_text}"))?;
    assert_eq!(save_value["operation"], "save_team");

    let get_team = post_json_rpc(
        &client,
        &url,
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "manage_background_agents",
                "arguments": {
                    "operation": "get_team",
                    "team": "daemon-bg-team"
                }
            }
        }),
    )
    .await?;
    let get_text = tool_call_text(&get_team);
    let get_value: Value =
        serde_json::from_str(&get_text).with_context(|| format!("parse get_team tool text: {get_text}"))?;
    assert_eq!(get_value["operation"], "get_team");
    assert_eq!(get_value["member_groups"], 1);
    assert_eq!(get_value["total_instances"], 2);
    assert!(
        get_value["members"][0].get("input").is_none()
            || get_value["members"][0]["input"].is_null()
    );
    assert!(
        get_value["members"][0].get("inputs").is_none()
            || get_value["members"][0]["inputs"].is_null()
    );

    let run_batch = post_json_rpc(
        &client,
        &url,
        json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {
                "name": "manage_background_agents",
                "arguments": {
                    "operation": "run_batch",
                    "team": "daemon-bg-team",
                    "inputs": ["alpha", "beta"],
                    "run_now": false
                }
            }
        }),
    )
    .await?;
    let run_text = tool_call_text(&run_batch);
    let run_value: Value =
        serde_json::from_str(&run_text).with_context(|| format!("parse run_batch tool text: {run_text}"))?;
    assert_eq!(run_value["operation"], "run_batch");
    assert_eq!(run_value["run_now"], false);
    assert_eq!(run_value["total"], 2);

    Ok(())
}
