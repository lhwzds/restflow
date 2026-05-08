use anyhow::Result;
use runtime::daemon::stop_daemon;

pub async fn run() -> Result<()> {
    if stop_daemon()? {
        println!("RestFlow daemon stopped");
    } else {
        println!("RestFlow daemon not running");
    }
    Ok(())
}
