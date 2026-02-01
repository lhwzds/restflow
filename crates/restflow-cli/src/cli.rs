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
    /// Start interactive TUI chat
    Chat(ChatArgs),

    /// Generate shell completions
    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },

    /// Run an agent directly
    Run(RunArgs),

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

#[derive(Args, Default)]
pub struct ChatArgs {
    /// Agent ID to use
    #[arg(short, long)]
    pub agent: Option<String>,

    /// Session ID to continue
    #[arg(short, long)]
    pub session: Option<String>,

    /// Initial message to send
    #[arg(short, long)]
    pub message: Option<String>,
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

        #[arg(long)]
        cron: Option<String>,
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

    /// Install from marketplace
    Install { name: String },
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

#[derive(Subcommand)]
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
