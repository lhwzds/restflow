use crate::cli::RestartArgs;
use crate::commands::daemon::restart_background;
use anyhow::Result;

pub async fn run(_args: RestartArgs) -> Result<()> {
    restart_background(None).await
}
