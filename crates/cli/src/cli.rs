use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

/// Output format for CLI commands
#[derive(ValueEnum, Clone, Copy, Debug, Default)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum CodexExecutionModeArg {
    Safe,
    Bypass,
}

impl OutputFormat {
    #[allow(dead_code)]
    pub fn is_json(self) -> bool {
        matches!(self, OutputFormat::Json)
    }
}

#[derive(Parser)]
#[command(name = "restflow")]
#[command(version, about = "RestFlow - AI Agent Workflow Automation")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Database path for daemon lifecycle commands
    #[arg(long, global = true, env = "RESTFLOW_DB_PATH")]
    pub db_path: Option<String>,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Output format
    #[arg(long, global = true, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Generate shell completions
    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },

    /// Start RestFlow daemon
    Start(StartArgs),

    /// Stop RestFlow daemon
    Stop,

    /// Show RestFlow status
    Status,

    /// Restart RestFlow daemon
    Restart(RestartArgs),

    /// Upgrade RestFlow CLI to the latest release
    Upgrade(UpgradeArgs),

    /// Agent management
    Agent {
        #[command(subcommand)]
        command: AgentCommands,
    },

    /// Daemon management
    Daemon {
        #[command(subcommand)]
        command: DaemonCommands,
    },

    /// Skill management
    Skill {
        #[command(subcommand)]
        command: SkillCommands,
    },

    /// Secret management
    Secret {
        #[command(subcommand)]
        command: SecretCommands,
    },

    /// Configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },

    /// Maintenance operations
    Maintenance {
        #[command(subcommand)]
        command: MaintenanceCommands,
    },

    /// Show system information
    Info,

    /// Manage chat sessions
    Session {
        #[command(subcommand)]
        command: SessionCommands,
    },
}

#[derive(Args, Default, Clone, Copy)]
pub struct StartArgs {}

#[derive(Args, Default, Clone, Copy)]
pub struct RestartArgs {}

#[derive(Args, Clone, Copy, Default)]
pub struct UpgradeArgs {
    /// Reinstall even if the current version is already the latest
    #[arg(long)]
    pub force: bool,
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::Parser;

    #[test]
    fn parses_start_command() {
        let cli = Cli::try_parse_from(["restflow", "start"]).expect("parse start");
        assert!(matches!(cli.command, Some(super::Commands::Start(_))));
    }

    #[test]
    fn parses_stop_command() {
        let cli = Cli::try_parse_from(["restflow", "stop"]).expect("parse stop");
        assert!(matches!(cli.command, Some(super::Commands::Stop)));
    }

    #[test]
    fn parses_status_command() {
        let cli = Cli::try_parse_from(["restflow", "status"]).expect("parse status");
        assert!(matches!(cli.command, Some(super::Commands::Status)));
    }

    #[test]
    fn parses_restart_command() {
        let cli = Cli::try_parse_from(["restflow", "restart"]).expect("parse restart");
        assert!(matches!(cli.command, Some(super::Commands::Restart(_))));
    }

    #[test]
    fn parses_upgrade_command() {
        let cli = Cli::try_parse_from(["restflow", "upgrade"]).expect("parse upgrade");
        assert!(matches!(cli.command, Some(super::Commands::Upgrade(_))));
    }

    #[test]
    fn parses_session_list_command() {
        let cli = Cli::try_parse_from(["restflow", "session", "list"]).expect("parse session list");
        assert!(matches!(
            cli.command,
            Some(super::Commands::Session {
                command: super::SessionCommands::List
            })
        ));
    }

    #[test]
    fn parses_daemon_restart_command() {
        let cli =
            Cli::try_parse_from(["restflow", "daemon", "restart"]).expect("parse daemon restart");
        assert!(matches!(
            cli.command,
            Some(super::Commands::Daemon {
                command: super::DaemonCommands::Restart { .. }
            })
        ));
    }

    #[test]
    fn parses_daemon_start_command() {
        let cli = Cli::try_parse_from(["restflow", "daemon", "start"]).expect("parse daemon start");
        assert!(matches!(
            cli.command,
            Some(super::Commands::Daemon {
                command: super::DaemonCommands::Start { foreground: false }
            })
        ));
    }

    #[test]
    fn parses_daemon_stop_command() {
        let cli = Cli::try_parse_from(["restflow", "daemon", "stop"]).expect("parse daemon stop");
        assert!(matches!(
            cli.command,
            Some(super::Commands::Daemon {
                command: super::DaemonCommands::Stop
            })
        ));
    }

    #[test]
    fn parses_daemon_status_command() {
        let cli =
            Cli::try_parse_from(["restflow", "daemon", "status"]).expect("parse daemon status");
        assert!(matches!(
            cli.command,
            Some(super::Commands::Daemon {
                command: super::DaemonCommands::Status
            })
        ));
    }

    #[test]
    fn parses_daemon_restart_with_foreground() {
        let cli = Cli::try_parse_from(["restflow", "daemon", "restart", "--foreground"])
            .expect("parse daemon restart options");
        assert!(matches!(
            cli.command,
            Some(super::Commands::Daemon {
                command: super::DaemonCommands::Restart { foreground: true }
            })
        ));
    }

    #[test]
    fn rejects_agent_exec_command() {
        let cli = Cli::try_parse_from(["restflow", "agent", "exec", "agent-1"]);
        assert!(cli.is_err());
    }

    #[test]
    fn parses_agent_codex_execution_mode() {
        let cli = Cli::try_parse_from([
            "restflow",
            "agent",
            "create",
            "--name",
            "agent-1",
            "--codex-execution-mode",
            "bypass",
        ])
        .expect("parse agent codex execution mode");

        assert!(matches!(
            cli.command,
            Some(super::Commands::Agent {
                command: super::AgentCommands::Create {
                    codex_execution_mode: Some(super::CodexExecutionModeArg::Bypass),
                    ..
                }
            })
        ));
    }

    #[test]
    fn parses_maintenance_cleanup_command() {
        let cli =
            Cli::try_parse_from(["restflow", "maintenance", "cleanup"]).expect("parse cleanup");
        assert!(matches!(
            cli.command,
            Some(super::Commands::Maintenance {
                command: super::MaintenanceCommands::Cleanup
            })
        ));
    }
}

#[derive(Subcommand)]
pub enum AgentCommands {
    /// List all agents
    List,

    /// Show agent details
    Show { id: String },

    /// Create new agent
    Create {
        #[arg(short, long)]
        name: String,

        #[arg(long, hide = true)]
        provider: Option<String>,

        #[arg(short, long, hide = true)]
        model: Option<String>,

        #[arg(long)]
        prompt: Option<String>,

        #[arg(long, value_enum, hide = true)]
        codex_execution_mode: Option<CodexExecutionModeArg>,

        #[arg(long, hide = true)]
        codex_reasoning_effort: Option<String>,
    },

    /// Update agent
    Update {
        id: String,

        #[arg(short, long)]
        name: Option<String>,

        #[arg(long, hide = true)]
        provider: Option<String>,

        #[arg(short, long, hide = true)]
        model: Option<String>,

        #[arg(long, value_enum, hide = true)]
        codex_execution_mode: Option<CodexExecutionModeArg>,

        #[arg(long, hide = true)]
        codex_reasoning_effort: Option<String>,
    },

    /// Delete agent
    Delete { id: String },
}

#[derive(Subcommand)]
pub enum DaemonCommands {
    /// Start daemon
    Start {
        /// Run in foreground
        #[arg(long)]
        foreground: bool,
    },

    /// Stop daemon
    Stop,

    /// Show daemon status
    Status,

    /// Restart daemon
    Restart {
        /// Run in foreground
        #[arg(long)]
        foreground: bool,
    },
}

#[derive(Subcommand)]
pub enum SkillCommands {
    /// List skills
    List,

    /// Show skill details
    Show { id: String },

    /// Export skill to file
    #[command(hide = true)]
    Export {
        id: String,

        #[arg(short, long)]
        output: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum SecretCommands {
    /// List secrets
    List,

    /// Set secret
    Set { key: String, value: String },

    /// Delete secret
    Delete { key: String },

    /// Check if secret exists
    Has { key: String },
}

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Show configuration
    Show,

    /// Get config value
    Get { key: String },

    /// Set config value
    Set { key: String, value: String },

    /// Reset configuration to defaults
    Reset,
}

#[derive(Subcommand)]
pub enum MaintenanceCommands {
    /// Run storage cleanup immediately
    Cleanup,
}

#[derive(Subcommand, Clone)]
pub enum SessionCommands {
    /// List all sessions
    List,

    /// Show a session's conversation
    Show {
        /// Session ID
        id: String,
    },

    /// Create a new session
    Create {
        /// Agent ID to associate with
        #[arg(long, default_value = "default")]
        agent: String,

        /// Model name
        #[arg(long, default_value = "gpt-5.4")]
        model: String,
    },

    /// Delete a session
    Delete {
        /// Session ID
        id: String,
    },

    /// Search across sessions
    Search {
        /// Search query
        query: String,

        /// Agent ID to filter by
        #[arg(long)]
        agent: Option<String>,

        /// Maximum number of matching sessions to return
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },
}
