mod cli;
mod commands;
mod config;
mod output;
mod setup;
mod tui;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};
use restflow_core::paths;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Configure logging: always write to file (needed for TUI mode)
    let log_dir = paths::ensure_data_dir()?.join("logs");
    std::fs::create_dir_all(&log_dir).ok();

    let file_appender = tracing_appender::rolling::daily(log_dir, "restflow.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let level = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(false)
        .with_level(true)
        .with_env_filter(level)
        .init();

    let core = setup::prepare_core(cli.db_path.clone()).await?;

    match cli.command {
        Some(Commands::Chat(args)) => commands::chat::run(core, args).await,
        Some(Commands::Run(args)) => commands::run::run(core, args, cli.format).await,
        Some(Commands::Agent { command }) => {
            commands::agent::run(core, command, cli.format).await
        }
        Some(Commands::Task { command }) => commands::task::run(core, command, cli.format).await,
        Some(Commands::Skill { command }) => {
            commands::skill::run(core, command, cli.format).await
        }
        Some(Commands::Memory { command }) => {
            commands::memory::run(core, command, cli.format).await
        }
        Some(Commands::Secret { command }) => {
            commands::secret::run(core, command, cli.format).await
        }
        Some(Commands::Config { command }) => {
            commands::config::run(core, command, cli.format).await
        }
        Some(Commands::Mcp) => commands::mcp::run(core).await,
        Some(Commands::Info) => commands::info::run(),
        None => commands::chat::run(core, Default::default()).await,
    }
}
