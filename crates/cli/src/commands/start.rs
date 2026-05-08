use crate::cli::StartArgs;
use anyhow::Result;
use runtime::daemon::ensure_daemon_running;

pub async fn run(args: StartArgs) -> Result<()> {
    let _ = args;
    ensure_daemon_running().await?;
    println!("RestFlow daemon started");
    Ok(())
}
