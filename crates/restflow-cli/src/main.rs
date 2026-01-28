mod config;
mod setup;
mod tui;

use anyhow::Result;
use restflow_core::paths;
use setup::prepare_core;

#[tokio::main]
async fn main() -> Result<()> {
    // Configure logging: always write to file (needed for TUI mode)
    let log_dir = paths::ensure_data_dir()?.join("logs");
    std::fs::create_dir_all(&log_dir).ok();

    let file_appender = tracing_appender::rolling::daily(log_dir, "restflow.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(false)
        .with_level(true)
        .init();

    // Always launch the TUI interface
    let core = prepare_core().await?;
    tui::run(core).await
}
