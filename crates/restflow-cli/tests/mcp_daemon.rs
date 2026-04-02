use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde_json::{Value, json};
use std::fs::File;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
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
        let body = std::fs::read_to_string(&self.log_path).unwrap_or_else(|error| {
            format!("<failed to read log {}: {error}>", self.log_path.display())
        });
        let tail = tail_text(&body, 8000);
        format!("daemon_log_path={}\n{}", self.log_path.display(), tail)
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
    let log_path = Path::new(state_dir).join("mcp-daemon-test.log");
    let log_file = File::create(&log_path).context("create daemon test log")?;
    let stderr_file = log_file
        .try_clone()
        .context("clone daemon test log handle")?;
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
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .context("failed to spawn restflow daemon")?;
    Ok(DaemonChild { child, log_path })
}

fn reserve_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    drop(listener);
    port
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

fn tail_text(body: &str, max_chars: usize) -> String {
    let count = body.chars().count();
    if count <= max_chars {
        return body.to_string();
    }
    body.chars().skip(count - max_chars).collect()
}

fn parse_json_rpc_response(body: &str) -> Result<Value> {
    let trimmed = body.trim();
    if let Ok(value) = serde_json::from_str(trimmed) {
        return Ok(value);
    }

    let mut parsed_event = None;
    for event in trimmed.split("\n\n") {
        let payload = event
            .lines()
            .filter(|line| !line.trim_start().starts_with(':'))
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if payload.is_empty() {
            continue;
        }
        match serde_json::from_str(&payload) {
            Ok(value) => {
                parsed_event = Some(value);
                break;
            }
            Err(_) => continue,
        }
    }

    parsed_event.with_context(|| format!("invalid JSON response: {}", body))
}

async fn wait_for_mcp_ready(url: &str, daemon: &mut DaemonChild) -> Result<()> {
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
    let probe_client = Client::builder()
        .timeout(Duration::from_millis(250))
        .connect_timeout(Duration::from_millis(250))
        .build()
        .context("build MCP readiness client")?;
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut last_error = String::from("daemon has not responded yet");

    while Instant::now() < deadline {
        if let Some(status) = daemon
            .try_wait()
            .context("failed to poll daemon child status")?
        {
            bail!(
                "daemon exited before MCP was ready: {}\n{}",
                status,
                daemon.diagnostics()
            );
        }

        match post_json_rpc(&probe_client, url, initialize.clone()).await {
            Ok(response) if response.get("result").is_some() => return Ok(()),
            Ok(response) => {
                last_error = format!("unexpected initialize response: {response}");
            }
            Err(error) => {
                last_error = error.to_string();
            }
        }

        sleep(Duration::from_millis(100)).await;
    }

    bail!(
        "timed out waiting for MCP HTTP server readiness; last_error: {}\n{}",
        last_error,
        daemon.diagnostics()
    )
}

async fn spawn_ready_daemon(db_path: &str, state_dir: &str) -> Result<(DaemonChild, String)> {
    let mut failures = Vec::new();

    for attempt in 0..6 {
        let port = reserve_port();
        let url = format!("http://127.0.0.1:{port}/mcp");
        let mut daemon = spawn_daemon(db_path, state_dir, port)
            .with_context(|| format!("spawn daemon for attempt {}", attempt + 1))?;

        match wait_for_mcp_ready(&url, &mut daemon).await {
            Ok(()) => return Ok((daemon, url)),
            Err(error) => {
                failures.push(format!(
                    "attempt {} on port {} failed: {:#}",
                    attempt + 1,
                    port,
                    error
                ));
                drop(daemon);
            }
        }
    }

    bail!(
        "failed to start MCP daemon after retries:\n{}",
        failures.join("\n\n")
    )
}

fn tool_call_text(response: &Value) -> String {
    if !response["result"]["structuredContent"].is_null() {
        return response["result"]["structuredContent"].to_string();
    }

    let items = response["result"]["content"]
        .as_array()
        .expect("tool response should contain content array");

    items
        .iter()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .find(|text| {
            let trimmed = text.trim();
            trimmed.starts_with('{') || trimmed.starts_with('[')
        })
        .or_else(|| {
            items
                .iter()
                .find_map(|item| item.get("text").and_then(Value::as_str))
        })
        .expect("tool response should contain text content")
        .to_string()
}

fn parse_tool_text_json(text: &str) -> Result<Value> {
    if let Ok(value) = serde_json::from_str(text) {
        return Ok(value);
    }

    let start = text
        .find(['{', '['])
        .context("tool text should contain JSON payload")?;
    serde_json::from_str(&text[start..]).with_context(|| format!("parse tool text json: {text}"))
}

#[test]
fn tool_call_text_prefers_structured_content_payload() {
    let response = json!({
        "result": {
            "content": [
                { "type": "text", "text": "Background team saved successfully." }
            ],
            "structuredContent": {
                "operation": "save_team",
                "team": "demo"
            }
        }
    });

    let text = tool_call_text(&response);
    let parsed = parse_tool_text_json(&text).expect("structured content should parse as JSON");
    assert_eq!(parsed["operation"], "save_team");
    assert_eq!(parsed["team"], "demo");
}

fn guarded_approval_id(value: &Value) -> Result<&str> {
    value["approval_id"]
        .as_str()
        .or_else(|| value["assessment"]["approval_id"].as_str())
        .context("guarded response should include approval_id")
}

#[test]
fn tail_text_keeps_suffix() {
    let body = "abcdefghij";
    assert_eq!(tail_text(body, 4), "ghij");
    assert_eq!(tail_text(body, 16), body);
}

#[test]
fn parse_json_rpc_response_accepts_sse_with_comment_and_event_lines() {
    let body = ": keepalive\nevent: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"ok\":true}}\n\n";
    let parsed = parse_json_rpc_response(body).expect("SSE response should parse");
    assert_eq!(parsed["result"]["ok"], true);
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_daemon_mcp_manage_background_agents_team_contract() -> Result<()> {
    let _lock = env_lock();
    let temp = tempdir().context("tempdir")?;
    let state_dir = temp.path().join("state");
    std::fs::create_dir_all(&state_dir).context("create state dir")?;
    let db_path = temp.path().join("restflow.db");

    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .context("build reqwest client")?;
    let (_daemon, url) = spawn_ready_daemon(
        db_path.to_str().expect("db path utf8"),
        state_dir.to_str().expect("state dir utf8"),
    )
    .await?;

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

    let save_team_initial = post_json_rpc(
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
    let save_initial_text = tool_call_text(&save_team_initial);
    let save_initial_value = parse_tool_text_json(&save_initial_text)?;
    let save_value = if save_initial_value["operation"] == "save_team" {
        save_initial_value
    } else {
        let approval_id = guarded_approval_id(&save_initial_value)?;
        let save_team = post_json_rpc(
            &client,
            &url,
            json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {
                    "name": "manage_background_agents",
                    "arguments": {
                        "operation": "save_team",
                        "team": "daemon-bg-team",
                        "approval_id": approval_id,
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
        parse_tool_text_json(&save_text)?
    };
    assert_eq!(save_value["operation"], "save_team");

    let get_team = post_json_rpc(
        &client,
        &url,
        json!({
            "jsonrpc": "2.0",
            "id": 5,
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
    let get_value = parse_tool_text_json(&get_text)?;
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

    let run_batch_initial = post_json_rpc(
        &client,
        &url,
        json!({
            "jsonrpc": "2.0",
            "id": 6,
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
    let run_initial_text = tool_call_text(&run_batch_initial);
    let run_initial_value = parse_tool_text_json(&run_initial_text)?;
    let run_value = if run_initial_value["operation"] == "run_batch" {
        run_initial_value
    } else {
        let approval_id = guarded_approval_id(&run_initial_value)?;
        let run_batch = post_json_rpc(
            &client,
            &url,
            json!({
                "jsonrpc": "2.0",
                "id": 7,
                "method": "tools/call",
                "params": {
                    "name": "manage_background_agents",
                    "arguments": {
                        "operation": "run_batch",
                        "team": "daemon-bg-team",
                        "inputs": ["alpha", "beta"],
                        "run_now": false,
                        "approval_id": approval_id
                    }
                }
            }),
        )
        .await?;
        let run_text = tool_call_text(&run_batch);
        parse_tool_text_json(&run_text)?
    };
    assert_eq!(run_value["operation"], "run_batch");
    assert_eq!(run_value["run_now"], false);
    assert_eq!(run_value["total"], 2);

    Ok(())
}
