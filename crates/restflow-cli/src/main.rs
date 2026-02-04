mod cli;
mod commands;
mod completions;
mod config;
mod daemon;
mod executor;
mod output;
mod setup;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use clap_complete::generate;
use cli::{Cli, Commands, DaemonCommands};
use restflow_core::daemon::{check_daemon_status, start_daemon, stop_daemon, DaemonStatus};
use restflow_core::paths;
use std::io;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let _config = config::CliConfig::load();

    // Configure logging: always write to file
    let log_dir = paths::ensure_restflow_dir()?.join("logs");
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

    // Handle daemon commands that don't need AppCore (to avoid database lock conflicts)
    if let Some(Commands::Daemon { command }) = &cli.command {
        match command {
            DaemonCommands::Start { foreground: false } => {
                match check_daemon_status()? {
                    DaemonStatus::Running { pid } => {
                        println!("Daemon already running (PID: {})", pid);
                    }
                    _ => {
                        let pid = start_daemon()?;
                        println!("Daemon started (PID: {})", pid);
                    }
                }
                return Ok(());
            }
            DaemonCommands::Stop => {
                if stop_daemon()? {
                    println!("Sent stop signal to daemon");
                } else {
                    println!("Daemon not running");
                }
                return Ok(());
            }
            DaemonCommands::Status => {
                match check_daemon_status()? {
                    DaemonStatus::Running { pid } => {
                        println!("Daemon running (PID: {})", pid);
                    }
                    DaemonStatus::NotRunning => {
                        println!("Daemon not running");
                    }
                    DaemonStatus::Stale { pid } => {
                        println!("Daemon not running (stale PID: {})", pid);
                    }
                }
                return Ok(());
            }
            DaemonCommands::Start { foreground: true } => {
                // Continue to open database for foreground mode
            }
        }
    }

    // Commands that need direct core access (daemon, mcp, key, claude, codex, run, start)
    // These bypass the executor pattern for now
    let needs_direct_core = matches!(
        &cli.command,
        Some(Commands::Daemon { .. })
            | Some(Commands::Mcp { .. })
            | Some(Commands::Key { .. })
            | Some(Commands::Claude(_))
            | Some(Commands::Codex(_))
            | Some(Commands::Run(_))
            | Some(Commands::Start(_))
    );

    let db_path = setup::resolve_db_path(cli.db_path.clone())?;

    if needs_direct_core {
        // Use direct core for commands that require it
        let core = setup::prepare_core(Some(db_path)).await?;

        match cli.command {
            Some(Commands::Run(args)) => commands::run::run(core, args, cli.format).await,
            Some(Commands::Start(args)) => commands::start::run(args).await,
            Some(Commands::Daemon { command }) => commands::daemon::run(core, command).await,
            Some(Commands::Key { command }) => commands::key::run(core, command, cli.format).await,
            Some(Commands::Mcp { command }) => commands::mcp::run(core, command, cli.format).await,
            Some(Commands::Claude(args)) => commands::claude::run(core, args, cli.format).await,
            Some(Commands::Codex(args)) => commands::codex::run(core, args, cli.format).await,
            _ => unreachable!(),
        }
    } else {
        // Use executor for commands that support IPC
        let exec = executor::create(Some(db_path)).await?;

        match cli.command {
            Some(Commands::Agent { command }) => {
                commands::agent::run(exec, command, cli.format).await
            }
            Some(Commands::Task { command }) => {
                commands::task::run(exec, command, cli.format).await
            }
            Some(Commands::Skill { command }) => {
                commands::skill::run(exec, command, cli.format).await
            }
            Some(Commands::Memory { command }) => {
                commands::memory::run(exec, command, cli.format).await
            }
            Some(Commands::Secret { command }) => {
                commands::secret::run(exec, command, cli.format).await
            }
            Some(Commands::Config { command }) => {
                commands::config::run(exec, command, cli.format).await
            }
            Some(Commands::Session { command }) => {
                commands::session::run(exec, command, cli.format).await
            }
            Some(Commands::Auth { command }) => commands::auth::run(command, cli.format).await,
            Some(Commands::Security { command }) => {
                commands::security::run(command, cli.format).await
            }
            Some(Commands::Migrate(args)) => commands::migrate::run(args).await,
            Some(Commands::Info) => commands::info::run(),
            Some(Commands::Completions { .. }) => Ok(()),
            None => {
                Cli::command().print_help()?;
                Ok(())
            }
            _ => unreachable!(),
        }
    }
}
