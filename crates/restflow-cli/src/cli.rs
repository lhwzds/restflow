use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

/// Output format for CLI commands
#[derive(ValueEnum, Clone, Copy, Debug, Default)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
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

    /// Run an agent directly
    Run(RunArgs),

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

    /// Task management
    Task {
        #[command(subcommand)]
        command: TaskCommands,
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

    /// Migrate configuration from old locations
    Migrate(MigrateArgs),

    /// MCP server management
    Mcp {
        #[command(subcommand)]
        command: McpCommands,
    },

    /// Show system information
    Info,

    /// Execute via Claude Code CLI (uses OAuth)
    Claude(ClaudeArgs),

    /// Execute via OpenAI Codex CLI
    Codex(CodexArgs),

    /// Manage chat sessions
    Session {
        #[command(subcommand)]
        command: SessionCommands,
    },
}

#[derive(Args)]
pub struct ClaudeArgs {
    /// Prompt to send to Claude
    #[arg(short, long)]
    pub prompt: Option<String>,

    /// Model to use (opus, sonnet, haiku)
    #[arg(short, long, default_value = "sonnet")]
    pub model: String,

    /// Session ID for conversation persistence
    #[arg(short = 's', long)]
    pub session: Option<String>,

    /// Create a new session and use it
    #[arg(long)]
    pub new_session: bool,

    /// Working directory
    #[arg(short = 'w', long)]
    pub cwd: Option<String>,

    /// Timeout in seconds
    #[arg(long, default_value = "300")]
    pub timeout: u64,

    /// Enable Playwright browser tools via MCP
    #[arg(long)]
    pub browser: bool,

    /// Run Playwright in headless mode (use --headless=false for headed)
    #[arg(long, default_value_t = true)]
    pub headless: bool,

    /// Set Playwright viewport size, e.g. 1280x720
    #[arg(long)]
    pub viewport: Option<String>,

    /// Auth profile ID to use (defaults to first available Anthropic profile)
    #[arg(long)]
    pub auth_profile: Option<String>,
}

#[derive(Args)]
pub struct CodexArgs {
    /// Prompt to send to Codex
    #[arg(short, long)]
    pub prompt: Option<String>,

    /// Model to use
    #[arg(short, long, default_value = "gpt-5")]
    pub model: String,

    /// Session ID for conversation persistence
    #[arg(short = 's', long)]
    pub session: Option<String>,

    /// Working directory
    #[arg(short = 'w', long)]
    pub cwd: Option<String>,

    /// Timeout in seconds
    #[arg(long, default_value = "300")]
    pub timeout: u64,
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

#[derive(Args)]
pub struct RunArgs {
    /// Agent ID to run
    pub agent_id: String,

    /// Input prompt
    #[arg(short, long)]
    pub input: Option<String>,

    /// Run in background
    #[arg(short, long)]
    pub background: bool,

    /// Stream output
    #[arg(long)]
    pub stream: bool,
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
    },

    /// Update agent
    Update {
        id: String,

        #[arg(short, long)]
        name: Option<String>,

        #[arg(short, long)]
        model: Option<String>,
    },

    /// Delete agent
    Delete { id: String },

    /// Execute agent
    Exec {
        id: String,

        #[arg(short, long)]
        input: Option<String>,

        /// Optional chat session ID for message mirroring
        #[arg(long)]
        session: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum TaskCommands {
    /// List tasks
    List {
        #[arg(short, long)]
        status: Option<String>,
    },

    /// Show task details
    Show { id: String },

    /// Create task
    Create {
        #[arg(long)]
        agent: String,

        #[arg(long)]
        name: String,

        #[arg(long)]
        input: Option<String>,

        /// Override task prompt (alias of --input)
        #[arg(long)]
        prompt: Option<String>,

        /// Runtime template used to build task input
        #[arg(long)]
        input_template: Option<String>,

        #[arg(long)]
        description: Option<String>,

        /// Memory scope: shared_agent or per_task
        #[arg(long)]
        memory_scope: Option<String>,

        #[arg(long)]
        cron: Option<String>,

        #[arg(long)]
        timezone: Option<String>,
    },

    /// Update background task definition
    Update {
        id: String,

        #[arg(long)]
        name: Option<String>,

        #[arg(long)]
        agent: Option<String>,

        #[arg(long)]
        description: Option<String>,

        #[arg(long)]
        input: Option<String>,

        /// Override task prompt (alias of --input)
        #[arg(long)]
        prompt: Option<String>,

        /// Runtime template used to build task input
        #[arg(long)]
        input_template: Option<String>,

        /// Memory scope: shared_agent or per_task
        #[arg(long)]
        memory_scope: Option<String>,

        #[arg(long)]
        cron: Option<String>,

        #[arg(long)]
        timezone: Option<String>,
    },

    /// Control background task execution state
    Control {
        id: String,

        /// One of: start, pause, resume, stop, run_now
        #[arg(long)]
        action: String,
    },

    /// Show aggregated progress of a background task
    Progress {
        id: String,

        #[arg(long, default_value_t = 10)]
        event_limit: usize,
    },

    /// Send/list messages for a background task
    Message {
        #[command(subcommand)]
        command: TaskMessageCommands,
    },

    /// Pause task
    Pause { id: String },

    /// Resume task
    Resume { id: String },

    /// Cancel task
    Cancel { id: String },

    /// Watch task events
    Watch { id: String },

    /// Run task immediately
    Run { id: String },
}

#[derive(Subcommand)]
pub enum TaskMessageCommands {
    /// Send a message to a running/scheduled background task
    Send {
        id: String,

        #[arg(long)]
        message: String,

        /// One of: user, agent, system
        #[arg(long, default_value = "user")]
        source: String,
    },

    /// List recent messages of a background task
    List {
        id: String,

        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
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

        /// Enable the HTTP API
        #[arg(long)]
        http: bool,

        /// HTTP port for the API
        #[arg(short, long)]
        port: Option<u16>,

        /// Enable the MCP HTTP server
        #[arg(long)]
        mcp: bool,

        /// MCP HTTP server port
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

        /// Enable the HTTP API
        #[arg(long)]
        http: bool,

        /// HTTP port for the API
        #[arg(short, long)]
        port: Option<u16>,

        /// Enable the MCP HTTP server
        #[arg(long)]
        mcp: bool,

        /// MCP HTTP server port
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

        /// Install scope: user (default) or workspace
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
