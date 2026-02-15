use anyhow::{Result, bail};
use tokio::process::Command;

const CODEX_MCP_SERVER_NAME: &str = "restflow";

async fn is_codex_available() -> bool {
    match Command::new("codex").arg("--version").output().await {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

async fn is_already_configured(port: u16) -> Result<bool> {
    let output = Command::new("codex")
        .args(["mcp", "get", CODEX_MCP_SERVER_NAME, "--json"])
        .output()
        .await?;

    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let expected_url = format!("http://127.0.0.1:{port}");
    Ok(stdout.contains("streamable_http") && stdout.contains(&expected_url))
}

async fn remove_server() {
    let _ = Command::new("codex")
        .args(["mcp", "remove", CODEX_MCP_SERVER_NAME])
        .output()
        .await;
}

/// Auto-configure Codex CLI to use RestFlow MCP HTTP server.
///
/// Registers (or updates) the `restflow` MCP server entry pointing
/// to `http://127.0.0.1:{port}` using the `codex mcp add --url` command.
/// Silently skips if Codex CLI is not installed.
pub async fn try_sync_codex_http_mcp(port: u16) -> Result<()> {
    if !is_codex_available().await {
        return Ok(());
    }

    if is_already_configured(port).await? {
        return Ok(());
    }

    // Remove stale entry (wrong port or missing)
    remove_server().await;

    let url = format!("http://127.0.0.1:{port}");
    let output = Command::new("codex")
        .args(["mcp", "add", CODEX_MCP_SERVER_NAME, "--url", &url])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "failed to configure Codex MCP server '{}': {}",
            CODEX_MCP_SERVER_NAME,
            stderr.trim()
        );
    }

    Ok(())
}
