use anyhow::{Context, Result, bail};
use std::process::Output;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

const CLAUDE_MCP_SERVER_NAME: &str = "restflow";
const LEGACY_STDIO_SERVER_NAME: &str = "restflow-stdio";
const LEGACY_HTTP_SERVER_NAME: &str = "restflow-http";
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

async fn is_claude_available() -> bool {
    let mut command = Command::new("claude");
    command.arg("--version");
    match run_command_with_timeout(command, "claude --version", COMMAND_TIMEOUT).await {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

async fn is_already_configured(port: u16) -> Result<bool> {
    let mut command = Command::new("claude");
    command.args(["mcp", "get", CLAUDE_MCP_SERVER_NAME]);
    let output =
        run_command_with_timeout(command, "claude mcp get restflow", COMMAND_TIMEOUT).await?;

    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let expected_url = format!("http://127.0.0.1:{port}");
    Ok(stdout.contains("Type: http") && stdout.contains(&expected_url))
}

async fn remove_server(scope: &str, name: &str) {
    let mut command = Command::new("claude");
    command.args(["mcp", "remove", "--scope", scope, name]);
    let description = format!("claude mcp remove --scope {} {}", scope, name);
    let _ = run_command_with_timeout(command, &description, COMMAND_TIMEOUT).await;
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
    let mut command = Command::new("claude");
    command.args([
        "mcp",
        "add",
        "--scope",
        "user",
        "--transport",
        "http",
        CLAUDE_MCP_SERVER_NAME,
        &url,
    ]);
    let output =
        run_command_with_timeout(command, "claude mcp add restflow", COMMAND_TIMEOUT).await?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[tokio::test]
    async fn run_command_with_timeout_times_out() {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 1"]);
        let error = run_command_with_timeout(command, "sleep", Duration::from_millis(10)).await;

        assert!(error.is_err());
        assert!(error.unwrap_err().to_string().contains("timed out"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn run_command_with_timeout_returns_output() {
        let mut command = Command::new("sh");
        command.args(["-c", "printf ok"]);
        let output = run_command_with_timeout(command, "printf", Duration::from_secs(1))
            .await
            .unwrap();

        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout), "ok");
    }
}
