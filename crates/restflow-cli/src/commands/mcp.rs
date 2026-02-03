use anyhow::{Result, bail};
use comfy_table::{Cell, Table};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::process::Command;
use std::sync::Arc;

use crate::cli::McpCommands;
use crate::output::{OutputFormat, json::print_json};
use restflow_core::AppCore;
use restflow_core::paths;
use restflow_tauri_lib::RestFlowMcpServer;

const MCP_SERVERS_FILE: &str = "mcp_servers.json";

pub async fn run(core: Arc<AppCore>, command: McpCommands, format: OutputFormat) -> Result<()> {
    match command {
        McpCommands::List => list_servers(format).await,
        McpCommands::Add { name, command } => add_server(name, command, format).await,
        McpCommands::Remove { name } => remove_server(&name, format).await,
        McpCommands::Start { name } => start_server(core, &name, format).await,
        McpCommands::Stop { name } => stop_server(&name, format).await,
        McpCommands::Serve => serve_builtin(core).await,
    }
}

async fn list_servers(format: OutputFormat) -> Result<()> {
    let servers = load_servers()?;

    if format.is_json() {
        return print_json(&servers);
    }

    if servers.is_empty() {
        println!("No MCP servers configured.");
        return Ok(());
    }

    let mut table = Table::new();
    table.set_header(vec!["Name", "Command", "Status", "PID"]);

    for server in servers {
        table.add_row(vec![
            Cell::new(server.name),
            Cell::new(server.command),
            Cell::new(server.status),
            Cell::new(server.pid.map(|pid| pid.to_string()).unwrap_or_default()),
        ]);
    }

    crate::output::table::print_table(table)
}

async fn add_server(name: String, command: String, format: OutputFormat) -> Result<()> {
    let mut servers = load_servers()?;

    if servers.iter().any(|server| server.name == name) {
        bail!("MCP server already exists: {name}");
    }

    servers.push(McpServerEntry {
        name,
        command,
        status: "stopped".to_string(),
        pid: None,
    });

    save_servers(&servers)?;

    if format.is_json() {
        return print_json(&servers);
    }

    println!("MCP server added.");
    Ok(())
}

async fn remove_server(name: &str, format: OutputFormat) -> Result<()> {
    let mut servers = load_servers()?;
    let index = servers
        .iter()
        .position(|server| server.name == name)
        .ok_or_else(|| anyhow::anyhow!("MCP server not found: {name}"))?;

    if servers[index].status == "running" {
        bail!("Stop the MCP server before removing it.");
    }

    servers.remove(index);
    save_servers(&servers)?;

    if format.is_json() {
        return print_json(&servers);
    }

    println!("MCP server removed.");
    Ok(())
}

async fn serve_builtin(core: Arc<AppCore>) -> Result<()> {
    let server = RestFlowMcpServer::new(core);
    server.run().await?;
    Ok(())
}

async fn start_server(core: Arc<AppCore>, name: &str, format: OutputFormat) -> Result<()> {
    if name == "restflow" {
        return serve_builtin(core).await;
    }

    let mut servers = load_servers()?;
    let server = servers
        .iter_mut()
        .find(|server| server.name == name)
        .ok_or_else(|| anyhow::anyhow!("MCP server not found: {name}"))?;

    if server.status == "running" {
        bail!("MCP server already running: {name}");
    }

    let child = spawn_command(&server.command)?;
    let pid = child.id() as i32;

    server.status = "running".to_string();
    server.pid = Some(pid);
    save_servers(&servers)?;

    if format.is_json() {
        return print_json(&json!({ "started": true, "name": name, "pid": pid }));
    }

    println!("MCP server started: {name} (pid {pid})");
    Ok(())
}

async fn stop_server(name: &str, format: OutputFormat) -> Result<()> {
    let mut servers = load_servers()?;
    let server = servers
        .iter_mut()
        .find(|server| server.name == name)
        .ok_or_else(|| anyhow::anyhow!("MCP server not found: {name}"))?;

    let pid = server
        .pid
        .ok_or_else(|| anyhow::anyhow!("No pid recorded for {name}"))?;
    stop_process(pid)?;

    server.status = "stopped".to_string();
    server.pid = None;
    save_servers(&servers)?;

    if format.is_json() {
        return print_json(&json!({ "stopped": true, "name": name }));
    }

    println!("MCP server stopped: {name}");
    Ok(())
}

fn load_servers() -> Result<Vec<McpServerEntry>> {
    let path = servers_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let bytes = std::fs::read(path)?;
    let servers: Vec<McpServerEntry> = serde_json::from_slice(&bytes)?;
    Ok(servers)
}

fn save_servers(servers: &[McpServerEntry]) -> Result<()> {
    let path = servers_path()?;
    let bytes = serde_json::to_vec_pretty(servers)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

fn servers_path() -> Result<std::path::PathBuf> {
    Ok(paths::ensure_restflow_dir()?.join(MCP_SERVERS_FILE))
}

fn spawn_command(command: &str) -> Result<std::process::Child> {
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", command])
            .spawn()
            .map_err(Into::into)
    }

    #[cfg(not(target_os = "windows"))]
    {
        Command::new("/bin/sh")
            .args(["-c", command])
            .spawn()
            .map_err(Into::into)
    }
}

fn stop_process(pid: i32) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        let status = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status()?;
        if !status.success() {
            bail!("Failed to stop process {pid}");
        }
        Ok(())
    }

    #[cfg(unix)]
    {
        use nix::sys::signal::{Signal, kill};
        use nix::unistd::Pid;
        kill(Pid::from_raw(pid), Signal::SIGTERM)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct McpServerEntry {
    name: String,
    command: String,
    status: String,
    pid: Option<i32>,
}
