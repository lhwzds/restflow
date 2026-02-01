mod cli;
mod commands;
mod completions;
mod config;
mod daemon;
mod output;
mod setup;
mod tui;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use clap_complete::generate;
use cli::{Cli, Commands};
use restflow_core::paths;
use std::io;

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

    let level = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(false)
        .with_level(true)
        .with_env_filter(level)
        .init();

    if let Some(Commands::Completions { shell }) = cli.command {
        let mut cmd = Cli::command();
        generate(shell, &mut cmd, "restflow", &mut io::stdout());
        return Ok(());
    }

    let db_path = cli
        .db_path
        .clone()
        .or_else(|| config.default.db_path.clone());
    let core = setup::prepare_core(db_path).await?;

    match cli.command {
        Some(Commands::Chat(args)) => commands::chat::run(core, args).await,
        Some(Commands::Run(args)) => commands::run::run(core, args, cli.format).await,
        Some(Commands::Agent { command }) => commands::agent::run(core, command, cli.format).await,
        Some(Commands::Task { command }) => commands::task::run(core, command, cli.format).await,
        Some(Commands::Daemon { command }) => commands::daemon::run(core, command).await,
        Some(Commands::Skill { command }) => commands::skill::run(core, command, cli.format).await,
        Some(Commands::Memory { command }) => {
            commands::memory::run(core, command, cli.format).await
        }
        Some(Commands::Secret { command }) => {
            commands::secret::run(core, command, cli.format).await
        }
        Some(Commands::Auth { command }) => commands::auth::run(command, cli.format).await,
        Some(Commands::Security { command }) => commands::security::run(command, cli.format).await,
        Some(Commands::Config { command }) => {
            commands::config::run(core, command, cli.format).await
        }
        Some(Commands::Session { command }) => {
            commands::session::run(core, command, cli.format).await
        }
        Some(Commands::Mcp { command }) => commands::mcp::run(core, command, cli.format).await,
        Some(Commands::Info) => commands::info::run(),
        Some(Commands::Claude(args)) => commands::claude::run(core, args, cli.format).await,
        None => commands::chat::run(core, Default::default()).await,
        Some(Commands::Completions { .. }) => Ok(()),
    }
}
