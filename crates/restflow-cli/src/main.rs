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
use restflow_core::daemon::{
    DaemonConfig, DaemonStatus, check_daemon_status, start_daemon, start_daemon_with_config,
    stop_daemon,
};
use restflow_core::paths;
use std::io;
use tokio::time::{Duration, sleep};

fn command_needs_direct_core(command: &Option<Commands>) -> bool {
    matches!(
        command,
        Some(Commands::Daemon { .. })
            | Some(Commands::Codex(_))
            | Some(Commands::Run(_))
            | Some(Commands::Hook { .. })
    )
}

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
    if let Some(Commands::Daemon { command }) = &cli.command {
        match command {
            DaemonCommands::Start {
                foreground: false, ..
            } => {
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
            DaemonCommands::Restart {
                foreground: false,
                http,
                port,
                mcp,
                mcp_port,
            } => {
                let was_running = stop_daemon()?;
                if was_running {
                    println!("Sent stop signal to daemon");
                    wait_for_daemon_exit().await?;
                }

                let config = DaemonConfig {
                    http: *http,
                    http_port: *port,
                    mcp: *mcp,
                    mcp_port: *mcp_port,
                };
                let pid = start_daemon_with_config(config)?;
                if was_running {
                    println!("Daemon restarted (PID: {})", pid);
                } else {
                    println!("Daemon started (PID: {})", pid);
                }
                return Ok(());
            }
            DaemonCommands::Start {
                foreground: true, ..
            }
            | DaemonCommands::Restart {
                foreground: true, ..
            } => {
                // Continue to open database for foreground mode
            }
        }
    }

    if matches!(
        &cli.command,
        Some(Commands::Key { .. }) | Some(Commands::Claude(_)) | Some(Commands::Auth { .. })
    ) {
        return match cli.command {
            Some(Commands::Key { command }) => commands::key::run(command, cli.format).await,
            Some(Commands::Claude(args)) => commands::claude::run(args, cli.format).await,
            Some(Commands::Auth { command }) => commands::auth::run(command, cli.format).await,
            _ => unreachable!(),
        };
    }

    if let Some(Commands::Mcp { command }) = &cli.command {
        return commands::mcp::run(command.clone(), cli.format).await;
    }

    // Commands that need direct core access (daemon, codex, run)
    // These bypass the executor pattern for now
    let needs_direct_core = command_needs_direct_core(&cli.command);

    let db_path = setup::resolve_db_path(cli.db_path.clone())?;

    if needs_direct_core {
        // Use direct core for commands that require it
        let core = setup::prepare_core(Some(db_path)).await?;

        match cli.command {
            Some(Commands::Run(args)) => commands::run::run(core, args, cli.format).await,
            Some(Commands::Daemon { command }) => commands::daemon::run(core, command).await,
            Some(Commands::Codex(args)) => commands::codex::run(core, args, cli.format).await,
            Some(Commands::Hook { command }) => {
                commands::hook::run(core, command, cli.format).await
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
            Some(Commands::Security { command }) => {
                commands::security::run(command, cli.format).await
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

async fn wait_for_daemon_exit() -> Result<()> {
    for _ in 0..50 {
        match check_daemon_status()? {
            DaemonStatus::Running { .. } => sleep(Duration::from_millis(100)).await,
            DaemonStatus::NotRunning | DaemonStatus::Stale { .. } => return Ok(()),
        }
    }

    anyhow::bail!("Daemon did not stop within timeout")
}

#[cfg(test)]
mod tests {
    use super::command_needs_direct_core;
    use crate::cli::{CodexArgs, Commands, HookCommands, RunArgs, StartArgs};

    #[test]
    fn start_does_not_need_direct_core() {
        let command = Some(Commands::Start(StartArgs::default()));
        assert!(!command_needs_direct_core(&command));
    }

    #[test]
    fn run_needs_direct_core() {
        let command = Some(Commands::Run(RunArgs {
            agent_id: "agent-1".to_string(),
            input: None,
            background: false,
            stream: false,
        }));
        assert!(command_needs_direct_core(&command));
    }

    #[test]
    fn codex_needs_direct_core() {
        let command = Some(Commands::Codex(CodexArgs {
            prompt: None,
            model: "gpt-5".to_string(),
            session: None,
            cwd: None,
            timeout: 300,
        }));
        assert!(command_needs_direct_core(&command));
    }

    #[test]
    fn hook_needs_direct_core() {
        let command = Some(Commands::Hook {
            command: HookCommands::List,
        });
        assert!(command_needs_direct_core(&command));
    }
}
