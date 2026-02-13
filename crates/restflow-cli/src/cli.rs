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

    /// Database path (defaults to ~/.local/share/restflow/restflow.db)
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

    /// Hook management
    Hook {
        #[command(subcommand)]
        command: HookCommands,
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

    /// Memory operations
    Memory {
        #[command(subcommand)]
        command: MemoryCommands,
    },

    /// Secret management
    Secret {
        #[command(subcommand)]
        command: SecretCommands,
    },

    /// API key management (simplified interface)
    Key {
        #[command(subcommand)]
        command: KeyCommands,
    },

    /// Authentication management
    Auth {
        #[command(subcommand)]
        command: AuthCommands,
    },

    /// Security management
    Security {
        #[command(subcommand)]
        command: SecurityCommands,
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

    /// Migrate configuration from old locations
    Migrate(MigrateArgs),

    /// MCP server management
    Mcp {
        #[command(subcommand)]
        command: McpCommands,
    },

    /// Show system information
    Info,

    /// Manage chat sessions
    Session {
        #[command(subcommand)]
        command: SessionCommands,
    },

    /// Workspace notes management
    Note {
        #[command(subcommand)]
        command: NoteCommands,
    },

    /// Telegram pairing / access control
    Pairing {
        #[command(subcommand)]
        command: PairingCommands,
    },

    /// Route binding (agent routing)
    Route {
        #[command(subcommand)]
        command: RouteCommands,
    },
}

#[derive(Args)]
pub struct MigrateArgs {
    /// Dry run - show what would be migrated without making changes
    #[arg(long)]
    pub dry_run: bool,

    /// Force migration even if target exists
    #[arg(long)]
    pub force: bool,
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
    fn parses_hook_list_command() {
        let cli = Cli::try_parse_from(["restflow", "hook", "list"]).expect("parse hook list");
        assert!(matches!(
            cli.command,
            Some(super::Commands::Hook {
                command: super::HookCommands::List
            })
        ));
    }

    #[test]
    fn rejects_task_commands() {
        let cli = Cli::try_parse_from(["restflow", "task", "list"]);
        assert!(cli.is_err());
        let cli = Cli::try_parse_from(["restflow", "background-agent", "list"]);
        assert!(cli.is_err());
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
    fn parses_note_list_command() {
        let cli = Cli::try_parse_from(["restflow", "note", "list", "--folder", "feature"])
            .expect("parse note list");
        assert!(matches!(
            cli.command,
            Some(super::Commands::Note {
                command: super::NoteCommands::List { .. }
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

        #[arg(short, long)]
        model: Option<String>,

        #[arg(long)]
        prompt: Option<String>,

        #[arg(long, value_enum)]
        codex_execution_mode: Option<CodexExecutionModeArg>,
    },

    /// Update agent
    Update {
        id: String,

        #[arg(short, long)]
        name: Option<String>,

        #[arg(short, long)]
        model: Option<String>,

        #[arg(long, value_enum)]
        codex_execution_mode: Option<CodexExecutionModeArg>,
    },

    /// Delete agent
    Delete { id: String },
}

#[derive(Subcommand)]
pub enum HookCommands {
    /// List hooks
    List,

    /// Create a hook quickly from CLI
    Create {
        #[arg(long)]
        name: String,

        /// One of: task_started, task_completed, task_failed, task_cancelled
        #[arg(long)]
        event: String,

        /// One of: webhook, script, send_message, run_task
        #[arg(long)]
        action: String,

        /// URL for webhook action
        #[arg(long)]
        url: Option<String>,

        /// Script path for script action
        #[arg(long)]
        script: Option<String>,

        /// Channel type for send_message action
        #[arg(long)]
        channel: Option<String>,

        /// Message template for send_message action
        #[arg(long)]
        message: Option<String>,

        /// Agent ID for run_task action
        #[arg(long)]
        agent: Option<String>,

        /// Input template for run_task action
        #[arg(long)]
        input: Option<String>,
    },

    /// Delete a hook
    Delete { id: String },

    /// Execute a hook with synthetic context
    Test { id: String },
}

#[derive(Subcommand)]
pub enum DaemonCommands {
    /// Start daemon
    Start {
        /// Run in foreground
        #[arg(long)]
        foreground: bool,

        /// MCP HTTP server port (default: 8787, MCP is always enabled)
        #[arg(long)]
        mcp_port: Option<u16>,
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

        /// MCP HTTP server port (default: 8787, MCP is always enabled)
        #[arg(long)]
        mcp_port: Option<u16>,
    },
}

#[derive(Subcommand)]
pub enum SkillCommands {
    /// List skills
    List,

    /// Show skill details
    Show { id: String },

    /// Create skill
    Create {
        #[arg(short, long)]
        name: String,
    },

    /// Delete skill
    Delete { id: String },

    /// Import skill from file
    Import { path: String },

    /// Export skill to file
    Export {
        id: String,

        #[arg(short, long)]
        output: Option<String>,
    },

    /// Search marketplace
    Search { query: String },

    /// Install a skill from marketplace, git, or local sources
    Install {
        /// Source: marketplace id, git URL, local path, or .skill package
        source: String,

        /// Subpath within a git repository
        #[arg(long)]
        path: Option<String>,

        /// Install scope (must be `user`)
        #[arg(long, default_value = "user")]
        scope: String,
    },
}

#[derive(Subcommand)]
pub enum MemoryCommands {
    /// Search memory
    Search { query: String },

    /// List memory chunks
    List {
        #[arg(long)]
        agent: Option<String>,

        #[arg(long)]
        tag: Option<String>,
    },

    /// Export memory
    Export {
        #[arg(long)]
        agent: Option<String>,

        #[arg(short, long)]
        output: Option<String>,
    },

    /// Show memory stats
    Stats,

    /// Clear memory
    Clear {
        #[arg(long)]
        agent: Option<String>,
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
pub enum KeyCommands {
    /// Add a new API key
    Add {
        /// Provider (anthropic, claude-code, openai, deepseek)
        provider: String,

        /// The API key value
        key: String,

        /// Optional display name
        #[arg(short, long)]
        name: Option<String>,
    },

    /// List all keys
    List {
        /// Filter by provider
        #[arg(short, long)]
        provider: Option<String>,
    },

    /// Show key details
    Show {
        /// Key ID (first 8 chars of profile ID)
        id: String,
    },

    /// Set key as default (highest priority)
    Use {
        /// Key ID
        id: String,
    },

    /// Remove a key
    Remove {
        /// Key ID
        id: String,
    },

    /// Test if a key works
    Test {
        /// Key ID
        id: String,
    },

    /// Auto-discover keys from environment and files
    Discover,
}

#[derive(Subcommand)]
pub enum AuthCommands {
    /// Show authentication status
    Status,

    /// Discover credentials from all sources
    Discover,

    /// List all credential profiles
    List,

    /// Show profile details
    Show { id: String },

    /// Add manual API key
    Add {
        #[arg(long)]
        provider: String,

        #[arg(long)]
        key: String,

        #[arg(long)]
        name: Option<String>,
    },

    /// Remove a profile
    Remove { id: String },
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

#[derive(Subcommand)]
pub enum SecurityCommands {
    /// List pending approvals
    Approvals,

    /// Approve a request
    Approve { id: String },

    /// Reject a request
    Reject { id: String },

    /// Manage allowlist
    Allowlist {
        #[command(subcommand)]
        action: AllowlistAction,
    },
}

#[derive(Subcommand)]
pub enum AllowlistAction {
    /// Show allowlist
    Show,

    /// Add allowlist pattern
    Add {
        pattern: String,

        #[arg(short, long)]
        description: Option<String>,
    },

    /// Remove allowlist pattern by index
    Remove { index: usize },
}

#[derive(Subcommand, Clone)]
pub enum McpCommands {
    /// List MCP servers
    List,

    /// Add MCP server
    Add { name: String, command: String },

    /// Remove MCP server
    Remove { name: String },

    /// Start MCP server
    Start { name: String },

    /// Stop MCP server
    Stop { name: String },

    /// Run the built-in MCP server over stdio
    Serve,
}

#[derive(Subcommand)]
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
        #[arg(long, default_value = "claude-cli")]
        agent: String,

        /// Model name
        #[arg(long, default_value = "claude-code")]
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
    },
}

#[derive(Subcommand)]
pub enum NoteCommands {
    /// List workspace notes
    List {
        #[arg(long)]
        folder: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        priority: Option<String>,
        #[arg(long)]
        tag: Option<String>,
        #[arg(long)]
        assignee: Option<String>,
        #[arg(long)]
        search: Option<String>,
    },
    /// List distinct note folders
    Folders,
    /// Show note detail
    Show { id: String },
    /// Create a note
    Create {
        #[arg(long)]
        folder: String,
        #[arg(long)]
        title: String,
        #[arg(long)]
        file: Option<String>,
        #[arg(long)]
        content: Option<String>,
        #[arg(long)]
        priority: Option<String>,
        #[arg(long, num_args = 0..)]
        tags: Vec<String>,
    },
    /// Update a note
    Update {
        id: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        priority: Option<String>,
        #[arg(long)]
        assignee: Option<String>,
        #[arg(long)]
        folder: Option<String>,
        #[arg(long)]
        file: Option<String>,
        #[arg(long)]
        content: Option<String>,
        #[arg(long, num_args = 0..)]
        tags: Option<Vec<String>>,
    },
    /// Delete a note
    Delete { id: String },
}

#[derive(Subcommand)]
pub enum PairingCommands {
    /// List pending requests and allowed peers
    List,

    /// Approve a pairing request by code
    Approve {
        /// The 8-character pairing code
        code: String,
    },

    /// Deny a pairing request by code
    Deny {
        /// The 8-character pairing code
        code: String,
    },

    /// Revoke an allowed peer
    Revoke {
        /// The peer ID to revoke
        peer_id: String,
    },
}

#[derive(Subcommand)]
pub enum RouteCommands {
    /// List all route bindings
    List,

    /// Bind a target to an agent
    Bind {
        /// Binding type: peer, group, or default
        #[arg(long)]
        peer: Option<String>,

        /// Group/chat ID
        #[arg(long)]
        group: Option<String>,

        /// Set as default agent
        #[arg(long)]
        default: bool,

        /// Agent ID to route to
        #[arg(long)]
        agent: String,
    },

    /// Remove a route binding
    Unbind {
        /// Binding ID
        id: String,
    },
}
