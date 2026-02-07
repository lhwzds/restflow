use anyhow::{Result, bail};
use tokio::process::Command;

const CLAUDE_MCP_SERVER_NAME: &str = "restflow";
const LEGACY_HTTP_SERVER_NAME: &str = "restflow-http";

async fn is_claude_available() -> bool {
    match Command::new("claude").arg("--version").output().await {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

async fn is_already_configured(command: &str) -> Result<bool> {
    let output = Command::new("claude")
        .args(["mcp", "get", CLAUDE_MCP_SERVER_NAME])
        .output()
        .await?;

    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.contains("Type: stdio")
        && stdout.contains("Args: mcp serve")
        && stdout.contains(command))
}

async fn remove_server(scope: &str, name: &str) {
    let _ = Command::new("claude")
        .args(["mcp", "remove", "--scope", scope, name])
        .output()
        .await;
}

pub async fn try_sync_restflow_stdio_mcp() -> Result<()> {
    if !is_claude_available().await {
        return Ok(());
    }

    remove_server("user", LEGACY_HTTP_SERVER_NAME).await;
    remove_server("local", LEGACY_HTTP_SERVER_NAME).await;

    let command = std::env::current_exe()
        .ok()
        .and_then(|path| path.into_os_string().into_string().ok())
        .unwrap_or_else(|| "restflow".to_string());

    if is_already_configured(&command).await? {
        return Ok(());
    }

    remove_server("user", CLAUDE_MCP_SERVER_NAME).await;
    remove_server("local", CLAUDE_MCP_SERVER_NAME).await;

    let output = Command::new("claude")
        .args([
            "mcp",
            "add",
            "--scope",
            "user",
            CLAUDE_MCP_SERVER_NAME,
            "--",
            &command,
            "mcp",
            "serve",
        ])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "failed to configure Claude MCP server '{}': {}",
            CLAUDE_MCP_SERVER_NAME,
            stderr.trim()
        );
    }

    Ok(())
}
