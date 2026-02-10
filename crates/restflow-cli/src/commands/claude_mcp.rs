use anyhow::{Result, bail};
use tokio::process::Command;

const CLAUDE_MCP_SERVER_NAME: &str = "restflow";
const LEGACY_STDIO_SERVER_NAME: &str = "restflow-stdio";
const LEGACY_HTTP_SERVER_NAME: &str = "restflow-http";

async fn is_claude_available() -> bool {
    match Command::new("claude").arg("--version").output().await {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

async fn is_already_configured(port: u16) -> Result<bool> {
    let output = Command::new("claude")
        .args(["mcp", "get", CLAUDE_MCP_SERVER_NAME])
        .output()
        .await?;

    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let expected_url = format!("http://127.0.0.1:{port}");
    Ok(stdout.contains("Type: http") && stdout.contains(&expected_url))
}

async fn remove_server(scope: &str, name: &str) {
    let _ = Command::new("claude")
        .args(["mcp", "remove", "--scope", scope, name])
        .output()
        .await;
}

/// Auto-configure Claude to use RestFlow MCP over HTTP transport.
///
/// Registers (or updates) the `restflow` MCP entry pointing to
/// `http://127.0.0.1:{port}` with `--transport http`.
/// Silently skips if Claude CLI is not installed.
pub async fn try_sync_claude_http_mcp(port: u16) -> Result<()> {
    if !is_claude_available().await {
        return Ok(());
    }

    if is_already_configured(port).await? {
        return Ok(());
    }

    remove_server("user", LEGACY_STDIO_SERVER_NAME).await;
    remove_server("local", LEGACY_STDIO_SERVER_NAME).await;
    remove_server("user", LEGACY_HTTP_SERVER_NAME).await;
    remove_server("local", LEGACY_HTTP_SERVER_NAME).await;
    remove_server("user", CLAUDE_MCP_SERVER_NAME).await;
    remove_server("local", CLAUDE_MCP_SERVER_NAME).await;

    let url = format!("http://127.0.0.1:{port}");
    let output = Command::new("claude")
        .args([
            "mcp",
            "add",
            "--scope",
            "user",
            "--transport",
            "http",
            CLAUDE_MCP_SERVER_NAME,
            &url,
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
