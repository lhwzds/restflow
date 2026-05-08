use anyhow::{Context, Result, bail};
use std::process::Output;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

const CODEX_MCP_SERVER_NAME: &str = "restflow";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(3);

async fn run_command_with_timeout(
    mut command: Command,
    description: &str,
    timeout_duration: Duration,
) -> Result<Output> {
    command.kill_on_drop(true);
    timeout(timeout_duration, command.output())
        .await
        .with_context(|| {
            format!(
                "command '{}' timed out after {}s",
                description,
                timeout_duration.as_secs()
            )
        })?
        .with_context(|| format!("command '{}' failed to execute", description))
}

async fn is_codex_available() -> bool {
    let mut command = Command::new("codex");
    command.arg("--version");
    match run_command_with_timeout(command, "codex --version", COMMAND_TIMEOUT).await {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

async fn is_already_configured(port: u16) -> Result<bool> {
    let mut command = Command::new("codex");
    command.args(["mcp", "get", CODEX_MCP_SERVER_NAME, "--json"]);
    let output =
        run_command_with_timeout(command, "codex mcp get restflow --json", COMMAND_TIMEOUT).await?;

    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let expected_url = format!("http://127.0.0.1:{port}");
    Ok(stdout.contains("streamable_http") && stdout.contains(&expected_url))
}

async fn remove_server() {
    let mut command = Command::new("codex");
    command.args(["mcp", "remove", CODEX_MCP_SERVER_NAME]);
    let _ = run_command_with_timeout(command, "codex mcp remove restflow", COMMAND_TIMEOUT).await;
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
    let mut command = Command::new("codex");
    command.args(["mcp", "add", CODEX_MCP_SERVER_NAME, "--url", &url]);
    let output =
        run_command_with_timeout(command, "codex mcp add restflow --url", COMMAND_TIMEOUT).await?;

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
