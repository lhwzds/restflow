mod cli;
mod completions;
mod config;
mod setup;
mod tui;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};
use restflow_core::paths;
use setup::prepare_core;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = config::CliConfig::load();
    config.apply_api_key_env();

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

    if let Some(Commands::Completions { shell }) = cli.command {
        completions::generate_completions(shell);
        return Ok(());
    }

    let core = prepare_core(&config).await?;
    tui::run(core, &config).await
}
