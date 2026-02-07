use crate::cli::StartArgs;
use crate::commands::claude_mcp::try_sync_restflow_stdio_mcp;
use anyhow::Result;
use restflow_core::daemon::ensure_daemon_running;

pub async fn run(args: StartArgs) -> Result<()> {
    let _ = args;
    ensure_daemon_running().await?;
    if let Err(err) = try_sync_restflow_stdio_mcp().await {
        eprintln!("Warning: failed to auto-configure Claude MCP: {err}");
    }
    println!("RestFlow daemon started");
    Ok(())
}
