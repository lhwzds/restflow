mod cli;
mod commands;
mod config;
mod daemon;
mod error;
mod executor;
mod output;
mod setup;
#[cfg(test)]
mod test_support;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use clap_complete::generate;
use cli::{Cli, Commands};
use commands::task as task_commands;
use std::io::IsTerminal;
use restflow_core::paths;
use std::io;
use restflow_tui::{TuiLaunchOptions, run_tui};
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
    matches!(command, Some(Commands::Daemon { .. }))
}

fn command_uses_daemon_executor(command: &Option<Commands>) -> bool {
    !command_needs_direct_core(command)
}

fn executor_db_path_flag(raw_db_path: Option<String>, needs_direct_core: bool) -> Option<String> {
    if needs_direct_core { None } else { raw_db_path }
}

fn should_launch_tui_by_default(
    command: &Option<Commands>,
    stdin_is_tty: bool,
    stdout_is_tty: bool,
) -> bool {
    command.is_none() && stdin_is_tty && stdout_is_tty
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

    if should_launch_tui_by_default(
        &cli.command,
        io::stdin().is_terminal(),
        io::stdout().is_terminal(),
    ) {
        if cli.db_path.is_some() {
            anyhow::bail!(
                "The --db-path flag is not supported for the interactive TUI. Start the daemon against the desired database first."
            );
        }
        return run_tui(TuiLaunchOptions::default()).await;
    }

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

    // Commands that need direct core access.
    let needs_direct_core = command_needs_direct_core(&cli.command);

    if !command_uses_daemon_executor(&cli.command) {
        // Use direct core for commands that require it
        let db_path = setup::resolve_db_path(cli.db_path.clone())?;
        let core = setup::prepare_core(Some(db_path)).await?;

        match cli.command {
            Some(Commands::Daemon { command }) => commands::daemon::run(core, command).await,
            _ => unreachable!(),
        }
    } else {
        // Production CLI commands route through the daemon-backed executor unless they
        // explicitly require direct core access for daemon lifecycle operations.
        let exec = executor::create(executor_db_path_flag(
            cli.db_path.clone(),
            needs_direct_core,
        ))
        .await?;

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
            Some(Commands::Hook { command }) => {
                commands::hook::run(exec, command, cli.format).await
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
            Some(Commands::Pairing { command }) => {
                commands::pairing::run(exec, command, cli.format).await
            }
            Some(Commands::Route { command }) => {
                commands::pairing::run_route(exec, command, cli.format).await
            }
            Some(Commands::Maintenance { command }) => {
                commands::maintenance::run(exec, command, cli.format).await
            }
            Some(Commands::Security { command }) => {
                commands::security::run(command, cli.format).await
            }
            Some(Commands::Task { command }) => task_commands::run(exec, command, cli.format).await,
            Some(Commands::Team { command }) => {
                commands::team::run(exec, command, cli.format).await
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
    use super::{
        command_needs_direct_core, command_uses_daemon_executor, executor_db_path_flag,
        should_launch_tui_by_default,
    };
    use crate::cli::{
        Commands, HookCommands, MaintenanceCommands, PairingCommands, RouteCommands, StartArgs,
    };

    fn hook_command(command: HookCommands) -> Option<Commands> {
        Some(Commands::Hook { command })
    }

    #[test]
    fn start_does_not_need_direct_core() {
        let command = Some(Commands::Start(StartArgs::default()));
        assert!(!command_needs_direct_core(&command));
    }

    #[test]
    fn hook_does_not_need_direct_core() {
        let command = hook_command(HookCommands::List);
        assert!(!command_needs_direct_core(&command));
    }

    #[test]
    fn default_tui_launch_requires_no_command_and_tty() {
        assert!(should_launch_tui_by_default(&None, true, true));
        assert!(!should_launch_tui_by_default(&None, true, false));
        assert!(!should_launch_tui_by_default(&Some(Commands::Info), true, true));
    }

    #[test]
    fn hook_list_uses_daemon_executor() {
        let command = hook_command(HookCommands::List);
        assert!(command_uses_daemon_executor(&command));
    }

    #[test]
    fn hook_create_uses_daemon_executor() {
        let command = hook_command(HookCommands::Create {
            name: "notify".to_string(),
            event: "task_started".to_string(),
            action: "webhook".to_string(),
            url: Some("https://example.com/hook".to_string()),
            script: None,
            channel: None,
            message: None,
            agent: None,
            input: None,
        });
        assert!(command_uses_daemon_executor(&command));
    }

    #[test]
    fn hook_update_uses_daemon_executor() {
        let command = hook_command(HookCommands::Update {
            id: "hook-123".to_string(),
            name: "notify".to_string(),
            event: "task_started".to_string(),
            action: "webhook".to_string(),
            url: Some("https://example.com/hook".to_string()),
            script: None,
            channel: None,
            message: None,
            agent: None,
            input: None,
        });
        assert!(command_uses_daemon_executor(&command));
    }

    #[test]
    fn hook_delete_uses_daemon_executor() {
        let command = hook_command(HookCommands::Delete {
            id: "hook-123".to_string(),
        });
        assert!(command_uses_daemon_executor(&command));
    }

    #[test]
    fn task_command_module_is_available() {
        let _ = crate::commands::task::run;
    }

    #[test]
    fn hook_test_uses_daemon_executor() {
        let command = hook_command(HookCommands::Test {
            id: "hook-123".to_string(),
        });
        assert!(command_uses_daemon_executor(&command));
    }

    #[test]
    fn maintenance_does_not_need_direct_core() {
        let command = Some(Commands::Maintenance {
            command: MaintenanceCommands::Cleanup,
        });
        assert!(!command_needs_direct_core(&command));
    }

    #[test]
    fn pairing_does_not_need_direct_core() {
        let command = Some(Commands::Pairing {
            command: PairingCommands::List,
        });
        assert!(!command_needs_direct_core(&command));
    }

    #[test]
    fn route_does_not_need_direct_core() {
        let command = Some(Commands::Route {
            command: RouteCommands::List,
        });
        assert!(!command_needs_direct_core(&command));
    }

    #[test]
    fn executor_db_path_flag_drops_default_path_for_direct_core_commands() {
        assert_eq!(
            executor_db_path_flag(Some("/tmp/restflow.db".to_string()), true),
            None
        );
    }

    #[test]
    fn executor_db_path_flag_preserves_explicit_flag_for_daemon_routed_commands() {
        assert_eq!(
            executor_db_path_flag(Some("/tmp/restflow.db".to_string()), false),
            Some("/tmp/restflow.db".to_string())
        );
    }
}
