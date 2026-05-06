//! Built-in tool implementations.

// Shared utilities
pub(crate) mod operation_assessment;
pub(crate) mod path_utils;
pub(crate) mod shared;
pub(crate) mod subagent_read_capability;

mod bash;
mod file;
mod skrun;

pub mod edit;
pub mod multiedit;

// Migrated from restflow-ai
pub mod agent_crud;
pub mod auth_profile;
pub mod config;
pub mod diagnostics;
pub mod file_tracker;
pub mod patch;
pub mod process;
pub mod reply;
pub mod secrets;
pub mod session;
pub mod skill;
pub mod switch_model;
pub mod task;

// Migrated from restflow-core (tool_registry inline tools)
pub mod manage_ops;
pub mod marketplace;
pub mod security_query;
pub mod terminal;

// Search tools
pub mod glob_tool;
pub mod grep_tool;

// Batch tool
pub mod batch;

// Migrated from restflow-core
pub mod list_subagents;
pub mod load_skill;
pub mod registry_builder;
pub mod spawn;
pub mod spawn_subagent;
pub mod spawn_subagent_batch;
pub mod wait_subagents;

// Re-export edit tools
pub use edit::EditTool;
pub use multiedit::MultiEditTool;

// Re-export original 7
pub use bash::{BashInput, BashOutput, BashTool};
pub use file::{FileAction, FileTool};
pub use skrun::RunSkillTool;

// Re-export migrated tools
pub use agent_crud::AgentCrudTool;
pub use auth_profile::AuthProfileTool;
pub use config::ConfigTool;
pub use diagnostics::DiagnosticsTool;
pub use patch::PatchTool;
pub use process::ProcessTool;
pub use reply::ReplyTool;
pub use secrets::{SecretGetPolicy, SecretsTool};
pub use session::SessionTool;
pub use skill::SkillTool;
pub use switch_model::SwitchModelTool;
pub use task::TaskTool;

// Re-export tool_registry inline migrated tools
pub use manage_ops::ManageOpsTool;
pub use marketplace::MarketplaceTool;
pub use security_query::SecurityQueryTool;
pub use terminal::TerminalTool;

// Re-export search tools
pub use glob_tool::GlobTool;
pub use grep_tool::GrepTool;

// Re-export batch tool
pub use batch::BatchTool;

// Re-export core-migrated tools
pub use list_subagents::ListSubagentsTool;
pub use load_skill::LoadSkillTool;
pub use registry_builder::{
    BashConfig, FileConfig, SecretsConfig, ToolRegistryBuilder, default_registry,
};
pub use spawn::SpawnTool;
pub use spawn_subagent::SpawnSubagentTool;
pub use spawn_subagent_batch::SpawnSubagentBatchTool;
pub use wait_subagents::WaitSubagentsTool;
