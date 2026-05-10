mod cli {
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
        use clap::{CommandFactory, Parser};
        use clap_complete::{Shell, generate};

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
        fn help_mentions_restflow_and_daemon_without_chat_subcommand() {
            let help = Cli::command().render_help().to_string();

            assert!(help.contains("RestFlow"));
            assert!(help.contains("daemon"));
            assert!(!help.contains("Start interactive terminal chat"));
        }

        #[test]
        fn version_is_available_from_command_metadata() {
            let command = Cli::command();
            assert!(command.get_version().is_some());
        }

        #[test]
        fn bash_completions_start_with_restflow_function() {
            let mut command = Cli::command();
            let mut output = Vec::new();

            generate(Shell::Bash, &mut command, "restflow", &mut output);

            let text = String::from_utf8(output).expect("completion should be utf8");
            assert!(text.starts_with("_restflow"));
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
            let cli =
                Cli::try_parse_from(["restflow", "session", "list"]).expect("parse session list");
            assert!(matches!(
                cli.command,
                Some(super::Commands::Session {
                    command: super::SessionCommands::List
                })
            ));
        }

        #[test]
        fn parses_daemon_restart_command() {
            let cli = Cli::try_parse_from(["restflow", "daemon", "restart"])
                .expect("parse daemon restart");
            assert!(matches!(
                cli.command,
                Some(super::Commands::Daemon {
                    command: super::DaemonCommands::Restart { .. }
                })
            ));
        }

        #[test]
        fn parses_daemon_start_command() {
            let cli =
                Cli::try_parse_from(["restflow", "daemon", "start"]).expect("parse daemon start");
            assert!(matches!(
                cli.command,
                Some(super::Commands::Daemon {
                    command: super::DaemonCommands::Start { foreground: false }
                })
            ));
        }

        #[test]
        fn parses_daemon_stop_command() {
            let cli =
                Cli::try_parse_from(["restflow", "daemon", "stop"]).expect("parse daemon stop");
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
}

mod error {
    use colored::Colorize;

    const SUGGESTION_HEADER: &str = "Suggestion:";

    fn suggestions_for_message(msg: &str) -> Vec<Vec<String>> {
        let lower = msg.to_lowercase();
        let mut blocks = Vec::new();

        if lower.contains("api key not found")
            || lower.contains("missing api key")
            || lower.contains("no api key configured")
        {
            blocks.push(vec![
                "Set your API key with:".to_string(),
                format!(
                    "{} restflow secret set ANTHROPIC_API_KEY <value>",
                    "$".dimmed()
                ),
            ]);
        }

        if lower.contains("agent not found") {
            blocks.push(vec![
                "List available agents with:".to_string(),
                format!("{} restflow agent list", "$".dimmed()),
            ]);
        }

        if lower.contains("connection refused") || lower.contains("network") {
            blocks.push(vec![
                "Check your internet connection and try again.".to_string(),
            ]);
        }

        blocks
    }

    pub fn handle_error(err: anyhow::Error) -> ! {
        eprintln!("{} {}", "Error:".red().bold(), err);

        for lines in suggestions_for_message(&err.to_string()) {
            eprintln!("\n{}", SUGGESTION_HEADER.yellow().bold());
            for line in lines {
                eprintln!("  {}", line);
            }
        }

        std::process::exit(1);
    }

    #[cfg(test)]
    mod tests {
        use super::suggestions_for_message;

        #[test]
        fn suggests_api_key_fix() {
            let suggestions = suggestions_for_message("No API key configured");
            let joined = suggestions
                .iter()
                .flat_map(|block| block.iter())
                .cloned()
                .collect::<Vec<String>>()
                .join("\n");
            assert!(joined.contains("restflow secret set ANTHROPIC_API_KEY"));
        }

        #[test]
        fn suggests_agent_list() {
            let suggestions = suggestions_for_message("agent not found: abc");
            let joined = suggestions
                .iter()
                .flat_map(|block| block.iter())
                .cloned()
                .collect::<Vec<String>>()
                .join("\n");
            assert!(joined.contains("restflow agent list"));
        }

        #[test]
        fn suggests_network_hint() {
            let suggestions = suggestions_for_message("connection refused");
            let joined = suggestions
                .iter()
                .flat_map(|block| block.iter())
                .cloned()
                .collect::<Vec<String>>()
                .join("\n");
            assert!(joined.contains("internet connection"));
        }

        #[test]
        fn no_suggestion_for_unrelated_error() {
            let suggestions = suggestions_for_message("unexpected parse error");
            assert!(suggestions.is_empty());
        }
    }
}

mod setup {
    //! CLI setup module
    //!
    //! Handles initialization of the RestFlow core for CLI usage.

    use ::daemon::{AppCore, paths};
    use anyhow::Result;
    use std::sync::Arc;

    /// Resolve the database path for CLI usage.
    pub fn resolve_db_path(db_path: Option<String>) -> Result<String> {
        match db_path {
            Some(path) => Ok(path),
            None => paths::ensure_database_path_string(),
        }
    }

    /// Build the embedded RestFlow core
    pub async fn prepare_core(db_path: Option<String>) -> Result<Arc<AppCore>> {
        let db_path = resolve_db_path(db_path)?;
        Ok(Arc::new(AppCore::new(&db_path).await?))
    }

    // TODO: Add startup-time API key validation for chat flows.
    // The old validation used rig-core which has been removed.
}

mod config {
    pub mod settings {
        pub use ::daemon::storage::CliConfig;
    }

    pub use settings::CliConfig;
}

mod output {
    pub mod json {
        use anyhow::Result;
        use serde::Serialize;

        pub fn print_json<T: Serialize>(value: &T) -> Result<()> {
            let output = serde_json::to_string_pretty(value)?;
            println!("{output}");
            Ok(())
        }
    }

    pub mod table {
        use anyhow::Result;
        use comfy_table::Table;

        pub fn print_table(table: Table) -> Result<()> {
            println!("{table}");
            Ok(())
        }
    }

    pub use crate::cli::OutputFormat;
}

mod executor {
    #[cfg(test)]
    pub mod direct {
        use anyhow::{Result, bail};
        use async_trait::async_trait;
        use std::sync::Arc;

        use crate::executor::CommandExecutor;
        use crate::setup;
        use ::daemon::StoredAgent;
        use ::daemon::services::{
            agent as agent_service, config as config_service, secrets as secrets_service,
            session::SessionService, skills as skills_service,
        };
        use ::daemon::storage::SystemConfig;
        use ::daemon::{AppCore, Secret};
        use types::{AgentNode, ChatSession, ChatSessionSummary};
        use types::{CleanupReportResponse, Skill};
        /// Test-only executor used by command unit tests.
        pub struct DirectExecutor {
            core: Arc<AppCore>,
        }

        impl DirectExecutor {
            pub async fn connect(db_path: Option<String>) -> Result<Self> {
                let core = setup::prepare_core(db_path).await?;
                Ok(Self { core })
            }
        }

        #[async_trait]
        impl CommandExecutor for DirectExecutor {
            async fn list_agents(&self) -> Result<Vec<StoredAgent>> {
                agent_service::list_agents(&self.core).await
            }

            async fn get_agent(&self, id: &str) -> Result<StoredAgent> {
                agent_service::get_agent(&self.core, id).await
            }

            async fn create_agent(&self, name: String, agent: AgentNode) -> Result<StoredAgent> {
                agent_service::create_agent(&self.core, name, agent).await
            }

            async fn update_agent(
                &self,
                id: &str,
                name: Option<String>,
                agent: Option<AgentNode>,
            ) -> Result<StoredAgent> {
                agent_service::update_agent(&self.core, id, name, agent).await
            }

            async fn delete_agent(&self, id: &str) -> Result<()> {
                agent_service::delete_agent(&self.core, id).await
            }

            async fn list_skills(&self) -> Result<Vec<Skill>> {
                skills_service::list_skills(&self.core).await
            }

            async fn get_skill(&self, id: &str) -> Result<Option<Skill>> {
                skills_service::get_skill(&self.core, id).await
            }

            async fn list_secrets(&self) -> Result<Vec<Secret>> {
                secrets_service::list_secrets(&self.core).await
            }

            async fn set_secret(
                &self,
                key: &str,
                value: &str,
                description: Option<String>,
            ) -> Result<()> {
                secrets_service::set_secret(&self.core, key, value, description).await
            }

            async fn create_secret(
                &self,
                key: &str,
                value: &str,
                description: Option<String>,
            ) -> Result<()> {
                secrets_service::create_secret(&self.core, key, value, description).await
            }

            async fn update_secret(
                &self,
                key: &str,
                value: &str,
                description: Option<String>,
            ) -> Result<()> {
                secrets_service::update_secret(&self.core, key, value, description).await
            }

            async fn delete_secret(&self, key: &str) -> Result<()> {
                secrets_service::delete_secret(&self.core, key).await
            }

            async fn has_secret(&self, key: &str) -> Result<bool> {
                Ok(secrets_service::get_secret(&self.core, key)
                    .await?
                    .is_some())
            }

            async fn get_config(&self) -> Result<SystemConfig> {
                config_service::get_config(&self.core).await
            }

            async fn get_global_config(&self) -> Result<SystemConfig> {
                config_service::get_global_config(&self.core).await
            }

            async fn set_config(&self, config: SystemConfig) -> Result<()> {
                config_service::update_config(&self.core, config).await
            }

            async fn run_cleanup(&self) -> Result<CleanupReportResponse> {
                let report = ::daemon::services::cleanup::run_cleanup(&self.core).await?;
                Ok(CleanupReportResponse {
                    chat_sessions: report.chat_sessions,
                    daemon_log_files: report.daemon_log_files,
                })
            }

            async fn list_sessions(&self) -> Result<Vec<ChatSessionSummary>> {
                Ok(SessionService::from_storage(&self.core.storage)
                    .list_session_views(None, None, false)?
                    .iter()
                    .map(ChatSessionSummary::from)
                    .collect())
            }

            async fn get_session(&self, id: &str) -> Result<ChatSession> {
                SessionService::from_storage(&self.core.storage)
                    .get_session_view(id)?
                    .ok_or_else(|| anyhow::anyhow!("Session not found: {}", id))
            }

            async fn search_sessions(
                &self,
                query: &str,
                agent_id: Option<&str>,
                limit: usize,
            ) -> Result<Vec<ChatSessionSummary>> {
                Ok(SessionService::from_storage(&self.core.storage)
                    .search_session_views(query, agent_id, None, false, limit.max(1))?
                    .iter()
                    .map(ChatSessionSummary::from)
                    .collect())
            }

            async fn create_session(
                &self,
                agent_id: Option<String>,
                model: Option<String>,
                name: Option<String>,
                skill_id: Option<String>,
            ) -> Result<ChatSession> {
                let agent_id = resolve_agent_id(&self.core, agent_id).await?;
                let model = model.unwrap_or_else(|| "gpt-5.4".to_string());
                SessionService::from_storage(&self.core.storage)
                    .create_workspace_session(agent_id, model, name, skill_id, None)
            }

            async fn delete_session(&self, id: &str) -> Result<bool> {
                SessionService::from_storage(&self.core.storage).delete_session(id)
            }
        }

        async fn resolve_agent_id(core: &Arc<AppCore>, agent_id: Option<String>) -> Result<String> {
            if let Some(agent_id) = agent_id {
                return Ok(agent_id);
            }

            let agents = agent_service::list_agents(core).await?;
            if agents.is_empty() {
                bail!("No agents available");
            }

            Ok(agents[0].id.clone())
        }
    }

    pub mod ipc {
        use anyhow::Result;
        use async_trait::async_trait;
        use std::path::Path;
        use tokio::sync::Mutex;
        use types::{CleanupReportResponse, OkResponse};

        use crate::executor::CommandExecutor;
        use ::daemon::Secret;
        use ::daemon::StoredAgent;
        use ::daemon::daemon::request_mapper::to_contract;
        use ::daemon::daemon::{IpcClient, IpcRequest};
        use ::daemon::storage::SystemConfig;
        use types::{AgentNode, ChatSession, ChatSessionSummary, Skill};

        pub struct IpcExecutor {
            client: Mutex<IpcClient>,
        }

        impl IpcExecutor {
            pub async fn connect(socket_path: &Path) -> Result<Self> {
                let client = IpcClient::connect(socket_path).await?;
                Ok(Self {
                    client: Mutex::new(client),
                })
            }

            async fn request_typed<T: serde::de::DeserializeOwned>(
                &self,
                req: IpcRequest,
            ) -> Result<T> {
                let mut client = self.client.lock().await;
                client.request_typed(req).await
            }

            async fn request_optional<T: serde::de::DeserializeOwned>(
                &self,
                req: IpcRequest,
            ) -> Result<Option<T>> {
                let mut client = self.client.lock().await;
                client.request_optional(req).await
            }
        }

        #[async_trait]
        impl CommandExecutor for IpcExecutor {
            async fn list_agents(&self) -> Result<Vec<StoredAgent>> {
                self.request_typed(IpcRequest::ListAgents).await
            }

            async fn get_agent(&self, id: &str) -> Result<StoredAgent> {
                self.request_typed(IpcRequest::GetAgent { id: id.to_string() })
                    .await
            }

            async fn create_agent(&self, name: String, agent: AgentNode) -> Result<StoredAgent> {
                let agent = to_contract(agent)?;
                self.request_typed(IpcRequest::CreateAgent { name, agent })
                    .await
            }

            async fn update_agent(
                &self,
                id: &str,
                name: Option<String>,
                agent: Option<AgentNode>,
            ) -> Result<StoredAgent> {
                let agent = agent.map(to_contract).transpose()?;
                self.request_typed(IpcRequest::UpdateAgent {
                    id: id.to_string(),
                    name,
                    agent,
                })
                .await
            }

            async fn delete_agent(&self, id: &str) -> Result<()> {
                let _: OkResponse = self
                    .request_typed(IpcRequest::DeleteAgent { id: id.to_string() })
                    .await?;
                Ok(())
            }

            async fn list_skills(&self) -> Result<Vec<Skill>> {
                self.request_typed(IpcRequest::ListSkills).await
            }

            async fn get_skill(&self, id: &str) -> Result<Option<Skill>> {
                self.request_optional(IpcRequest::GetSkill { id: id.to_string() })
                    .await
            }

            async fn list_secrets(&self) -> Result<Vec<Secret>> {
                self.request_typed(IpcRequest::ListSecrets).await
            }

            async fn set_secret(
                &self,
                key: &str,
                value: &str,
                description: Option<String>,
            ) -> Result<()> {
                let _: OkResponse = self
                    .request_typed(IpcRequest::SetSecret {
                        key: key.to_string(),
                        value: value.to_string(),
                        description,
                    })
                    .await?;
                Ok(())
            }

            async fn create_secret(
                &self,
                key: &str,
                value: &str,
                description: Option<String>,
            ) -> Result<()> {
                let _: OkResponse = self
                    .request_typed(IpcRequest::CreateSecret {
                        key: key.to_string(),
                        value: value.to_string(),
                        description,
                    })
                    .await?;
                Ok(())
            }

            async fn update_secret(
                &self,
                key: &str,
                value: &str,
                description: Option<String>,
            ) -> Result<()> {
                let _: OkResponse = self
                    .request_typed(IpcRequest::UpdateSecret {
                        key: key.to_string(),
                        value: value.to_string(),
                        description,
                    })
                    .await?;
                Ok(())
            }

            async fn delete_secret(&self, key: &str) -> Result<()> {
                let _: OkResponse = self
                    .request_typed(IpcRequest::DeleteSecret {
                        key: key.to_string(),
                    })
                    .await?;
                Ok(())
            }

            async fn has_secret(&self, key: &str) -> Result<bool> {
                let response = self
                    .request_optional::<types::SecretResponse>(IpcRequest::GetSecret {
                        key: key.to_string(),
                    })
                    .await?;
                Ok(response.is_some())
            }

            async fn get_config(&self) -> Result<SystemConfig> {
                self.request_typed(IpcRequest::GetConfig).await
            }

            async fn get_global_config(&self) -> Result<SystemConfig> {
                self.request_typed(IpcRequest::GetGlobalConfig).await
            }

            async fn set_config(&self, config: SystemConfig) -> Result<()> {
                let config = to_contract(config)?;
                let _: OkResponse = self.request_typed(IpcRequest::SetConfig { config }).await?;
                Ok(())
            }

            async fn run_cleanup(&self) -> Result<CleanupReportResponse> {
                self.request_typed(IpcRequest::RunCleanup).await
            }

            async fn list_sessions(&self) -> Result<Vec<ChatSessionSummary>> {
                let mut client = self.client.lock().await;
                client.list_sessions().await
            }

            async fn get_session(&self, id: &str) -> Result<ChatSession> {
                let mut client = self.client.lock().await;
                client.get_session(id.to_string()).await
            }

            async fn search_sessions(
                &self,
                query: &str,
                agent_id: Option<&str>,
                limit: usize,
            ) -> Result<Vec<ChatSessionSummary>> {
                let mut client = self.client.lock().await;
                client
                    .search_sessions(
                        query.to_string(),
                        agent_id.map(ToOwned::to_owned),
                        Some(limit.max(1)),
                    )
                    .await
            }

            async fn create_session(
                &self,
                agent_id: Option<String>,
                model: Option<String>,
                name: Option<String>,
                skill_id: Option<String>,
            ) -> Result<ChatSession> {
                let mut client = self.client.lock().await;
                client.create_session(agent_id, model, name, skill_id).await
            }

            async fn delete_session(&self, id: &str) -> Result<bool> {
                let mut client = self.client.lock().await;
                client.delete_session(id.to_string()).await
            }
        }
    }

    use ::daemon::Secret;
    use ::daemon::StoredAgent;
    use ::daemon::daemon::is_daemon_available;
    use ::daemon::paths;
    use ::daemon::storage::SystemConfig;
    use anyhow::Result;
    use async_trait::async_trait;
    use std::sync::Arc;
    use types::CleanupReportResponse;
    use types::{AgentNode, ChatSession, ChatSessionSummary, Skill};

    #[async_trait]
    pub trait CommandExecutor: Send + Sync {
        async fn list_agents(&self) -> Result<Vec<StoredAgent>>;
        async fn get_agent(&self, id: &str) -> Result<StoredAgent>;
        async fn create_agent(&self, name: String, agent: AgentNode) -> Result<StoredAgent>;
        async fn update_agent(
            &self,
            id: &str,
            name: Option<String>,
            agent: Option<AgentNode>,
        ) -> Result<StoredAgent>;
        async fn delete_agent(&self, id: &str) -> Result<()>;

        async fn list_skills(&self) -> Result<Vec<Skill>>;
        async fn get_skill(&self, id: &str) -> Result<Option<Skill>>;

        async fn list_secrets(&self) -> Result<Vec<Secret>>;
        async fn set_secret(
            &self,
            key: &str,
            value: &str,
            description: Option<String>,
        ) -> Result<()>;
        #[allow(dead_code)]
        async fn create_secret(
            &self,
            key: &str,
            value: &str,
            description: Option<String>,
        ) -> Result<()>;
        #[allow(dead_code)]
        async fn update_secret(
            &self,
            key: &str,
            value: &str,
            description: Option<String>,
        ) -> Result<()>;
        async fn delete_secret(&self, key: &str) -> Result<()>;
        async fn has_secret(&self, key: &str) -> Result<bool>;

        async fn get_config(&self) -> Result<SystemConfig>;
        async fn get_global_config(&self) -> Result<SystemConfig>;
        async fn set_config(&self, config: SystemConfig) -> Result<()>;

        async fn run_cleanup(&self) -> Result<CleanupReportResponse>;

        // Session operations
        async fn list_sessions(&self) -> Result<Vec<ChatSessionSummary>>;
        async fn get_session(&self, id: &str) -> Result<ChatSession>;
        async fn search_sessions(
            &self,
            query: &str,
            agent_id: Option<&str>,
            limit: usize,
        ) -> Result<Vec<ChatSessionSummary>>;
        async fn create_session(
            &self,
            agent_id: Option<String>,
            model: Option<String>,
            name: Option<String>,
            skill_id: Option<String>,
        ) -> Result<ChatSession>;
        async fn delete_session(&self, id: &str) -> Result<bool>;
    }

    pub async fn create(db_path: Option<String>) -> Result<Arc<dyn CommandExecutor>> {
        if let Some(db_path) = db_path {
            anyhow::bail!(
                "The --db-path flag is only supported for daemon lifecycle commands. Commands routed through the daemon must target the running daemon instance instead of selecting a database path directly: {}",
                db_path
            );
        }

        // This is the only production executor entrypoint for daemon-routed commands.
        let socket_path = paths::socket_path()?;
        if is_daemon_available(&socket_path).await {
            let executor = ipc::IpcExecutor::connect(&socket_path).await?;
            return Ok(Arc::new(executor));
        }

        anyhow::bail!("RestFlow daemon is not running. Start it with 'restflow daemon start'.")
    }

    #[cfg(test)]
    #[allow(clippy::await_holding_lock)]
    mod tests {
        use super::*;
        use tempfile::tempdir;

        fn env_lock() -> std::sync::MutexGuard<'static, ()> {
            crate::test_support::env_lock()
        }

        #[tokio::test]
        async fn create_requires_running_daemon() {
            let _guard = env_lock();
            let temp = tempdir().expect("tempdir");
            let prev = std::env::var_os("RESTFLOW_DIR");
            unsafe { std::env::set_var("RESTFLOW_DIR", temp.path()) };

            let err = match create(None).await {
                Ok(_) => panic!("create should fail without daemon"),
                Err(err) => err,
            };
            assert!(err.to_string().contains("daemon is not running"));
            assert!(
                err.to_string()
                    .contains("Start it with 'restflow daemon start'")
            );

            match prev {
                Some(value) => unsafe { std::env::set_var("RESTFLOW_DIR", value) },
                None => unsafe { std::env::remove_var("RESTFLOW_DIR") },
            }
        }

        #[tokio::test]
        async fn create_rejects_db_path_for_executor_commands() {
            let err = match create(Some("/tmp/restflow.db".to_string())).await {
                Ok(_) => panic!("create should reject db_path for daemon-routed commands"),
                Err(err) => err,
            };
            assert!(
                err.to_string()
                    .contains("only supported for daemon lifecycle commands")
            );
        }
    }
}

mod commands {
    pub mod agent {
        use anyhow::{Result, bail};
        use comfy_table::{Cell, Table};
        use std::sync::Arc;

        use crate::cli::{AgentCommands, CodexExecutionModeArg};
        use crate::commands::utils::{format_timestamp, short_id};
        use crate::executor::CommandExecutor;
        use crate::output::{OutputFormat, json::print_json};
        use types::AgentNode;

        pub async fn run(
            executor: Arc<dyn CommandExecutor>,
            command: AgentCommands,
            format: OutputFormat,
        ) -> Result<()> {
            match command {
                AgentCommands::List => list_agents(executor, format).await,
                AgentCommands::Show { id } => show_agent(executor, &id, format).await,
                AgentCommands::Create {
                    name,
                    provider,
                    model,
                    prompt,
                    codex_execution_mode,
                    codex_reasoning_effort,
                } => {
                    create_agent(
                        executor,
                        &name,
                        provider,
                        model,
                        prompt,
                        codex_execution_mode,
                        codex_reasoning_effort,
                        format,
                    )
                    .await
                }
                AgentCommands::Update {
                    id,
                    name,
                    provider,
                    model,
                    codex_execution_mode,
                    codex_reasoning_effort,
                } => {
                    update_agent(
                        executor,
                        &id,
                        name,
                        provider,
                        model,
                        codex_execution_mode,
                        codex_reasoning_effort,
                        format,
                    )
                    .await
                }
                AgentCommands::Delete { id } => delete_agent(executor, &id, format).await,
            }
        }

        async fn list_agents(
            executor: Arc<dyn CommandExecutor>,
            format: OutputFormat,
        ) -> Result<()> {
            let agents = executor.list_agents().await?;

            if format.is_json() {
                return print_json(&agents);
            }

            let mut table = Table::new();
            table.set_header(vec!["ID", "Name", "Provider", "Model", "Updated"]);

            for agent in agents {
                let model_ref = agent.agent.resolved_model_ref();
                let model_str = model_ref
                    .map(|model_ref| model_ref.model.as_serialized_str())
                    .unwrap_or("(not set)");
                let provider_str = model_ref
                    .map(|model_ref| model_ref.provider.as_canonical_str())
                    .unwrap_or("auto");
                table.add_row(vec![
                    Cell::new(short_id(&agent.id)),
                    Cell::new(agent.name),
                    Cell::new(provider_str),
                    Cell::new(model_str),
                    Cell::new(format_timestamp(agent.updated_at)),
                ]);
            }

            crate::output::table::print_table(table)
        }

        async fn show_agent(
            executor: Arc<dyn CommandExecutor>,
            id: &str,
            format: OutputFormat,
        ) -> Result<()> {
            let agent = executor.get_agent(id).await?;

            if format.is_json() {
                return print_json(&agent);
            }

            println!("ID:          {}", agent.id);
            println!("Name:        {}", agent.name);
            if let Some(model_ref) = agent.agent.resolved_model_ref() {
                println!("Model:       {}", model_ref.model.as_serialized_str());
                println!("Provider:    {}", model_ref.provider.as_canonical_str());
            } else {
                println!(
                    "Model:       (not set - will auto-select based on configured credentials)"
                );
            }
            println!("Created:     {}", format_timestamp(agent.created_at));
            println!("Updated:     {}", format_timestamp(agent.updated_at));
            println!("Tools:       {}", format_tools(&agent.agent.tools));
            if let Some(mode) = agent.agent.codex_cli_execution_mode {
                println!("Codex Mode:  {}", mode.as_str());
            }
            if let Some(effort) = &agent.agent.codex_cli_reasoning_effort {
                println!("Codex Effort: {}", effort);
            }

            if let Some(prompt) = agent.agent.prompt {
                println!("\nSystem Prompt:\n{prompt}");
            }

            Ok(())
        }

        #[allow(clippy::too_many_arguments)]
        async fn create_agent(
            executor: Arc<dyn CommandExecutor>,
            name: &str,
            provider: Option<String>,
            model: Option<String>,
            prompt: Option<String>,
            codex_execution_mode: Option<CodexExecutionModeArg>,
            codex_reasoning_effort: Option<String>,
            format: OutputFormat,
        ) -> Result<()> {
            reject_agent_model_options(
                provider.as_deref(),
                model.as_deref(),
                codex_execution_mode.as_ref(),
                codex_reasoning_effort.as_deref(),
            )?;

            let mut agent_node = AgentNode::new();
            if let Some(prompt) = prompt {
                agent_node = agent_node.with_prompt(prompt);
            }

            let created = executor.create_agent(name.to_string(), agent_node).await?;

            if format.is_json() {
                return print_json(&created);
            }

            println!("Agent created: {} ({})", created.name, created.id);
            Ok(())
        }

        #[allow(clippy::too_many_arguments)]
        async fn update_agent(
            executor: Arc<dyn CommandExecutor>,
            id: &str,
            name: Option<String>,
            provider: Option<String>,
            model: Option<String>,
            codex_execution_mode: Option<CodexExecutionModeArg>,
            codex_reasoning_effort: Option<String>,
            format: OutputFormat,
        ) -> Result<()> {
            let existing = executor.get_agent(id).await?;

            reject_agent_model_options(
                provider.as_deref(),
                model.as_deref(),
                codex_execution_mode.as_ref(),
                codex_reasoning_effort.as_deref(),
            )?;

            let updated = executor
                .update_agent(id, name, Some(existing.agent))
                .await?;

            if format.is_json() {
                return print_json(&updated);
            }

            println!("Agent updated: {} ({})", updated.name, updated.id);
            Ok(())
        }

        async fn delete_agent(
            executor: Arc<dyn CommandExecutor>,
            id: &str,
            format: OutputFormat,
        ) -> Result<()> {
            executor.delete_agent(id).await?;

            if format.is_json() {
                return print_json(&serde_json::json!({ "deleted": true, "id": id }));
            }

            println!("Agent deleted: {id}");
            Ok(())
        }

        fn format_tools(tools: &Option<Vec<String>>) -> String {
            match tools {
                Some(tool_list) if !tool_list.is_empty() => tool_list.join(", "),
                _ => "-".to_string(),
            }
        }

        fn reject_agent_model_options(
            provider: Option<&str>,
            model: Option<&str>,
            codex_execution_mode: Option<&CodexExecutionModeArg>,
            codex_reasoning_effort: Option<&str>,
        ) -> Result<()> {
            if provider.is_none()
                && model.is_none()
                && codex_execution_mode.is_none()
                && codex_reasoning_effort.is_none()
            {
                return Ok(());
            }

            bail!(
                "Agent files no longer persist model or auth settings. Configure the runtime model separately instead of using --provider, --model, --codex-execution-mode, or --codex-reasoning-effort on agent commands."
            )
        }
    }

    pub mod config {
        use anyhow::{Result, bail};
        use comfy_table::{Cell, Table};
        use serde_json::json;
        use std::sync::Arc;

        use crate::cli::ConfigCommands;
        use crate::executor::CommandExecutor;
        use crate::output::{OutputFormat, json::print_json};
        use ::daemon::storage::{
            CliConfig, ConfigDocument, ConfigSourcePathInfo, SystemConfig,
            effective_config_sources, load_cli_config, load_global_cli_config, write_cli_config,
        };

        pub async fn run(
            executor: Arc<dyn CommandExecutor>,
            command: ConfigCommands,
            format: OutputFormat,
        ) -> Result<()> {
            match command {
                ConfigCommands::Show => show_config(executor, format).await,
                ConfigCommands::Get { key } => get_config_value(executor, &key, format).await,
                ConfigCommands::Set { key, value } => {
                    set_config_value(executor, &key, &value, format).await
                }
                ConfigCommands::Reset => reset_config(executor, format).await,
            }
        }

        async fn show_config(
            executor: Arc<dyn CommandExecutor>,
            format: OutputFormat,
        ) -> Result<()> {
            let config = load_effective_config_document(executor).await?;
            let sources = effective_config_sources()?;

            if format.is_json() {
                let mut payload = serde_json::to_value(&config)?;
                if let Some(object) = payload.as_object_mut() {
                    object.insert(
                        "_effective_sources".to_string(),
                        serde_json::to_value(&sources)?,
                    );
                }
                return print_json(&payload);
            }

            let mut table = Table::new();
            table.set_header(vec!["Key", "Value"]);

            table.add_row(vec![
                Cell::new("system.worker_count"),
                Cell::new(config.system.worker_count),
            ]);
            table.add_row(vec![
                Cell::new("system.stall_timeout_seconds"),
                Cell::new(config.system.stall_timeout_seconds),
            ]);
            table.add_row(vec![
                Cell::new("system.chat_response_timeout_seconds"),
                Cell::new(format_optional_u64(
                    config.system.chat_response_timeout_seconds,
                )),
            ]);
            table.add_row(vec![
                Cell::new("system.max_retries"),
                Cell::new(config.system.max_retries),
            ]);
            table.add_row(vec![
                Cell::new("system.chat_session_retention_days"),
                Cell::new(config.system.chat_session_retention_days),
            ]);
            table.add_row(vec![
                Cell::new("system.log_file_retention_days"),
                Cell::new(config.system.log_file_retention_days),
            ]);
            table.add_row(vec![
                Cell::new("system.experimental_features"),
                Cell::new(format_string_list(&config.system.experimental_features)),
            ]);
            table.add_row(vec![
                Cell::new("agent.max_iterations"),
                Cell::new(config.agent.max_iterations),
            ]);
            table.add_row(vec![
                Cell::new("agent.max_depth"),
                Cell::new(config.agent.max_depth),
            ]);
            table.add_row(vec![
                Cell::new("agent.tool_timeout_secs"),
                Cell::new(config.agent.tool_timeout_secs),
            ]);
            table.add_row(vec![
                Cell::new("agent.llm_timeout_secs"),
                Cell::new(format_optional_u64(config.agent.llm_timeout_secs)),
            ]);
            table.add_row(vec![
                Cell::new("agent.bash_timeout_secs"),
                Cell::new(config.agent.bash_timeout_secs),
            ]);
            table.add_row(vec![
                Cell::new("agent.approval_timeout_secs"),
                Cell::new(config.agent.approval_timeout_secs),
            ]);
            table.add_row(vec![
                Cell::new("agent.auto_review_tools"),
                Cell::new(config.agent.auto_review_tools),
            ]);
            table.add_row(vec![
                Cell::new("agent.subagent_timeout_secs"),
                Cell::new(config.agent.subagent_timeout_secs),
            ]);
            table.add_row(vec![
                Cell::new("agent.max_parallel_subagents"),
                Cell::new(config.agent.max_parallel_subagents),
            ]);
            table.add_row(vec![
                Cell::new("agent.max_tool_calls"),
                Cell::new(config.agent.max_tool_calls),
            ]);
            table.add_row(vec![
                Cell::new("agent.max_tool_concurrency"),
                Cell::new(config.agent.max_tool_concurrency),
            ]);
            table.add_row(vec![
                Cell::new("agent.max_tool_result_length"),
                Cell::new(config.agent.max_tool_result_length),
            ]);
            table.add_row(vec![
                Cell::new("agent.prune_tool_max_chars"),
                Cell::new(config.agent.prune_tool_max_chars),
            ]);
            table.add_row(vec![
                Cell::new("agent.compact_preserve_tokens"),
                Cell::new(config.agent.compact_preserve_tokens),
            ]);
            table.add_row(vec![
                Cell::new("agent.max_wall_clock_secs"),
                Cell::new(format_optional_u64(config.agent.max_wall_clock_secs)),
            ]);
            table.add_row(vec![
                Cell::new("agent.fallback_models"),
                Cell::new(
                    config
                        .agent
                        .fallback_models
                        .as_ref()
                        .map(|m| m.join(", "))
                        .unwrap_or_else(|| "none".to_string()),
                ),
            ]);
            table.add_row(vec![
                Cell::new("api.session_list_limit"),
                Cell::new(config.api.session_list_limit),
            ]);
            table.add_row(vec![
                Cell::new("api.web_search_num_results"),
                Cell::new(config.api.web_search_num_results),
            ]);
            table.add_row(vec![
                Cell::new("runtime.chat_max_session_history"),
                Cell::new(config.runtime.chat_max_session_history),
            ]);
            table.add_row(vec![
                Cell::new("registry.github_cache_ttl_secs"),
                Cell::new(config.registry.github_cache_ttl_secs),
            ]);
            table.add_row(vec![
                Cell::new("registry.marketplace_cache_ttl_secs"),
                Cell::new(config.registry.marketplace_cache_ttl_secs),
            ]);
            table.add_row(vec![
                Cell::new("cli.version"),
                Cell::new(config.cli.version),
            ]);
            table.add_row(vec![
                Cell::new("cli.agent"),
                Cell::new(format_optional_string(config.cli.agent.as_deref())),
            ]);
            table.add_row(vec![
                Cell::new("cli.model"),
                Cell::new(format_optional_string(config.cli.model.as_deref())),
            ]);
            table.add_row(vec![
                Cell::new("sources.global"),
                Cell::new(format_source_info(&sources.global)),
            ]);
            table.add_row(vec![
                Cell::new("sources.workspace"),
                Cell::new(format_source_info(&sources.workspace)),
            ]);
            table.add_row(vec![
                Cell::new("sources.write_target"),
                Cell::new(format_source_info(&sources.write_target)),
            ]);
            crate::output::table::print_table(table)
        }

        async fn get_config_value(
            executor: Arc<dyn CommandExecutor>,
            key: &str,
            format: OutputFormat,
        ) -> Result<()> {
            let config = load_effective_config_document(executor).await?;

            let value = match key {
                "system" => json!(config.system),
                "system.worker_count" => json!(config.system.worker_count),
                "system.stall_timeout_seconds" => json!(config.system.stall_timeout_seconds),
                "system.chat_response_timeout_seconds" => {
                    json!(config.system.chat_response_timeout_seconds)
                }
                "system.max_retries" => json!(config.system.max_retries),
                "system.chat_session_retention_days" => {
                    json!(config.system.chat_session_retention_days)
                }
                "system.log_file_retention_days" => json!(config.system.log_file_retention_days),
                "system.experimental_features" => json!(config.system.experimental_features),
                "agent" => json!(config.agent),
                "agent.tool_timeout_secs" => json!(config.agent.tool_timeout_secs),
                "agent.llm_timeout_secs" => json!(config.agent.llm_timeout_secs),
                "agent.bash_timeout_secs" => json!(config.agent.bash_timeout_secs),
                "agent.approval_timeout_secs" => json!(config.agent.approval_timeout_secs),
                "agent.auto_review_tools" => json!(config.agent.auto_review_tools),
                "agent.max_iterations" => json!(config.agent.max_iterations),
                "agent.max_depth" => json!(config.agent.max_depth),
                "agent.subagent_timeout_secs" => json!(config.agent.subagent_timeout_secs),
                "agent.max_parallel_subagents" => json!(config.agent.max_parallel_subagents),
                "agent.max_tool_calls" => json!(config.agent.max_tool_calls),
                "agent.max_tool_concurrency" => json!(config.agent.max_tool_concurrency),
                "agent.max_tool_result_length" => json!(config.agent.max_tool_result_length),
                "agent.prune_tool_max_chars" => json!(config.agent.prune_tool_max_chars),
                "agent.compact_preserve_tokens" => json!(config.agent.compact_preserve_tokens),
                "agent.max_wall_clock_secs" => json!(config.agent.max_wall_clock_secs),
                "agent.fallback_models" => json!(config.agent.fallback_models),
                "api" => json!(config.api),
                "api.session_list_limit" => json!(config.api.session_list_limit),
                "api.web_search_num_results" => json!(config.api.web_search_num_results),
                "runtime" => json!(config.runtime),
                "runtime.chat_max_session_history" => {
                    json!(config.runtime.chat_max_session_history)
                }
                "registry" => json!(config.registry),
                "registry.github_cache_ttl_secs" => {
                    json!(config.registry.github_cache_ttl_secs)
                }
                "registry.marketplace_cache_ttl_secs" => {
                    json!(config.registry.marketplace_cache_ttl_secs)
                }
                "cli" => json!(config.cli),
                "cli.version" => json!(config.cli.version),
                "cli.agent" => json!(config.cli.agent),
                "cli.model" => json!(config.cli.model),
                "_effective_sources" | "effective_sources" => json!(effective_config_sources()?),
                _ => bail!("Unsupported config key: {key}"),
            };

            if format.is_json() {
                return print_json(&json!({ "key": key, "value": value }));
            }

            println!("{key} = {value}");
            Ok(())
        }

        async fn set_config_value(
            executor: Arc<dyn CommandExecutor>,
            key: &str,
            value: &str,
            format: OutputFormat,
        ) -> Result<()> {
            // Keep CLI-only preferences local so daemon-owned config stays behind the executor boundary.
            if key.starts_with("cli.") {
                let mut config = load_global_cli_config()?;
                match key {
                    "cli.version" => {
                        config.version = parse_value(value)?;
                    }
                    "cli.agent" => {
                        config.agent = parse_optional_string(value);
                    }
                    "cli.model" => {
                        config.model = parse_optional_string(value);
                    }
                    _ => bail!("Unsupported config key: {key}"),
                }
                write_cli_config(&config)?;
            } else {
                let mut config = executor.get_global_config().await?;

                match key {
                    "system.worker_count" => {
                        config.worker_count = parse_value(value)?;
                    }
                    "system.stall_timeout_seconds" => {
                        config.stall_timeout_seconds = parse_value(value)?;
                    }
                    "system.chat_response_timeout_seconds" => {
                        config.chat_response_timeout_seconds = parse_optional_u64(value)?;
                    }
                    "system.max_retries" => {
                        config.max_retries = parse_value(value)?;
                    }
                    "system.chat_session_retention_days" => {
                        config.chat_session_retention_days = parse_value(value)?;
                    }
                    "system.log_file_retention_days" => {
                        config.log_file_retention_days = parse_value(value)?;
                    }
                    "system.experimental_features" => {
                        config.experimental_features = parse_string_list(value)?;
                    }
                    "agent.tool_timeout_secs" => {
                        config.agent.tool_timeout_secs = parse_value(value)?;
                    }
                    "agent.llm_timeout_secs" => {
                        config.agent.llm_timeout_secs = parse_optional_u64(value)?;
                    }
                    "agent.bash_timeout_secs" => {
                        config.agent.bash_timeout_secs = parse_value(value)?;
                    }
                    "agent.approval_timeout_secs" => {
                        config.agent.approval_timeout_secs = parse_value(value)?;
                    }
                    "agent.auto_review_tools" => {
                        config.agent.auto_review_tools = parse_value(value)?;
                    }
                    "agent.max_iterations" => {
                        config.agent.max_iterations = parse_value(value)?;
                    }
                    "agent.max_depth" => {
                        config.agent.max_depth = parse_value(value)?;
                    }
                    "agent.subagent_timeout_secs" => {
                        config.agent.subagent_timeout_secs = parse_value(value)?;
                    }
                    "agent.max_parallel_subagents" => {
                        config.agent.max_parallel_subagents = parse_value(value)?;
                    }
                    "agent.max_tool_calls" => {
                        config.agent.max_tool_calls = parse_value(value)?;
                    }
                    "agent.max_tool_concurrency" => {
                        config.agent.max_tool_concurrency = parse_value(value)?;
                    }
                    "agent.max_tool_result_length" => {
                        config.agent.max_tool_result_length = parse_value(value)?;
                    }
                    "agent.prune_tool_max_chars" => {
                        config.agent.prune_tool_max_chars = parse_value(value)?;
                    }
                    "agent.compact_preserve_tokens" => {
                        config.agent.compact_preserve_tokens = parse_value(value)?;
                    }
                    "agent.max_wall_clock_secs" => {
                        config.agent.max_wall_clock_secs = parse_optional_u64(value)?;
                    }
                    "agent.fallback_models" => {
                        config.agent.fallback_models = parse_optional_string_list(value)?;
                    }
                    "api.session_list_limit" => {
                        config.api_defaults.session_list_limit = parse_value(value)?;
                    }
                    "api.web_search_num_results" => {
                        config.api_defaults.web_search_num_results = parse_value(value)?;
                    }
                    "runtime.chat_max_session_history" => {
                        config.runtime_defaults.chat_max_session_history = parse_value(value)?;
                    }
                    "registry.github_cache_ttl_secs" => {
                        config.registry_defaults.github_cache_ttl_secs = parse_value(value)?;
                    }
                    "registry.marketplace_cache_ttl_secs" => {
                        config.registry_defaults.marketplace_cache_ttl_secs = parse_value(value)?;
                    }
                    _ => bail!("Unsupported config key: {key}"),
                }

                executor.set_config(config).await?;
            }

            if format.is_json() {
                return print_json(&json!({ "updated": true, "key": key }));
            }

            println!("Updated {key}");
            Ok(())
        }

        async fn reset_config(
            executor: Arc<dyn CommandExecutor>,
            format: OutputFormat,
        ) -> Result<()> {
            let config = SystemConfig::default();
            executor.set_config(config).await?;
            write_cli_config(&CliConfig::default())?;

            if format.is_json() {
                return print_json(&json!({ "reset": true }));
            }

            println!(
                "Global configuration reset to defaults. Workspace overrides may still apply."
            );
            Ok(())
        }

        fn parse_value<T>(value: &str) -> Result<T>
        where
            T: std::str::FromStr,
            T::Err: std::fmt::Display,
        {
            value
                .parse::<T>()
                .map_err(|e| anyhow::anyhow!("Invalid value '{value}': {e}"))
        }

        fn parse_optional_u64(value: &str) -> Result<Option<u64>> {
            let normalized = value.trim();
            if normalized.eq_ignore_ascii_case("none")
                || normalized.eq_ignore_ascii_case("null")
                || normalized.eq_ignore_ascii_case("unset")
            {
                return Ok(None);
            }
            parse_value::<u64>(normalized).map(Some)
        }

        fn parse_optional_string(value: &str) -> Option<String> {
            let normalized = value.trim();
            if normalized.eq_ignore_ascii_case("none")
                || normalized.eq_ignore_ascii_case("null")
                || normalized.eq_ignore_ascii_case("unset")
            {
                return None;
            }
            Some(normalized.to_string())
        }

        fn parse_string_list(value: &str) -> Result<Vec<String>> {
            serde_json::from_str(value).map_err(|e| anyhow::anyhow!("Invalid JSON array: {}", e))
        }

        fn parse_optional_string_list(value: &str) -> Result<Option<Vec<String>>> {
            let normalized = value.trim();
            if normalized.eq_ignore_ascii_case("none")
                || normalized.eq_ignore_ascii_case("null")
                || normalized.eq_ignore_ascii_case("unset")
            {
                return Ok(None);
            }
            parse_string_list(normalized).map(Some)
        }

        fn format_optional_string(value: Option<&str>) -> String {
            value.unwrap_or("none").to_string()
        }

        fn format_string_list(values: &[String]) -> String {
            serde_json::to_string(values).unwrap_or_else(|_| "[]".to_string())
        }

        async fn load_effective_config_document(
            executor: Arc<dyn CommandExecutor>,
        ) -> Result<ConfigDocument> {
            let system = executor.get_config().await?;
            let cli = load_cli_config()?;
            Ok(ConfigDocument::from_system_config(system, cli))
        }

        fn format_source_info(source: &Option<ConfigSourcePathInfo>) -> String {
            match source {
                Some(info) => {
                    let exists = if info.exists { "exists" } else { "missing" };
                    let origin = if info.from_env { "env" } else { "default" };
                    format!("{} ({exists}, {origin})", info.path)
                }
                None => "none".to_string(),
            }
        }

        fn format_optional_u64(value: Option<u64>) -> String {
            value
                .map(|secs| secs.to_string())
                .unwrap_or_else(|| "none".to_string())
        }

        #[cfg(test)]
        #[allow(clippy::await_holding_lock)]
        mod tests {
            use super::*;
            use crate::executor::{CommandExecutor, direct::DirectExecutor};
            use ::daemon::storage::{load_cli_config, load_global_cli_config};
            use std::env;
            use std::path::Path;
            use tempfile::tempdir;

            struct EnvGuard {
                key: &'static str,
                original: Option<std::ffi::OsString>,
            }

            impl EnvGuard {
                fn set_path(key: &'static str, path: &Path) -> Self {
                    let original = env::var_os(key);
                    unsafe {
                        env::set_var(key, path);
                    }
                    Self { key, original }
                }
            }

            impl Drop for EnvGuard {
                fn drop(&mut self) {
                    if let Some(value) = &self.original {
                        unsafe {
                            env::set_var(self.key, value);
                        }
                    } else {
                        unsafe {
                            env::remove_var(self.key);
                        }
                    }
                }
            }

            fn env_lock() -> std::sync::MutexGuard<'static, ()> {
                crate::test_support::env_lock()
            }

            struct TestContext {
                executor: Arc<dyn CommandExecutor>,
                _temp_dir: tempfile::TempDir,
                _restflow_dir_guard: EnvGuard,
                _global_config_guard: EnvGuard,
                _env_lock: std::sync::MutexGuard<'static, ()>,
            }

            async fn setup_executor() -> TestContext {
                let env_guard = env_lock();
                let temp_dir = tempdir().expect("tempdir");
                let restflow_dir_guard = EnvGuard::set_path("RESTFLOW_DIR", temp_dir.path());
                let config_path = temp_dir.path().join("config.toml");
                let global_config_guard =
                    EnvGuard::set_path("RESTFLOW_GLOBAL_CONFIG", &config_path);
                let db_path = temp_dir.path().join("restflow.db");
                let direct_executor = DirectExecutor::connect(Some(path_to_string(&db_path)))
                    .await
                    .expect("connect direct executor");
                let executor: Arc<dyn CommandExecutor> = Arc::new(direct_executor);

                TestContext {
                    executor,
                    _temp_dir: temp_dir,
                    _restflow_dir_guard: restflow_dir_guard,
                    _global_config_guard: global_config_guard,
                    _env_lock: env_guard,
                }
            }

            fn path_to_string(path: &Path) -> String {
                path.to_string_lossy().into_owned()
            }

            #[test]
            fn test_parse_optional_u64_none_aliases() {
                assert_eq!(parse_optional_u64("none").unwrap(), None);
                assert_eq!(parse_optional_u64("null").unwrap(), None);
                assert_eq!(parse_optional_u64("unset").unwrap(), None);
            }

            #[test]
            fn test_parse_optional_u64_number() {
                assert_eq!(parse_optional_u64("3600").unwrap(), Some(3600));
            }

            #[test]
            fn test_format_optional_u64() {
                assert_eq!(format_optional_u64(Some(42)), "42");
                assert_eq!(format_optional_u64(None), "none");
            }

            #[tokio::test]
            async fn test_set_config_supports_log_file_retention_days() {
                let ctx = setup_executor().await;

                set_config_value(
                    ctx.executor.clone(),
                    "system.log_file_retention_days",
                    "45",
                    OutputFormat::Json,
                )
                .await
                .expect("set config should succeed");

                let config = ctx.executor.get_config().await.expect("get config");
                assert_eq!(config.log_file_retention_days, 45);
            }

            #[tokio::test]
            async fn test_set_config_supports_agent_max_depth() {
                let ctx = setup_executor().await;

                set_config_value(
                    ctx.executor.clone(),
                    "agent.max_depth",
                    "4",
                    OutputFormat::Json,
                )
                .await
                .expect("set config should support agent.max_depth");

                let config = ctx.executor.get_config().await.expect("get config");
                assert_eq!(config.agent.max_depth, 4);
            }

            #[tokio::test]
            async fn test_set_config_supports_agent_auto_review_tools() {
                let ctx = setup_executor().await;

                set_config_value(
                    ctx.executor.clone(),
                    "agent.auto_review_tools",
                    "true",
                    OutputFormat::Json,
                )
                .await
                .expect("set config should support agent.auto_review_tools");

                let config = ctx.executor.get_config().await.expect("get config");
                assert!(config.agent.auto_review_tools);
            }

            #[tokio::test]
            async fn test_get_config_supports_log_file_retention_days() {
                let ctx = setup_executor().await;
                let mut config = ctx
                    .executor
                    .get_global_config()
                    .await
                    .expect("get global config");
                config.log_file_retention_days = 21;
                ctx.executor
                    .set_config(config)
                    .await
                    .expect("persist config");

                get_config_value(
                    ctx.executor.clone(),
                    "system.log_file_retention_days",
                    OutputFormat::Json,
                )
                .await
                .expect("get config should support system.log_file_retention_days");
            }

            #[tokio::test]
            async fn test_set_config_supports_cli_agent() {
                let ctx = setup_executor().await;

                set_config_value(
                    ctx.executor.clone(),
                    "cli.agent",
                    "planner",
                    OutputFormat::Json,
                )
                .await
                .expect("set config should support cli.agent");

                let cli = load_cli_config().expect("load cli config");
                assert_eq!(cli.agent.as_deref(), Some("planner"));
            }

            #[tokio::test]
            async fn test_set_config_cli_write_preserves_workspace_overrides() {
                let ctx = setup_executor().await;
                let workspace_path = ctx._temp_dir.path().join("workspace-config.toml");
                std::fs::write(&workspace_path, "[cli]\nagent = \"workspace-agent\"\n")
                    .expect("write workspace config");
                let _workspace_guard =
                    EnvGuard::set_path("RESTFLOW_WORKSPACE_CONFIG", &workspace_path);

                set_config_value(
                    ctx.executor.clone(),
                    "cli.model",
                    "gpt-5",
                    OutputFormat::Json,
                )
                .await
                .expect("set config should support cli.model");

                let global_cli = load_global_cli_config().expect("load global cli config");
                assert_eq!(global_cli.agent, None);
                assert_eq!(global_cli.model.as_deref(), Some("gpt-5"));

                let effective_cli = load_cli_config().expect("load effective cli config");
                assert_eq!(effective_cli.agent.as_deref(), Some("workspace-agent"));
                assert_eq!(effective_cli.model.as_deref(), Some("gpt-5"));
            }

            #[tokio::test]
            async fn test_set_config_supports_clearing_agent_fallback_models() {
                let ctx = setup_executor().await;

                set_config_value(
                    ctx.executor.clone(),
                    "agent.fallback_models",
                    "[\"glm-5\", \"gpt-5\"]",
                    OutputFormat::Json,
                )
                .await
                .expect("set fallback models should succeed");

                set_config_value(
                    ctx.executor.clone(),
                    "agent.fallback_models",
                    "null",
                    OutputFormat::Json,
                )
                .await
                .expect("clearing fallback models should succeed");

                let config = ctx
                    .executor
                    .get_global_config()
                    .await
                    .expect("get global config");
                assert_eq!(config.agent.fallback_models, None);
            }

            #[tokio::test]
            async fn test_get_config_supports_effective_sources_aliases() {
                let ctx = setup_executor().await;

                get_config_value(
                    ctx.executor.clone(),
                    "effective_sources",
                    OutputFormat::Json,
                )
                .await
                .expect("get config should support effective_sources");

                get_config_value(
                    ctx.executor.clone(),
                    "_effective_sources",
                    OutputFormat::Json,
                )
                .await
                .expect("get config should support _effective_sources");
            }
        }
    }

    pub mod daemon {
        use crate::cli::DaemonCommands;
        use crate::commands::daemon_state::{self, EffectiveDaemonStatus, RunningSource};
        use ::daemon::AppCore;
        use ::daemon::daemon::{DaemonConfig, IpcServer, start_daemon_with_config, stop_daemon};
        use ::daemon::paths;
        use anyhow::{Context, Result};
        use std::path::PathBuf;
        #[cfg(not(unix))]
        use std::process::Command;
        use std::sync::Arc;
        use tokio::time::{Duration, sleep};
        use tracing::{error, info, warn};

        #[cfg(unix)]
        use nix::libc;

        const CLEANUP_INTERVAL_HOURS: u64 = 24;
        const DAEMON_STOP_TIMEOUT: Duration = Duration::from_secs(30);
        const DAEMON_STOP_POLL_INTERVAL: Duration = Duration::from_millis(200);

        pub async fn restart_background() -> Result<()> {
            let config = DaemonConfig::default();

            let previous_pid = current_daemon_pid().await?;
            let was_running = stop_daemon_effective().await?;
            if was_running {
                println!("Sent stop signal to daemon");
                wait_for_daemon_exit(previous_pid).await?;
            }

            // Clean stale artifacts that may remain after an unclean shutdown.
            let report = ::daemon::daemon::recovery::recover().await?;
            if !report.is_clean() {
                println!("{}", report);
            }

            let pid =
                tokio::task::spawn_blocking(move || start_daemon_with_config(config)).await??;
            if was_running {
                println!("Daemon restarted (PID: {})", pid);
            } else {
                println!("Daemon started (PID: {})", pid);
            }
            Ok(())
        }

        pub async fn run(core: Arc<AppCore>, command: DaemonCommands) -> Result<()> {
            match command {
                DaemonCommands::Start { foreground } => start(core, foreground).await,
                DaemonCommands::Restart { foreground } => restart(core, foreground).await,
                DaemonCommands::Stop => stop().await,
                DaemonCommands::Status => status().await,
            }
        }

        /// Run daemon commands that do not require opening AppCore.
        /// Returns true when the command is handled and the caller should return.
        pub async fn run_without_core(command: &DaemonCommands) -> Result<bool> {
            if !should_run_without_core(command) {
                return Ok(false);
            }

            match command {
                DaemonCommands::Start { foreground: false } => {
                    start_background().await?;
                    Ok(true)
                }
                DaemonCommands::Restart { foreground: false } => {
                    restart_background().await?;
                    Ok(true)
                }
                DaemonCommands::Stop => {
                    stop().await?;
                    Ok(true)
                }
                DaemonCommands::Status => {
                    status().await?;
                    Ok(true)
                }
                DaemonCommands::Start { .. } | DaemonCommands::Restart { .. } => Ok(false),
            }
        }

        fn should_run_without_core(command: &DaemonCommands) -> bool {
            matches!(
                command,
                DaemonCommands::Start { foreground: false }
                    | DaemonCommands::Restart { foreground: false }
                    | DaemonCommands::Stop
                    | DaemonCommands::Status
            )
        }

        async fn start_background() -> Result<()> {
            let config = DaemonConfig::default();

            let snapshot = daemon_state::collect_daemon_status_snapshot(false).await?;
            if let EffectiveDaemonStatus::Running { pid, .. } = snapshot.daemon_status {
                print_already_running(pid);
                return Ok(());
            }

            let report = ::daemon::daemon::recovery::recover().await?;
            if !report.is_clean() {
                println!("{}", report);
            }
            let pid =
                tokio::task::spawn_blocking(move || start_daemon_with_config(config)).await??;
            println!("Daemon started (PID: {})", pid);
            Ok(())
        }

        async fn start(core: Arc<AppCore>, foreground: bool) -> Result<()> {
            let config = DaemonConfig::default();

            if foreground {
                // In foreground mode, clean stale artifacts before binding.
                let report = ::daemon::daemon::recovery::recover().await?;
                if !report.is_clean() {
                    println!("{}", report);
                }
                run_daemon(core, config).await
            } else {
                let snapshot = daemon_state::collect_daemon_status_snapshot(false).await?;
                if let EffectiveDaemonStatus::Running { pid, .. } = snapshot.daemon_status {
                    print_already_running(pid);
                    Ok(())
                } else {
                    // Clean stale artifacts (e.g. leftover socket) before spawning.
                    let report = ::daemon::daemon::recovery::recover().await?;
                    if !report.is_clean() {
                        println!("{}", report);
                    }
                    let pid = tokio::task::spawn_blocking(move || start_daemon_with_config(config))
                        .await??;
                    println!("Daemon started (PID: {})", pid);
                    Ok(())
                }
            }
        }

        fn print_already_running(pid: Option<u32>) {
            if let Some(pid) = pid {
                println!("Daemon already running (PID: {})", pid);
            } else {
                println!("Daemon already running (PID: unknown)");
            }
        }

        async fn restart(core: Arc<AppCore>, foreground: bool) -> Result<()> {
            if foreground {
                let config = DaemonConfig::default();
                let previous_pid = current_daemon_pid().await?;
                let was_running = stop_daemon_effective().await?;
                if was_running {
                    println!("Sent stop signal to daemon");
                    wait_for_daemon_exit(previous_pid).await?;
                }
                // Clean stale artifacts that may remain after an unclean shutdown.
                let report = ::daemon::daemon::recovery::recover().await?;
                if !report.is_clean() {
                    println!("{}", report);
                }
                run_daemon(core, config).await
            } else {
                restart_background().await
            }
        }

        async fn run_daemon(core: Arc<AppCore>, config: DaemonConfig) -> Result<()> {
            #[cfg(unix)]
            configure_nofile_limit();

            let lock_path = paths::daemon_lock_path()?;
            let _lock_guard = DaemonLockGuard::acquire(lock_path)?;

            let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);

            #[cfg(unix)]
            {
                let shutdown_tx = shutdown_tx.clone();
                tokio::spawn(async move {
                    let mut sigterm =
                        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                            .unwrap();

                    tokio::select! {
                        _ = sigterm.recv() => {
                            let _ = shutdown_tx.send(());
                        }
                        _ = tokio::signal::ctrl_c() => {
                            let _ = shutdown_tx.send(());
                        }
                    }
                });
            }

            #[cfg(not(unix))]
            {
                let shutdown_tx = shutdown_tx.clone();
                tokio::spawn(async move {
                    let _ = tokio::signal::ctrl_c().await;
                    let _ = shutdown_tx.send(());
                });
            }

            let socket_path = paths::socket_path()?;
            let ipc_server = IpcServer::new(core.clone(), socket_path);
            let ipc_shutdown = shutdown_tx.subscribe();
            let ipc_handle = tokio::spawn(async move {
                if let Err(err) = ipc_server.run(ipc_shutdown).await {
                    error!(error = %err, "IPC server stopped unexpectedly");
                }
            });

            let _ = config;

            if let Err(err) = run_and_log_cleanup(core.clone()).await {
                warn!(error = %err, "Startup cleanup failed");
            }

            let cleanup_shutdown = shutdown_tx.subscribe();
            let cleanup_core = core.clone();
            let cleanup_handle = tokio::spawn(async move {
                run_cleanup_loop(cleanup_core, cleanup_shutdown).await;
            });

            // Ensure core services did not fail immediately before declaring daemon as running.
            sleep(Duration::from_millis(120)).await;
            ensure_startup_services(ipc_handle.is_finished())?;

            let pid_path = paths::daemon_pid_path()?;
            std::fs::write(&pid_path, std::process::id().to_string())?;
            let _pid_guard = PidFileGuard::new(pid_path.clone());

            println!("Daemon running. Press Ctrl+C to stop.");

            let mut shutdown_rx = shutdown_tx.subscribe();
            let _ = shutdown_rx.recv().await;

            let _ = ipc_handle.await;
            let _ = cleanup_handle.await;

            println!("Daemon stopped");
            Ok(())
        }

        fn ensure_startup_services(ipc_finished: bool) -> Result<()> {
            if ipc_finished {
                anyhow::bail!("IPC server exited during startup");
            }

            Ok(())
        }

        async fn run_cleanup_loop(
            core: Arc<AppCore>,
            mut shutdown: tokio::sync::broadcast::Receiver<()>,
        ) {
            let mut interval =
                tokio::time::interval(Duration::from_secs(CLEANUP_INTERVAL_HOURS * 60 * 60));
            interval.tick().await;
            loop {
                tokio::select! {
                    _ = shutdown.recv() => break,
                    _ = interval.tick() => {
                        if let Err(err) = run_and_log_cleanup(core.clone()).await {
                            warn!(error = %err, "Scheduled cleanup failed");
                        }
                    }
                }
            }
        }

        async fn run_and_log_cleanup(core: Arc<AppCore>) -> Result<()> {
            let report = ::daemon::services::cleanup::run_cleanup(&core).await?;
            info!(
                chat_sessions = report.chat_sessions,
                daemon_logs = report.daemon_log_files,
                "Storage cleanup completed"
            );
            Ok(())
        }

        #[cfg(unix)]
        fn configure_nofile_limit() {
            const TARGET_NOFILE: libc::rlim_t = 8192;

            let mut limits = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };

            // SAFETY: `limits` points to initialized writable memory and `RLIMIT_NOFILE`
            // is a valid resource kind on Unix.
            let got_limits = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limits) };
            if got_limits != 0 {
                warn!(
                    errno = std::io::Error::last_os_error().to_string(),
                    "Failed to read RLIMIT_NOFILE"
                );
                return;
            }

            let hard_cap = if limits.rlim_max == libc::RLIM_INFINITY {
                TARGET_NOFILE
            } else {
                limits.rlim_max.min(TARGET_NOFILE)
            };

            if limits.rlim_cur >= hard_cap {
                return;
            }

            let desired = libc::rlimit {
                rlim_cur: hard_cap,
                rlim_max: limits.rlim_max,
            };

            // SAFETY: `desired` contains valid values derived from current rlimit.
            let set_limits = unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &desired) };
            if set_limits == 0 {
                info!(
                    previous_soft = limits.rlim_cur,
                    new_soft = hard_cap,
                    hard = limits.rlim_max,
                    "Raised RLIMIT_NOFILE soft limit for daemon process"
                );
            } else {
                warn!(
                    errno = std::io::Error::last_os_error().to_string(),
                    requested_soft = hard_cap,
                    hard = limits.rlim_max,
                    "Failed to raise RLIMIT_NOFILE soft limit"
                );
            }
        }

        async fn stop() -> Result<()> {
            if stop_daemon_effective().await? {
                println!("Sent stop signal to daemon");
                wait_for_daemon_exit_or_kill().await?;
                println!("Daemon stopped");
            } else {
                println!("Daemon not running");
            }
            Ok(())
        }

        async fn stop_daemon_effective() -> Result<bool> {
            if stop_daemon()? {
                return Ok(true);
            }

            let snapshot = daemon_state::collect_daemon_status_snapshot(false).await?;
            if let EffectiveDaemonStatus::Running { pid: Some(pid), .. } = snapshot.daemon_status {
                send_terminate_signal(pid)?;
                return Ok(true);
            }

            Ok(false)
        }

        async fn current_daemon_pid() -> Result<Option<u32>> {
            let snapshot = daemon_state::collect_daemon_status_snapshot(false).await?;
            Ok(daemon_snapshot_pid(&snapshot))
        }

        fn daemon_snapshot_pid(snapshot: &daemon_state::DaemonStatusSnapshot) -> Option<u32> {
            match snapshot.daemon_status {
                EffectiveDaemonStatus::Running { pid, .. } => pid,
                EffectiveDaemonStatus::Stale { pid } => Some(pid),
                EffectiveDaemonStatus::NotRunning => None,
            }
        }

        fn send_terminate_signal(pid: u32) -> Result<()> {
            #[cfg(unix)]
            {
                use nix::sys::signal::{Signal, kill};
                use nix::unistd::Pid;

                let pid_i32 = i32::try_from(pid)
                    .with_context(|| format!("Daemon PID {} exceeds i32 range", pid))?;
                kill(Pid::from_raw(pid_i32), Signal::SIGTERM)?;
            }

            #[cfg(not(unix))]
            {
                Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/F"])
                    .output()?;
            }

            Ok(())
        }

        async fn status() -> Result<()> {
            let snapshot = daemon_state::collect_daemon_status_snapshot(true).await?;

            match snapshot.daemon_status {
                EffectiveDaemonStatus::Running { pid, source } => {
                    match (pid, source) {
                        (Some(pid), RunningSource::PidFile) => {
                            println!("Daemon running (PID: {})", pid);
                        }
                        (Some(pid), RunningSource::LockFile) => {
                            println!("Daemon running (PID: {}, detected via lock file)", pid);
                        }
                        (Some(pid), RunningSource::SocketProbe) => {
                            println!("Daemon running (PID: {}, detected via socket)", pid);
                        }
                        (None, RunningSource::SocketProbe) => {
                            println!("Daemon running (PID: unknown, detected via socket)");
                        }
                        (None, RunningSource::PidFile | RunningSource::LockFile) => {
                            println!("Daemon running (PID: unknown)");
                        }
                    };
                }
                EffectiveDaemonStatus::NotRunning => {
                    println!("Daemon not running");
                    if let Some(report) = snapshot.auto_recovery {
                        println!("  {}", report);
                    }
                    if snapshot.stale_state == ::daemon::daemon::recovery::StaleState::StaleSocket {
                        println!(
                            "  Note: stale socket detected (run `daemon start` to auto-clean)"
                        );
                    }
                }
                EffectiveDaemonStatus::Stale { pid } => {
                    println!("Daemon not running (stale PID: {})", pid);
                    if matches!(
                        snapshot.stale_state,
                        ::daemon::daemon::recovery::StaleState::Both
                            | ::daemon::daemon::recovery::StaleState::StaleSocket
                    ) {
                        println!("  Note: stale socket also detected");
                    }
                    println!("  Hint: run `daemon start` or `daemon restart` to auto-clean");
                }
            }
            Ok(())
        }

        async fn wait_for_daemon_exit(previous_pid: Option<u32>) -> Result<()> {
            let deadline = tokio::time::Instant::now() + DAEMON_STOP_TIMEOUT;
            loop {
                let snapshot = daemon_state::collect_daemon_status_snapshot(false).await?;
                let previous_process_alive = previous_pid.is_some_and(is_process_alive);
                if !snapshot.is_running() && !previous_process_alive {
                    sleep(DAEMON_STOP_POLL_INTERVAL).await;
                    let confirmation = daemon_state::collect_daemon_status_snapshot(false).await?;
                    let previous_process_still_alive = previous_pid.is_some_and(is_process_alive);
                    if !confirmation.is_running() && !previous_process_still_alive {
                        return Ok(());
                    }
                }
                if tokio::time::Instant::now() >= deadline {
                    let mut detail = daemon_exit_wait_detail(&snapshot);
                    if let Some(pid) = previous_pid
                        && is_process_alive(pid)
                    {
                        detail.push_str(&format!("; previous pid={pid} still alive"));
                    }
                    anyhow::bail!(
                        "Daemon did not stop within {}s: {}",
                        DAEMON_STOP_TIMEOUT.as_secs(),
                        detail
                    );
                }
                sleep(DAEMON_STOP_POLL_INTERVAL).await;
            }
        }

        fn daemon_exit_wait_detail(snapshot: &daemon_state::DaemonStatusSnapshot) -> String {
            match snapshot.daemon_status {
                EffectiveDaemonStatus::Running {
                    pid: Some(pid),
                    source,
                } => format!("still running (pid={pid}, source={})", source.as_str()),
                EffectiveDaemonStatus::Running { pid: None, source } => {
                    format!("still running (pid=unknown, source={})", source.as_str())
                }
                EffectiveDaemonStatus::NotRunning => "status switched to not_running".to_string(),
                EffectiveDaemonStatus::Stale { pid } => format!("stale pid={pid}"),
            }
        }

        fn is_process_alive(pid: u32) -> bool {
            #[cfg(unix)]
            {
                use nix::sys::signal::kill;
                use nix::unistd::Pid;
                let Ok(pid_i32) = i32::try_from(pid) else {
                    return false;
                };
                kill(Pid::from_raw(pid_i32), None).is_ok()
            }

            #[cfg(not(unix))]
            {
                std::process::Command::new("tasklist")
                    .args(["/FI", &format!("PID eq {}", pid)])
                    .output()
                    .map(|output| {
                        String::from_utf8_lossy(&output.stdout).contains(&pid.to_string())
                    })
                    .unwrap_or(false)
            }
        }

        /// Wait for daemon to exit gracefully, then SIGKILL if it doesn't stop in time.
        ///
        /// Phase 1: Poll for graceful exit up to `DAEMON_STOP_TIMEOUT` (30s).
        /// Phase 2: If still alive, send SIGKILL and wait up to 5s more.
        async fn wait_for_daemon_exit_or_kill() -> Result<()> {
            const KILL_GRACE_PERIOD: Duration = Duration::from_secs(5);

            let deadline = tokio::time::Instant::now() + DAEMON_STOP_TIMEOUT;
            loop {
                let snapshot = daemon_state::collect_daemon_status_snapshot(false).await?;
                if !snapshot.is_running() {
                    return Ok(());
                }
                if tokio::time::Instant::now() >= deadline {
                    // Extract PID for SIGKILL
                    let pid = match snapshot.daemon_status {
                        EffectiveDaemonStatus::Running { pid: Some(pid), .. } => pid,
                        _ => {
                            anyhow::bail!(
                                "Daemon did not stop within {}s and PID is unknown; cannot force-kill",
                                DAEMON_STOP_TIMEOUT.as_secs()
                            );
                        }
                    };

                    warn!(
                        pid,
                        timeout_secs = DAEMON_STOP_TIMEOUT.as_secs(),
                        "Daemon did not stop gracefully, sending SIGKILL"
                    );
                    send_kill_signal(pid)?;

                    // Wait briefly for the kill to take effect
                    let kill_deadline = tokio::time::Instant::now() + KILL_GRACE_PERIOD;
                    loop {
                        let snap = daemon_state::collect_daemon_status_snapshot(false).await?;
                        if !snap.is_running() {
                            return Ok(());
                        }
                        if tokio::time::Instant::now() >= kill_deadline {
                            anyhow::bail!(
                                "Daemon (PID {}) still alive after SIGKILL; manual intervention required",
                                pid
                            );
                        }
                        sleep(DAEMON_STOP_POLL_INTERVAL).await;
                    }
                }
                sleep(DAEMON_STOP_POLL_INTERVAL).await;
            }
        }

        fn send_kill_signal(pid: u32) -> Result<()> {
            #[cfg(unix)]
            {
                use nix::sys::signal::{Signal, kill};
                use nix::unistd::Pid;

                let pid_i32 = i32::try_from(pid)
                    .with_context(|| format!("Daemon PID {} exceeds i32 range", pid))?;
                kill(Pid::from_raw(pid_i32), Signal::SIGKILL)?;
            }

            #[cfg(not(unix))]
            {
                Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/F"])
                    .output()?;
            }

            Ok(())
        }

        struct PidFileGuard {
            path: PathBuf,
        }

        impl PidFileGuard {
            fn new(path: PathBuf) -> Self {
                Self { path }
            }
        }

        impl Drop for PidFileGuard {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.path);
            }
        }

        struct DaemonLockGuard {
            path: PathBuf,
            #[cfg(unix)]
            _file: std::fs::File, // Keep file handle open for flock
        }

        impl DaemonLockGuard {
            fn acquire(path: PathBuf) -> Result<Self> {
                let current_pid = std::process::id();

                #[cfg(unix)]
                {
                    use std::fs::OpenOptions;
                    use std::io::Write;
                    use std::os::unix::io::AsRawFd;

                    // Create or open the lock file
                    let file = OpenOptions::new()
                        .create(true)
                        .write(true)
                        .truncate(true)
                        .open(&path)
                        .context("Failed to create daemon lock file")?;

                    // Try to acquire exclusive lock (non-blocking)
                    let rc =
                        unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };

                    if rc != 0 {
                        let err = std::io::Error::last_os_error();
                        if err.raw_os_error() == Some(libc::EWOULDBLOCK)
                            || err.raw_os_error() == Some(libc::EAGAIN)
                        {
                            anyhow::bail!(
                                "Daemon already running (lock file held by another process)"
                            );
                        }
                        anyhow::bail!("Failed to acquire daemon lock: {}", err);
                    }

                    // Write PID to lock file
                    write!(&file, "{}", current_pid)?;

                    Ok(Self { path, _file: file })
                }

                #[cfg(not(unix))]
                {
                    // Fallback for non-Unix platforms (still has TOCTOU but with reduced window)
                    let mut attempts = 0;
                    loop {
                        attempts += 1;

                        match std::fs::OpenOptions::new()
                            .create_new(true)
                            .write(true)
                            .open(&path)
                        {
                            Ok(mut lock_file) => {
                                use std::io::Write;
                                write!(lock_file, "{}", current_pid)?;
                                return Ok(Self { path });
                            }
                            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                                if let Some(lock_pid) = read_lock_pid(&path)
                                    && is_process_alive(lock_pid)
                                {
                                    anyhow::bail!(
                                        "Daemon already running (lock held by PID {})",
                                        lock_pid
                                    );
                                }
                                let _ = std::fs::remove_file(&path);
                                if attempts >= 2 {
                                    anyhow::bail!(
                                        "Failed to acquire daemon lock after removing stale lock file"
                                    );
                                }
                            }
                            Err(err) => return Err(err.into()),
                        }
                    }
                }
            }
        }

        impl Drop for DaemonLockGuard {
            fn drop(&mut self) {
                // Note: On Unix, the lock is automatically released when the file handle is dropped.
                // We still remove the file for cleanup.
                let _ = std::fs::remove_file(&self.path);
            }
        }

        #[cfg(not(unix))]
        fn read_lock_pid(path: &std::path::Path) -> Option<u32> {
            let content = std::fs::read_to_string(path).ok()?;
            content.trim().parse::<u32>().ok()
        }

        #[cfg(test)]
        mod tests {
            use super::{ensure_startup_services, should_run_without_core};
            use crate::cli::DaemonCommands;

            #[test]
            fn no_core_routing_accepts_background_start() {
                let command = DaemonCommands::Start { foreground: false };
                assert!(should_run_without_core(&command));
            }

            #[test]
            fn no_core_routing_rejects_foreground_start() {
                let command = DaemonCommands::Start { foreground: true };
                assert!(!should_run_without_core(&command));
            }

            #[test]
            fn no_core_routing_accepts_background_restart() {
                let command = DaemonCommands::Restart { foreground: false };
                assert!(should_run_without_core(&command));
            }

            #[test]
            fn no_core_routing_rejects_foreground_restart() {
                let command = DaemonCommands::Restart { foreground: true };
                assert!(!should_run_without_core(&command));
            }

            #[test]
            fn no_core_routing_accepts_stop_and_status() {
                assert!(should_run_without_core(&DaemonCommands::Stop));
                assert!(should_run_without_core(&DaemonCommands::Status));
            }

            #[test]
            fn startup_check_bails_on_ipc_failure() {
                let err = ensure_startup_services(true).expect_err("startup check should fail");
                assert!(err.to_string().contains("IPC server exited during startup"));
            }

            #[test]
            fn startup_check_passes_when_services_are_healthy() {
                ensure_startup_services(false).expect("startup check should pass");
            }
        }
    }

    pub mod daemon_state {
        use ::daemon::daemon::{self, DaemonStatus};
        use ::daemon::paths;
        use anyhow::Result;

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum RunningSource {
            PidFile,
            LockFile,
            SocketProbe,
        }

        impl RunningSource {
            pub fn as_str(self) -> &'static str {
                match self {
                    Self::PidFile => "pid_file",
                    Self::LockFile => "lock_file",
                    Self::SocketProbe => "socket_probe",
                }
            }
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum EffectiveDaemonStatus {
            Running {
                pid: Option<u32>,
                source: RunningSource,
            },
            NotRunning,
            Stale {
                pid: u32,
            },
        }

        #[derive(Debug, Clone)]
        pub struct DaemonStatusSnapshot {
            pub daemon_status: EffectiveDaemonStatus,
            pub auto_recovery: Option<String>,
            pub stale_state: ::daemon::daemon::recovery::StaleState,
            pub socket_path: std::path::PathBuf,
            pub pid_path: std::path::PathBuf,
            pub db_path: std::path::PathBuf,
        }

        impl DaemonStatusSnapshot {
            pub fn is_running(&self) -> bool {
                matches!(self.daemon_status, EffectiveDaemonStatus::Running { .. })
            }
        }

        pub async fn collect_daemon_status_snapshot(
            auto_recover_stale: bool,
        ) -> Result<DaemonStatusSnapshot> {
            let socket_path = paths::socket_path()?;
            let pid_path = paths::daemon_pid_path()?;
            let db_path = paths::database_path()?;
            let lock_path = paths::daemon_lock_path()?;

            let mut daemon_status = daemon::check_daemon_status()?;
            let mut auto_recovery = None;

            if auto_recover_stale && matches!(daemon_status, DaemonStatus::Stale { .. }) {
                let report = ::daemon::daemon::recovery::recover().await?;
                if !report.is_clean() {
                    auto_recovery = Some(report.to_string());
                }
                daemon_status = daemon::check_daemon_status()?;
            }

            let stale_state = ::daemon::daemon::recovery::inspect(&pid_path, &socket_path).await?;
            let socket_alive = daemon::is_daemon_available(&socket_path).await;
            let lock_pid = read_lock_pid(&lock_path);
            let lock_alive_pid = lock_pid.filter(|pid| is_process_alive(*pid));

            let daemon_status =
                resolve_effective_status(daemon_status, socket_alive, lock_alive_pid);

            Ok(DaemonStatusSnapshot {
                daemon_status,
                auto_recovery,
                stale_state,
                socket_path,
                pid_path,
                db_path,
            })
        }

        fn read_lock_pid(path: &std::path::Path) -> Option<u32> {
            std::fs::read_to_string(path)
                .ok()
                .and_then(|content| content.trim().parse::<u32>().ok())
        }

        fn resolve_effective_status(
            raw_status: DaemonStatus,
            socket_alive: bool,
            lock_alive_pid: Option<u32>,
        ) -> EffectiveDaemonStatus {
            match raw_status {
                DaemonStatus::Running { pid } => EffectiveDaemonStatus::Running {
                    pid: Some(pid),
                    source: RunningSource::PidFile,
                },
                DaemonStatus::Stale { pid } => EffectiveDaemonStatus::Stale { pid },
                DaemonStatus::NotRunning => {
                    if let Some(pid) = lock_alive_pid {
                        EffectiveDaemonStatus::Running {
                            pid: Some(pid),
                            source: RunningSource::LockFile,
                        }
                    } else if socket_alive {
                        EffectiveDaemonStatus::Running {
                            pid: None,
                            source: RunningSource::SocketProbe,
                        }
                    } else {
                        EffectiveDaemonStatus::NotRunning
                    }
                }
            }
        }

        fn is_process_alive(pid: u32) -> bool {
            #[cfg(unix)]
            {
                use nix::sys::signal::kill;
                use nix::unistd::Pid;
                let Ok(pid_i32) = i32::try_from(pid) else {
                    return false;
                };
                kill(Pid::from_raw(pid_i32), None).is_ok()
            }

            #[cfg(not(unix))]
            {
                std::process::Command::new("tasklist")
                    .args(["/FI", &format!("PID eq {}", pid)])
                    .output()
                    .map(|output| {
                        String::from_utf8_lossy(&output.stdout).contains(&pid.to_string())
                    })
                    .unwrap_or(false)
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            #[test]
            fn resolve_running_prefers_pid_file() {
                let status =
                    resolve_effective_status(DaemonStatus::Running { pid: 42 }, false, None);
                assert_eq!(
                    status,
                    EffectiveDaemonStatus::Running {
                        pid: Some(42),
                        source: RunningSource::PidFile
                    }
                );
            }

            #[test]
            fn resolve_running_from_lock_file_when_pid_missing() {
                let status = resolve_effective_status(DaemonStatus::NotRunning, false, Some(1234));
                assert_eq!(
                    status,
                    EffectiveDaemonStatus::Running {
                        pid: Some(1234),
                        source: RunningSource::LockFile
                    }
                );
            }

            #[test]
            fn resolve_running_from_socket_probe_when_no_pid_available() {
                let status = resolve_effective_status(DaemonStatus::NotRunning, true, None);
                assert_eq!(
                    status,
                    EffectiveDaemonStatus::Running {
                        pid: None,
                        source: RunningSource::SocketProbe
                    }
                );
            }

            #[test]
            fn resolve_not_running_when_no_evidence() {
                let status = resolve_effective_status(DaemonStatus::NotRunning, false, None);
                assert_eq!(status, EffectiveDaemonStatus::NotRunning);
            }

            #[test]
            fn resolve_stale_passthrough() {
                let status = resolve_effective_status(DaemonStatus::Stale { pid: 9 }, false, None);
                assert_eq!(status, EffectiveDaemonStatus::Stale { pid: 9 });
            }
        }
    }

    pub mod secret {
        use anyhow::Result;
        use comfy_table::{Cell, Table};
        use std::sync::Arc;

        use crate::cli::SecretCommands;
        use crate::commands::utils::format_timestamp;
        use crate::executor::CommandExecutor;
        use crate::output::{OutputFormat, json::print_json};
        use serde_json::json;

        pub async fn run(
            executor: Arc<dyn CommandExecutor>,
            command: SecretCommands,
            format: OutputFormat,
        ) -> Result<()> {
            match command {
                SecretCommands::List => list_secrets(executor, format).await,
                SecretCommands::Set { key, value } => {
                    set_secret(executor, &key, &value, format).await
                }
                SecretCommands::Delete { key } => delete_secret(executor, &key, format).await,
                SecretCommands::Has { key } => has_secret(executor, &key, format).await,
            }
        }

        async fn list_secrets(
            executor: Arc<dyn CommandExecutor>,
            format: OutputFormat,
        ) -> Result<()> {
            let secrets = executor.list_secrets().await?;

            if format.is_json() {
                return print_json(&secrets);
            }

            let mut table = Table::new();
            table.set_header(vec!["Key", "Updated"]);

            for secret in secrets {
                table.add_row(vec![
                    Cell::new(secret.key),
                    Cell::new(format_timestamp(Some(secret.updated_at))),
                ]);
            }

            crate::output::table::print_table(table)
        }

        async fn set_secret(
            executor: Arc<dyn CommandExecutor>,
            key: &str,
            value: &str,
            format: OutputFormat,
        ) -> Result<()> {
            executor.set_secret(key, value, None).await?;

            if format.is_json() {
                return print_json(&json!({ "set": true, "key": key }));
            }

            println!("Secret set: {key}");
            Ok(())
        }

        async fn delete_secret(
            executor: Arc<dyn CommandExecutor>,
            key: &str,
            format: OutputFormat,
        ) -> Result<()> {
            executor.delete_secret(key).await?;

            if format.is_json() {
                return print_json(&json!({ "deleted": true, "key": key }));
            }

            println!("Secret deleted: {key}");
            Ok(())
        }

        async fn has_secret(
            executor: Arc<dyn CommandExecutor>,
            key: &str,
            format: OutputFormat,
        ) -> Result<()> {
            let exists = executor.has_secret(key).await?;

            if format.is_json() {
                return print_json(&json!({ "key": key, "exists": exists }));
            }

            if exists {
                println!("Secret exists: {key}");
            } else {
                println!("Secret not found: {key}");
            }
            Ok(())
        }
    }

    pub mod session {
        use anyhow::{Result, bail};
        use comfy_table::{Cell, Table};
        use serde_json::json;
        use std::sync::Arc;

        use crate::cli::SessionCommands;
        use crate::commands::utils::{format_timestamp, short_id};
        use crate::executor::CommandExecutor;
        use crate::output::{OutputFormat, json::print_json};
        use types::ChatRole;

        pub async fn run(
            executor: Arc<dyn CommandExecutor>,
            command: SessionCommands,
            format: OutputFormat,
        ) -> Result<()> {
            match command {
                SessionCommands::List => list_sessions(executor, format).await,
                SessionCommands::Show { id } => show_session(executor, &id, format).await,
                SessionCommands::Create { agent, model } => {
                    create_session(executor, &agent, &model, format).await
                }
                SessionCommands::Delete { id } => delete_session(executor, &id, format).await,
                SessionCommands::Search {
                    query,
                    agent,
                    limit,
                } => search_sessions(executor, &query, agent.as_deref(), limit, format).await,
            }
        }

        async fn list_sessions(
            executor: Arc<dyn CommandExecutor>,
            format: OutputFormat,
        ) -> Result<()> {
            let sessions = executor.list_sessions().await?;

            if format.is_json() {
                return print_json(&sessions);
            }

            let mut table = Table::new();
            table.set_header(vec!["ID", "Name", "Agent", "Model", "Messages", "Updated"]);

            for session in sessions {
                table.add_row(vec![
                    Cell::new(short_id(&session.id)),
                    Cell::new(session.name),
                    Cell::new(session.agent_id),
                    Cell::new(session.model),
                    Cell::new(session.message_count),
                    Cell::new(format_timestamp(Some(session.updated_at))),
                ]);
            }

            crate::output::table::print_table(table)
        }

        async fn show_session(
            executor: Arc<dyn CommandExecutor>,
            id: &str,
            format: OutputFormat,
        ) -> Result<()> {
            let resolved_id = resolve_session_id(executor.clone(), id).await?;
            let session = executor.get_session(&resolved_id).await?;

            if format.is_json() {
                return print_json(&session);
            }

            println!("Session: {} ({})", session.name, session.id);
            println!("Agent: {}", session.agent_id);
            println!("Model: {}", session.model);
            println!("Messages: {}", session.messages.len());
            println!("Updated: {}", format_timestamp(Some(session.updated_at)));
            println!();

            for msg in &session.messages {
                let role = match msg.role {
                    ChatRole::User => "User",
                    ChatRole::Assistant => "Assistant",
                    ChatRole::System => "System",
                };

                println!("{}", role);
                println!("{}", msg.content);
                println!();
            }

            Ok(())
        }

        async fn create_session(
            executor: Arc<dyn CommandExecutor>,
            agent: &str,
            model: &str,
            format: OutputFormat,
        ) -> Result<()> {
            let session = executor
                .create_session(
                    Some(agent.to_string()),
                    Some(model.to_string()),
                    Some("New Chat".to_string()),
                    None,
                )
                .await?;

            if format.is_json() {
                return print_json(&session);
            }

            println!("Created session: {}", session.id);
            Ok(())
        }

        async fn delete_session(
            executor: Arc<dyn CommandExecutor>,
            id: &str,
            format: OutputFormat,
        ) -> Result<()> {
            let resolved = match resolve_session_id_optional(executor.clone(), id).await? {
                Some(id) => id,
                None => {
                    if format.is_json() {
                        return print_json(&json!({ "deleted": false, "id": id }));
                    }
                    println!("Session not found: {}", id);
                    return Ok(());
                }
            };

            let deleted = executor.delete_session(&resolved).await?;

            if format.is_json() {
                return print_json(&json!({
                    "deleted": deleted,
                    "id": resolved,
                }));
            }

            if deleted {
                println!("Deleted session: {}", resolved);
            } else {
                println!("Session not found: {}", resolved);
            }

            Ok(())
        }

        async fn search_sessions(
            executor: Arc<dyn CommandExecutor>,
            query: &str,
            agent: Option<&str>,
            limit: usize,
            format: OutputFormat,
        ) -> Result<()> {
            let normalized = query.trim().to_lowercase();
            if normalized.is_empty() {
                bail!("Search query cannot be empty");
            }

            let results = executor
                .search_sessions(&normalized, agent, limit.max(1))
                .await?;

            if format.is_json() {
                return print_json(&results);
            }

            if results.is_empty() {
                println!("No sessions matched: {}", query);
                return Ok(());
            }

            for (index, result) in results.iter().enumerate() {
                println!("{}. {} ({})", index + 1, result.name, result.id);
                println!("   Agent: {}", result.agent_id);
                println!("   Model: {}", result.model);
                println!("   Messages: {}", result.message_count);
                println!("   Updated: {}", format_timestamp(Some(result.updated_at)));
                if let Some(ref preview) = result.last_message_preview {
                    println!("   Preview: {}", preview);
                }
                println!();
            }

            Ok(())
        }

        async fn resolve_session_id_optional(
            executor: Arc<dyn CommandExecutor>,
            id: &str,
        ) -> Result<Option<String>> {
            let sessions = executor.list_sessions().await?;
            if sessions.iter().any(|session| session.id == id) {
                return Ok(Some(id.to_string()));
            }
            let mut matches: Vec<_> = sessions
                .iter()
                .filter(|session| session.id.starts_with(id))
                .collect();

            match matches.len() {
                0 => Ok(None),
                1 => Ok(Some(matches.remove(0).id.clone())),
                _ => bail!("Session id is ambiguous: {}", id),
            }
        }

        async fn resolve_session_id(
            executor: Arc<dyn CommandExecutor>,
            id: &str,
        ) -> Result<String> {
            resolve_session_id_optional(executor, id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Session not found: {}", id))
        }
    }

    pub mod skill {
        use anyhow::Result;
        use comfy_table::{Cell, Table};
        use std::sync::Arc;

        use crate::cli::SkillCommands;
        use crate::commands::utils::format_timestamp;
        use crate::executor::CommandExecutor;
        use crate::output::{OutputFormat, json::print_json};
        use ::daemon::services::skills as skill_service;
        use serde_json::json;

        pub async fn run(
            executor: Arc<dyn CommandExecutor>,
            command: SkillCommands,
            format: OutputFormat,
        ) -> Result<()> {
            match command {
                SkillCommands::List => list_skills(executor, format).await,
                SkillCommands::Show { id } => show_skill(executor, &id, format).await,
                SkillCommands::Export { id, output } => {
                    export_skill(executor, &id, output, format).await
                }
            }
        }

        async fn list_skills(
            executor: Arc<dyn CommandExecutor>,
            format: OutputFormat,
        ) -> Result<()> {
            let skills = executor.list_skills().await?;

            if format.is_json() {
                return print_json(&skills);
            }

            let mut table = Table::new();
            table.set_header(vec!["ID", "Name", "Updated", "Tags"]);

            for skill in skills {
                let tags = skill
                    .tags
                    .as_ref()
                    .map(|values| values.join(", "))
                    .unwrap_or_else(|| "-".to_string());
                table.add_row(vec![
                    Cell::new(skill.id),
                    Cell::new(skill.name),
                    Cell::new(format_timestamp(Some(skill.updated_at))),
                    Cell::new(tags),
                ]);
            }

            crate::output::table::print_table(table)
        }

        async fn show_skill(
            executor: Arc<dyn CommandExecutor>,
            id: &str,
            format: OutputFormat,
        ) -> Result<()> {
            let skill = executor
                .get_skill(id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Skill not found: {}", id))?;

            if format.is_json() {
                return print_json(&skill);
            }

            println!("ID:          {}", skill.id);
            println!("Name:        {}", skill.name);
            println!(
                "Description: {}",
                skill.description.clone().unwrap_or_else(|| "-".to_string())
            );
            println!(
                "Tags:        {}",
                skill.tags.clone().unwrap_or_default().join(", ")
            );
            println!("Updated:     {}", format_timestamp(Some(skill.updated_at)));
            println!("\nContent:\n{}", skill.content);

            Ok(())
        }

        async fn export_skill(
            executor: Arc<dyn CommandExecutor>,
            id: &str,
            output: Option<String>,
            format: OutputFormat,
        ) -> Result<()> {
            let skill = executor
                .get_skill(id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Skill not found: {}", id))?;

            let markdown = skill_service::export_skill_to_markdown(&skill);
            let path = output.unwrap_or_else(|| format!("{}.md", id));
            std::fs::write(&path, markdown)?;

            if format.is_json() {
                return print_json(&json!({ "id": id, "output": path }));
            }

            println!("Exported to: {}", path);
            Ok(())
        }
    }

    pub mod upgrade {
        use anyhow::{Context, Result, bail};
        use flate2::read::GzDecoder;
        use reqwest::{Client, RequestBuilder, header};
        use serde::Deserialize;
        use serde_json::json;
        use std::fs;
        use std::io::{Cursor, Read};
        use std::path::{Path, PathBuf};
        use std::time::Duration;
        use tar::Archive;
        use zip::ZipArchive;

        use crate::cli::UpgradeArgs;
        use crate::output::{OutputFormat, json::print_json};

        const DEFAULT_REPO: &str = "lhwzds/restflow";
        const GITHUB_API_ACCEPT: &str = "application/vnd.github+json";

        #[derive(Debug, Deserialize)]
        struct GitHubRelease {
            tag_name: String,
            assets: Vec<GitHubAsset>,
        }

        #[derive(Debug, Deserialize)]
        struct GitHubAsset {
            name: String,
            browser_download_url: String,
        }

        #[derive(Debug, Clone, Copy)]
        enum ArchiveKind {
            TarGz,
            Zip,
        }

        #[derive(Debug, Clone, Copy)]
        struct PlatformSpec {
            asset_name: &'static str,
            binary_name: &'static str,
            archive_kind: ArchiveKind,
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum VersionRelation {
            LatestNewer,
            CurrentNewer,
            Equal,
            Unknown,
        }

        pub async fn run(args: UpgradeArgs, format: OutputFormat) -> Result<()> {
            let repo =
                std::env::var("RESTFLOW_UPGRADE_REPO").unwrap_or_else(|_| DEFAULT_REPO.to_string());
            let current_version = env!("CARGO_PKG_VERSION");

            let client = Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .context("Failed to initialize HTTP client")?;

            let release = fetch_latest_release(&client, &repo).await?;
            let latest_version = normalize_release_version(&release.tag_name);
            let relation = compare_versions(current_version, latest_version);

            if !args.force && relation == VersionRelation::Equal {
                return print_skip_result(
                    format,
                    "up_to_date",
                    current_version,
                    &release.tag_name,
                    None,
                    "Current version is already the latest release",
                );
            }

            if !args.force && relation == VersionRelation::CurrentNewer {
                return print_skip_result(
                    format,
                    "current_newer",
                    current_version,
                    &release.tag_name,
                    None,
                    "Current version is newer than the latest published release",
                );
            }

            let platform = detect_platform_spec()?;
            let asset = select_asset(&release.assets, platform.asset_name).with_context(|| {
                let available: Vec<_> = release
                    .assets
                    .iter()
                    .map(|item| item.name.clone())
                    .collect();
                format!(
                    "Release {} does not contain asset {}. Available assets: {}",
                    release.tag_name,
                    platform.asset_name,
                    available.join(", ")
                )
            })?;

            if !format.is_json() {
                println!(
                    "Latest release: {} ({})",
                    release.tag_name, platform.asset_name
                );
                println!("Downloading asset...");
            }

            let archive_bytes = download_asset(&client, &asset.browser_download_url)
                .await
                .with_context(|| format!("Failed to download {}", asset.name))?;

            if !format.is_json() {
                println!("Extracting binary...");
            }

            let binary_bytes =
                extract_binary(&archive_bytes, platform.archive_kind, platform.binary_name)?;

            let install_path = install_path()?;
            install_binary(&binary_bytes, &install_path)?;
            let alias_updated = ensure_rf_alias(&install_path)?;
            let codesigned = try_codesign(&install_path);

            if format.is_json() {
                return print_json(&json!({
                    "status": "upgraded",
                    "current_version": current_version,
                    "latest_tag": release.tag_name,
                    "installed_path": install_path,
                    "rf_alias_updated": alias_updated,
                    "codesigned": codesigned,
                    "forced": args.force,
                }));
            }

            println!("Upgrade complete.");
            println!("Version: {current_version} -> {latest_version}");
            println!("Installed: {}", install_path.display());
            if alias_updated {
                println!("Alias: rf -> restflow");
            }
            if cfg!(target_os = "macos") && !codesigned {
                println!(
                    "Warning: codesign step failed; binary may trigger macOS verification warnings."
                );
            }

            Ok(())
        }

        async fn fetch_latest_release(client: &Client, repo: &str) -> Result<GitHubRelease> {
            let release_url = format!("https://api.github.com/repos/{repo}/releases/latest");
            let response = with_github_headers(client.get(release_url))
                .send()
                .await
                .context("Failed to request latest release metadata")?
                .error_for_status()
                .context("GitHub API returned an error for latest release metadata")?;

            response
                .json::<GitHubRelease>()
                .await
                .context("Failed to decode latest release metadata")
        }

        async fn download_asset(client: &Client, url: &str) -> Result<Vec<u8>> {
            let response = with_github_headers(client.get(url))
                .send()
                .await
                .context("Failed to request release asset")?
                .error_for_status()
                .context("GitHub returned an error for release asset download")?;

            let bytes = response
                .bytes()
                .await
                .context("Failed to read release asset bytes")?;
            Ok(bytes.to_vec())
        }

        fn with_github_headers(request: RequestBuilder) -> RequestBuilder {
            let request = request
                .header(
                    header::USER_AGENT,
                    format!("restflow-cli/{}", env!("CARGO_PKG_VERSION")),
                )
                .header(header::ACCEPT, GITHUB_API_ACCEPT);

            match github_token() {
                Some(token) => request.bearer_auth(token),
                None => request,
            }
        }

        fn github_token() -> Option<String> {
            std::env::var("GITHUB_TOKEN")
                .ok()
                .or_else(|| std::env::var("GH_TOKEN").ok())
                .filter(|token| !token.trim().is_empty())
        }

        fn select_asset<'a>(
            assets: &'a [GitHubAsset],
            asset_name: &str,
        ) -> Option<&'a GitHubAsset> {
            assets.iter().find(|asset| asset.name == asset_name)
        }

        fn extract_binary(
            archive_bytes: &[u8],
            archive_kind: ArchiveKind,
            binary_name: &str,
        ) -> Result<Vec<u8>> {
            match archive_kind {
                ArchiveKind::TarGz => extract_from_tar_gz(archive_bytes, binary_name),
                ArchiveKind::Zip => extract_from_zip(archive_bytes, binary_name),
            }
        }

        fn extract_from_tar_gz(archive_bytes: &[u8], binary_name: &str) -> Result<Vec<u8>> {
            let gz = GzDecoder::new(Cursor::new(archive_bytes));
            let mut archive = Archive::new(gz);

            for entry in archive.entries().context("Failed to list tar entries")? {
                let mut entry = entry.context("Failed to read tar entry")?;
                let path = entry.path().context("Failed to read tar entry path")?;
                let file_name = path.file_name().and_then(|name| name.to_str());
                if file_name == Some(binary_name) {
                    let mut binary = Vec::new();
                    entry
                        .read_to_end(&mut binary)
                        .context("Failed to extract binary from tar archive")?;
                    return Ok(binary);
                }
            }

            bail!("Binary {} not found in tar.gz archive", binary_name);
        }

        fn extract_from_zip(archive_bytes: &[u8], binary_name: &str) -> Result<Vec<u8>> {
            let cursor = Cursor::new(archive_bytes);
            let mut archive = ZipArchive::new(cursor).context("Failed to open zip archive")?;

            for index in 0..archive.len() {
                let mut entry = archive
                    .by_index(index)
                    .with_context(|| format!("Failed to read zip entry {index}"))?;
                if entry.name().ends_with(binary_name) {
                    let mut binary = Vec::new();
                    entry
                        .read_to_end(&mut binary)
                        .context("Failed to extract binary from zip archive")?;
                    return Ok(binary);
                }
            }

            bail!("Binary {} not found in zip archive", binary_name);
        }

        fn detect_platform_spec() -> Result<PlatformSpec> {
            platform_spec_for(std::env::consts::OS, std::env::consts::ARCH).ok_or_else(|| {
                anyhow::anyhow!(
                    "Unsupported platform: {}-{}",
                    std::env::consts::ARCH,
                    std::env::consts::OS
                )
            })
        }

        fn platform_spec_for(os: &str, arch: &str) -> Option<PlatformSpec> {
            match (os, arch) {
                ("macos", "aarch64") => Some(PlatformSpec {
                    asset_name: "restflow-aarch64-apple-darwin.tar.gz",
                    binary_name: "restflow",
                    archive_kind: ArchiveKind::TarGz,
                }),
                ("macos", "x86_64") => Some(PlatformSpec {
                    asset_name: "restflow-x86_64-apple-darwin.tar.gz",
                    binary_name: "restflow",
                    archive_kind: ArchiveKind::TarGz,
                }),
                ("linux", "aarch64") => Some(PlatformSpec {
                    asset_name: "restflow-aarch64-unknown-linux-gnu.tar.gz",
                    binary_name: "restflow",
                    archive_kind: ArchiveKind::TarGz,
                }),
                ("linux", "x86_64") => Some(PlatformSpec {
                    asset_name: "restflow-x86_64-unknown-linux-gnu.tar.gz",
                    binary_name: "restflow",
                    archive_kind: ArchiveKind::TarGz,
                }),
                ("windows", "x86_64") => Some(PlatformSpec {
                    asset_name: "restflow-x86_64-pc-windows-msvc.zip",
                    binary_name: "restflow.exe",
                    archive_kind: ArchiveKind::Zip,
                }),
                _ => None,
            }
        }

        fn normalize_release_version(tag: &str) -> &str {
            tag.strip_prefix("cli-v")
                .or_else(|| tag.strip_prefix("v"))
                .unwrap_or(tag)
        }

        fn compare_versions(current: &str, latest: &str) -> VersionRelation {
            if current == latest {
                return VersionRelation::Equal;
            }

            let current_triplet = parse_semver_triplet(current);
            let latest_triplet = parse_semver_triplet(latest);

            match (current_triplet, latest_triplet) {
                (Some(current_triplet), Some(latest_triplet)) => {
                    match current_triplet.cmp(&latest_triplet) {
                        std::cmp::Ordering::Less => VersionRelation::LatestNewer,
                        std::cmp::Ordering::Greater => VersionRelation::CurrentNewer,
                        std::cmp::Ordering::Equal => VersionRelation::Equal,
                    }
                }
                _ => VersionRelation::Unknown,
            }
        }

        /// Parsed semver with optional prerelease tag.
        #[derive(Debug, Clone, PartialEq, Eq)]
        struct SemVer {
            major: u64,
            minor: u64,
            patch: u64,
            prerelease: Option<String>,
        }

        impl Ord for SemVer {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                let triplet_cmp = (self.major, self.minor, self.patch).cmp(&(
                    other.major,
                    other.minor,
                    other.patch,
                ));
                if triplet_cmp != std::cmp::Ordering::Equal {
                    return triplet_cmp;
                }
                // Per semver spec: prerelease < release for same triplet
                match (&self.prerelease, &other.prerelease) {
                    (None, None) => std::cmp::Ordering::Equal,
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (Some(a), Some(b)) => a.cmp(b),
                }
            }
        }

        impl PartialOrd for SemVer {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }

        fn parse_semver_triplet(version: &str) -> Option<SemVer> {
            // Split off build metadata first
            let without_build = version
                .split_once('+')
                .map(|(left, _)| left)
                .unwrap_or(version);

            // Split prerelease from core
            let (core, prerelease) = match without_build.split_once('-') {
                Some((c, p)) => (c, Some(p.to_string())),
                None => (without_build, None),
            };

            let mut parts = core.split('.');
            let major = parts.next()?.parse::<u64>().ok()?;
            let minor = parts.next()?.parse::<u64>().ok()?;
            let patch = parts.next()?.parse::<u64>().ok()?;

            Some(SemVer {
                major,
                minor,
                patch,
                prerelease,
            })
        }

        fn install_path() -> Result<PathBuf> {
            #[cfg(target_os = "windows")]
            {
                let base =
                    dirs::data_local_dir().context("Failed to resolve local data directory")?;
                return Ok(base.join("restflow").join("bin").join("restflow.exe"));
            }

            #[cfg(not(target_os = "windows"))]
            {
                let home = dirs::home_dir().context("Failed to resolve home directory")?;
                Ok(home.join(".local").join("bin").join("restflow"))
            }
        }

        fn install_binary(binary: &[u8], install_path: &Path) -> Result<()> {
            let parent = install_path
                .parent()
                .context("Install path must have a parent directory")?;
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create install directory {}",
                    parent.to_string_lossy()
                )
            })?;

            let temp_path = parent.join(format!(
                ".{}.tmp-{}",
                install_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("restflow"),
                std::process::id()
            ));

            fs::write(&temp_path, binary).with_context(|| {
                format!(
                    "Failed to write temporary binary {}",
                    temp_path.to_string_lossy()
                )
            })?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut permissions = fs::metadata(&temp_path)
                    .with_context(|| {
                        format!(
                            "Failed to read metadata for {}",
                            temp_path.to_string_lossy()
                        )
                    })?
                    .permissions();
                permissions.set_mode(0o755);
                fs::set_permissions(&temp_path, permissions).with_context(|| {
                    format!(
                        "Failed to set executable permission on {}",
                        temp_path.to_string_lossy()
                    )
                })?;
            }

            if install_path.exists() {
                fs::remove_file(install_path).with_context(|| {
                    format!(
                        "Failed to replace existing binary {}",
                        install_path.to_string_lossy()
                    )
                })?;
            }

            fs::rename(&temp_path, install_path).with_context(|| {
                format!(
                    "Failed to install binary at {}",
                    install_path.to_string_lossy()
                )
            })?;

            Ok(())
        }

        #[cfg(unix)]
        fn ensure_rf_alias(install_path: &Path) -> Result<bool> {
            use std::os::unix::fs::symlink;

            let alias_path = install_path
                .parent()
                .context("Install path must have a parent directory")?
                .join("rf");

            if fs::symlink_metadata(&alias_path).is_ok() {
                fs::remove_file(&alias_path).with_context(|| {
                    format!(
                        "Failed to remove existing rf alias {}",
                        alias_path.to_string_lossy()
                    )
                })?;
            }

            symlink(install_path, &alias_path).with_context(|| {
                format!(
                    "Failed to create rf alias at {}",
                    alias_path.to_string_lossy()
                )
            })?;

            Ok(true)
        }

        #[cfg(not(unix))]
        fn ensure_rf_alias(_: &Path) -> Result<bool> {
            Ok(false)
        }

        #[cfg(target_os = "macos")]
        fn try_codesign(install_path: &Path) -> bool {
            match std::process::Command::new("codesign")
                .arg("--force")
                .arg("--sign")
                .arg("-")
                .arg(install_path)
                .output()
            {
                Ok(output) => output.status.success(),
                Err(_) => false,
            }
        }

        #[cfg(not(target_os = "macos"))]
        fn try_codesign(_: &Path) -> bool {
            false
        }

        fn print_skip_result(
            format: OutputFormat,
            status: &str,
            current_version: &str,
            latest_tag: &str,
            installed_path: Option<&Path>,
            reason: &str,
        ) -> Result<()> {
            if format.is_json() {
                return print_json(&json!({
                    "status": status,
                    "current_version": current_version,
                    "latest_tag": latest_tag,
                    "installed_path": installed_path,
                    "reason": reason,
                }));
            }

            println!("{reason}.");
            println!("Current version: {current_version}");
            println!("Latest tag: {latest_tag}");
            Ok(())
        }

        #[cfg(test)]
        mod tests {
            use super::{
                ArchiveKind, VersionRelation, compare_versions, normalize_release_version,
                parse_semver_triplet, platform_spec_for,
            };

            #[test]
            fn normalizes_release_tags() {
                assert_eq!(normalize_release_version("cli-v0.2.0"), "0.2.0");
                assert_eq!(normalize_release_version("v1.0.0"), "1.0.0");
                assert_eq!(normalize_release_version("0.3.0"), "0.3.0");
            }

            #[test]
            fn compares_versions() {
                assert_eq!(
                    compare_versions("0.2.0", "0.2.1"),
                    VersionRelation::LatestNewer
                );
                assert_eq!(
                    compare_versions("0.2.1", "0.2.0"),
                    VersionRelation::CurrentNewer
                );
                assert_eq!(compare_versions("0.2.1", "0.2.1"), VersionRelation::Equal);
                assert_eq!(
                    compare_versions("main", "cli-v0.2.1"),
                    VersionRelation::Unknown
                );
            }

            #[test]
            fn parses_semver_triplets() {
                let v = parse_semver_triplet("1.2.3").unwrap();
                assert_eq!((v.major, v.minor, v.patch), (1, 2, 3));
                assert_eq!(v.prerelease, None);

                let v = parse_semver_triplet("1.2.3-beta.1").unwrap();
                assert_eq!((v.major, v.minor, v.patch), (1, 2, 3));
                assert_eq!(v.prerelease, Some("beta.1".to_string()));

                assert!(parse_semver_triplet("1.2").is_none());
            }

            #[test]
            fn prerelease_compares_correctly() {
                // prerelease < release for same triplet
                assert_eq!(
                    compare_versions("1.0.0-rc.1", "1.0.0"),
                    VersionRelation::LatestNewer
                );
                // alpha < beta (lexicographic)
                assert_eq!(
                    compare_versions("1.0.0-alpha", "1.0.0-beta"),
                    VersionRelation::LatestNewer
                );
                // same release version
                assert_eq!(compare_versions("1.0.0", "1.0.0"), VersionRelation::Equal);
            }

            #[test]
            fn resolves_platform_spec() {
                let mac = platform_spec_for("macos", "aarch64").expect("macOS aarch64 spec");
                assert_eq!(mac.asset_name, "restflow-aarch64-apple-darwin.tar.gz");
                assert_eq!(mac.binary_name, "restflow");
                assert!(matches!(mac.archive_kind, ArchiveKind::TarGz));

                let windows = platform_spec_for("windows", "x86_64").expect("windows spec");
                assert_eq!(windows.asset_name, "restflow-x86_64-pc-windows-msvc.zip");
                assert_eq!(windows.binary_name, "restflow.exe");
                assert!(matches!(windows.archive_kind, ArchiveKind::Zip));

                assert!(platform_spec_for("linux", "armv7").is_none());
            }
        }
    }

    pub mod utils {
        use chrono::{DateTime, Local, TimeZone};

        pub fn format_timestamp(timestamp: Option<i64>) -> String {
            let Some(ts) = timestamp else {
                return "-".to_string();
            };

            let datetime: DateTime<Local> = match Local.timestamp_millis_opt(ts).single() {
                Some(dt) => dt,
                None => return "-".to_string(),
            };

            datetime.format("%Y-%m-%d %H:%M:%S").to_string()
        }

        pub fn short_id(value: &str) -> String {
            value.chars().take(8).collect()
        }
    }

    pub mod info {
        use anyhow::Result;

        pub fn run() -> Result<()> {
            println!("RestFlow CLI {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }

    pub mod maintenance {
        use anyhow::Result;
        use serde_json::json;
        use std::sync::Arc;

        use crate::cli::MaintenanceCommands;
        use crate::executor::CommandExecutor;
        use crate::output::{OutputFormat, json::print_json};

        pub async fn run(
            executor: Arc<dyn CommandExecutor>,
            command: MaintenanceCommands,
            format: OutputFormat,
        ) -> Result<()> {
            match command {
                MaintenanceCommands::Cleanup => run_cleanup(executor, format).await,
            }
        }

        async fn run_cleanup(
            executor: Arc<dyn CommandExecutor>,
            format: OutputFormat,
        ) -> Result<()> {
            let report = executor.run_cleanup().await?;

            if format.is_json() {
                return print_json(&json!({
                    "chat_sessions": report.chat_sessions,
                    "daemon_log_files": report.daemon_log_files
                }));
            }

            println!("Cleanup finished:");
            println!("  chat_sessions: {}", report.chat_sessions);
            println!("  daemon_log_files: {}", report.daemon_log_files);
            Ok(())
        }
    }

    pub mod restart {
        use crate::cli::RestartArgs;
        use crate::commands::daemon::restart_background;
        use anyhow::Result;

        pub async fn run(_args: RestartArgs) -> Result<()> {
            restart_background().await
        }
    }

    pub mod start {
        use crate::cli::StartArgs;
        use ::daemon::daemon::ensure_daemon_running;
        use anyhow::Result;

        pub async fn run(args: StartArgs) -> Result<()> {
            let _ = args;
            ensure_daemon_running().await?;
            println!("RestFlow daemon started");
            Ok(())
        }
    }

    pub mod status {
        use anyhow::Result;
        use serde_json::json;

        use crate::commands::daemon_state::{self, EffectiveDaemonStatus, RunningSource};
        use crate::output::{OutputFormat, json::print_json};

        pub async fn run(format: OutputFormat) -> Result<()> {
            let snapshot = daemon_state::collect_daemon_status_snapshot(true).await?;

            let (status, pid, stale_pid, running_source) = match snapshot.daemon_status {
                EffectiveDaemonStatus::Running { pid, source } => {
                    ("running", pid, None, Some(source.as_str()))
                }
                EffectiveDaemonStatus::NotRunning => ("not_running", None, None, None),
                EffectiveDaemonStatus::Stale { pid } => ("stale", None, Some(pid), None),
            };

            if format.is_json() {
                return print_json(&json!({
                    "daemon_status": status,
                    "pid": pid,
                    "stale_pid": stale_pid,
                    "running_source": running_source,
                    "auto_recovery": snapshot.auto_recovery,
                    "socket_path": snapshot.socket_path,
                    "pid_path": snapshot.pid_path,
                    "db_path": snapshot.db_path,
                }));
            }

            println!("RestFlow Status");
            match snapshot.daemon_status {
                EffectiveDaemonStatus::Running {
                    pid: Some(pid),
                    source,
                } => {
                    if source == RunningSource::PidFile {
                        println!("Daemon: running (PID: {pid})");
                    } else {
                        println!("Daemon: running (PID: {pid}, source: {})", source.as_str());
                    }
                }
                EffectiveDaemonStatus::Running { pid: None, source } => {
                    println!(
                        "Daemon: running (PID: unknown, source: {})",
                        source.as_str()
                    );
                }
                EffectiveDaemonStatus::NotRunning => println!("Daemon: not running"),
                EffectiveDaemonStatus::Stale { pid } => {
                    println!("Daemon: stale pid file (PID: {pid})")
                }
            }
            if let Some(report) = snapshot.auto_recovery {
                println!("Auto-recovery: {report}");
            }
            println!("Socket: {}", snapshot.socket_path.display());
            println!("PID file: {}", snapshot.pid_path.display());
            println!("DB path: {}", snapshot.db_path.display());

            Ok(())
        }
    }

    pub mod stop {
        use ::daemon::daemon::stop_daemon;
        use anyhow::Result;

        pub async fn run() -> Result<()> {
            if stop_daemon()? {
                println!("RestFlow daemon stopped");
            } else {
                println!("RestFlow daemon not running");
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod test_support {
    use std::sync::{Mutex, OnceLock};

    pub fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

use ::daemon::paths;
use anyhow::Result;
use clap::{CommandFactory, Parser};
use clap_complete::generate;
use cli::{Cli, Commands};
use std::io;
use std::io::IsTerminal;
use tracing_appender::non_blocking::WorkerGuard;
use tui::{TuiLaunchOptions, run_tui};

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
            Some(Commands::Secret { command }) => {
                commands::secret::run(exec, command, cli.format).await
            }
            Some(Commands::Config { command }) => {
                commands::config::run(exec, command, cli.format).await
            }
            Some(Commands::Session { command }) => {
                commands::session::run(exec, command, cli.format).await
            }
            Some(Commands::Maintenance { command }) => {
                commands::maintenance::run(exec, command, cli.format).await
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
    use super::{command_needs_direct_core, executor_db_path_flag, should_launch_tui_by_default};
    use crate::cli::{Commands, MaintenanceCommands, StartArgs};

    #[test]
    fn start_does_not_need_direct_core() {
        let command = Some(Commands::Start(StartArgs::default()));
        assert!(!command_needs_direct_core(&command));
    }

    #[test]
    fn default_tui_launch_requires_no_command_and_tty() {
        assert!(should_launch_tui_by_default(&None, true, true));
        assert!(!should_launch_tui_by_default(&None, true, false));
        assert!(!should_launch_tui_by_default(
            &Some(Commands::Info),
            true,
            true
        ));
    }

    #[test]
    fn maintenance_does_not_need_direct_core() {
        let command = Some(Commands::Maintenance {
            command: MaintenanceCommands::Cleanup,
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
