use crate::cli::StartArgs;
use crate::commands::claude_mcp::try_sync_claude_http_mcp;
use crate::commands::codex_mcp::try_sync_codex_http_mcp;
use anyhow::Result;
use restflow_core::daemon::ensure_daemon_running;

pub async fn run(args: StartArgs) -> Result<()> {
    let _ = args;
    ensure_daemon_running().await?;
    if let Err(err) = try_sync_claude_http_mcp(8787).await {
        eprintln!("Warning: failed to auto-configure Claude MCP: {err}");
    }
    if let Err(err) = try_sync_codex_http_mcp(8787).await {
        eprintln!("Warning: failed to auto-configure Codex MCP: {err}");
    }
    println!("RestFlow daemon started");
    Ok(())
}
