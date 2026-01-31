use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;

use crate::output::OutputFormat;

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
        /// Shell to generate completions for
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

    /// Configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },

    /// Start as MCP server
    Mcp,

    /// Show system information
    Info,
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
pub enum ConfigCommands {
    /// Show configuration
    Show,

    /// Get config value
    Get { key: String },

    /// Set config value
    Set { key: String, value: String },
}
