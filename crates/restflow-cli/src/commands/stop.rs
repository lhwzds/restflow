use anyhow::Result;
use restflow_core::daemon::stop_daemon;

pub async fn run() -> Result<()> {
    if stop_daemon()? {
        println!("RestFlow daemon stopped");
    } else {
        println!("RestFlow daemon not running");
    }
    Ok(())
}
