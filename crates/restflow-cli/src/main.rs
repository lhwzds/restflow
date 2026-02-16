mod cli;
mod commands;
mod completions;
mod config;
mod daemon;
mod error;
mod executor;
mod output;
mod setup;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use clap_complete::generate;
use cli::{Cli, Commands};
use restflow_core::paths;
use std::io;
use tracing_appender::non_blocking::WorkerGuard;

fn init_logging(verbose: bool) -> Option<WorkerGuard> {
    let level = if verbose { "debug" } else { "info" };

    if let Ok(base_dir) = paths::ensure_restflow_dir() {
        let log_dir = base_dir.join("logs");
        if std::fs::create_dir_all(&log_dir).is_ok() {
            let probe_path = log_dir.join(".write-probe");
            let probe_result = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&probe_path);

            if probe_result.is_ok() {
                let _ = std::fs::remove_file(&probe_path);
                let file_appender = tracing_appender::rolling::daily(log_dir, "restflow.log");
                let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
                tracing_subscriber::fmt()
                    .with_writer(non_blocking)
                    .with_ansi(false)
                    .with_target(false)
                    .with_level(true)
                    .with_env_filter(level)
                    .init();
                return Some(guard);
            }
        }
    }

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_target(false)
        .with_level(true)
        .with_env_filter(level)
        .init();
    None
}

fn command_needs_direct_core(command: &Option<Commands>) -> bool {
    matches!(
        command,
        Some(Commands::Daemon { .. })
            | Some(Commands::Hook { .. })
            | Some(Commands::Pairing { .. })
            | Some(Commands::Route { .. })
            | Some(Commands::Maintenance { .. })
    )
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        error::handle_error(err);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    let _config = config::CliConfig::load();
    let _log_guard = init_logging(cli.verbose);

    if let Some(Commands::Completions { shell }) = cli.command {
        let mut cmd = Cli::command();
        generate(shell, &mut cmd, "restflow", &mut io::stdout());
        return Ok(());
    }

    if let Some(Commands::Stop) = cli.command {
        commands::stop::run().await?;
        return Ok(());
    }

    if let Some(Commands::Status) = cli.command {
        commands::status::run(cli.format).await?;
        return Ok(());
    }

    if let Some(Commands::Start(args)) = &cli.command {
        commands::start::run(*args).await?;
        return Ok(());
    }

    if let Some(Commands::Upgrade(args)) = &cli.command {
        commands::upgrade::run(*args, cli.format).await?;
        return Ok(());
    }

    if let Some(Commands::Restart(args)) = &cli.command {
        commands::restart::run(*args).await?;
        return Ok(());
    }

    // Handle daemon commands that don't need AppCore (to avoid database lock conflicts)
    if let Some(Commands::Daemon { command }) = &cli.command
        && commands::daemon::run_without_core(command).await?
    {
        return Ok(());
    }

    if matches!(
        &cli.command,
        Some(Commands::Key { .. }) | Some(Commands::Auth { .. })
    ) {
        return match cli.command {
            Some(Commands::Key { command }) => commands::key::run(command, cli.format).await,
            Some(Commands::Auth { command }) => commands::auth::run(command, cli.format).await,
            _ => unreachable!(),
        };
    }

    if let Some(Commands::Mcp { command }) = &cli.command {
        return commands::mcp::run(command.clone(), cli.format).await;
    }

    // Commands that need direct core access (daemon, run, hook)
    // These bypass the executor pattern for now
    let needs_direct_core = command_needs_direct_core(&cli.command);

    let db_path = setup::resolve_db_path(cli.db_path.clone())?;

    if needs_direct_core {
        // Use direct core for commands that require it
        let core = setup::prepare_core(Some(db_path)).await?;

        match cli.command {
            Some(Commands::Daemon { command }) => commands::daemon::run(core, command).await,
            Some(Commands::Hook { command }) => {
                commands::hook::run(core, command, cli.format).await
            }
            Some(Commands::Pairing { command }) => {
                commands::pairing::run(core, command, cli.format).await
            }
            Some(Commands::Route { command }) => {
                commands::pairing::run_route(core, command, cli.format).await
            }
            Some(Commands::Maintenance { command }) => {
                commands::maintenance::run(core, command, cli.format).await
            }
            _ => unreachable!(),
        }
    } else {
        // Use executor for commands that support IPC
        let exec = executor::create(Some(db_path)).await?;

        match cli.command {
            Some(Commands::Agent { command }) => {
                commands::agent::run(exec, command, cli.format).await
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
            Some(Commands::Note { command }) => {
                commands::note::run(exec, command, cli.format).await
            }
            Some(Commands::Security { command }) => {
                commands::security::run(command, cli.format).await
            }
            Some(Commands::BackgroundAgent { command }) => {
                commands::background_agent::run(exec, command, cli.format).await
            }
            Some(Commands::Shared { command }) => {
                commands::shared::run(exec, command, cli.format).await
            }
            Some(Commands::Deliverable { command }) => {
                commands::deliverable::run(exec, command, cli.format).await
            }
            Some(Commands::Trigger { command }) => {
                commands::trigger::run(exec, command, cli.format).await
            }
            Some(Commands::Migrate(args)) => commands::migrate::run(args).await,
            Some(Commands::Info) => commands::info::run(),
            Some(Commands::Completions { .. }) => Ok(()),
            Some(Commands::Stop) => Ok(()),
            Some(Commands::Status) => Ok(()),
            Some(Commands::Upgrade(_)) => Ok(()),
            Some(Commands::Restart(_)) => Ok(()),
            None => {
                Cli::command().print_help()?;
                Ok(())
            }
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::command_needs_direct_core;
    use crate::cli::{Commands, HookCommands, MaintenanceCommands, StartArgs};

    #[test]
    fn start_does_not_need_direct_core() {
        let command = Some(Commands::Start(StartArgs::default()));
        assert!(!command_needs_direct_core(&command));
    }

    #[test]
    fn hook_needs_direct_core() {
        let command = Some(Commands::Hook {
            command: HookCommands::List,
        });
        assert!(command_needs_direct_core(&command));
    }

    #[test]
    fn maintenance_needs_direct_core() {
        let command = Some(Commands::Maintenance {
            command: MaintenanceCommands::Cleanup,
        });
        assert!(command_needs_direct_core(&command));
    }
}
