//! Built-in tool implementations.

// Original 7 tools
mod bash;
mod discord;
mod email;
mod file;
mod http;
mod slack;
pub mod telegram;

// Migrated from restflow-ai
pub mod agent_crud;
pub mod auth_profile;
pub mod background_agent;
pub mod config;
pub mod diagnostics;
pub mod file_tracker;
pub mod filtered;
pub mod jina_reader;
pub mod mcp_cache;
pub mod memory_mgmt;
pub mod memory_search;
pub mod memory_store;
pub mod monty_python;
pub mod patch;
pub mod process;
pub mod python_backend;
pub mod reply;
pub mod save_deliverable;
pub mod secrets;
pub mod session;
pub mod skill;
pub mod spawn_subtask;
pub mod switch_model;
pub mod transcribe;
pub mod vision;
pub mod web_fetch;
pub mod web_search;
pub mod workspace_note;

// Migrated from restflow-core (tool_registry inline tools)
pub mod manage_ops;
pub mod marketplace;
pub mod security_query;
pub mod shared_space;
pub mod terminal;
pub mod trigger;
pub mod unified_memory_search;

// Migrated from restflow-core
pub mod list_agents;
pub mod registry_builder;
pub mod spawn;
pub mod spawn_agent;
pub mod use_skill;
pub mod wait_agents;

// Re-export original 7
pub use bash::{BashInput, BashOutput, BashTool};
pub use discord::DiscordTool;
pub use email::EmailTool;
pub use file::{FileAction, FileTool};
pub use http::HttpTool;
pub use slack::SlackTool;
pub use telegram::{TelegramTool, send_telegram_notification};

// Re-export migrated tools
pub use agent_crud::AgentCrudTool;
pub use auth_profile::AuthProfileTool;
pub use background_agent::BackgroundAgentTool;
pub use config::ConfigTool;
pub use diagnostics::DiagnosticsTool;
pub use filtered::FilteredToolset;
pub use jina_reader::JinaReaderTool;
pub use mcp_cache::{McpServerConfig, get_mcp_tools, invalidate_mcp_cache};
pub use memory_mgmt::MemoryManagementTool;
pub use memory_search::{MemorySearchMatch, MemorySearchTool, SemanticMemory};
pub use memory_store::{DeleteMemoryTool, ListMemoryTool, ReadMemoryTool, SaveMemoryTool};
pub use monty_python::{PythonTool, RunPythonTool};
pub use patch::PatchTool;
pub use process::ProcessTool;
pub use python_backend::{PythonExecutionBackend, PythonExecutionLimits};
pub use reply::ReplyTool;
pub use save_deliverable::SaveDeliverableTool;
pub use secrets::SecretsTool;
pub use session::SessionTool;
pub use skill::SkillTool;
pub use spawn_subtask::{SpawnSubtaskRequest, SpawnSubtaskTool};
pub use switch_model::SwitchModelTool;
pub use transcribe::{TranscribeConfig, TranscribeTool};
pub use vision::VisionTool;
pub use web_fetch::WebFetchTool;
pub use web_search::WebSearchTool;
pub use workspace_note::WorkspaceNoteTool;

// Re-export tool_registry inline migrated tools
pub use manage_ops::ManageOpsTool;
pub use marketplace::MarketplaceTool;
pub use security_query::SecurityQueryTool;
pub use shared_space::SharedSpaceTool;
pub use terminal::TerminalTool;
pub use trigger::TriggerTool;
pub use unified_memory_search::UnifiedMemorySearchTool;

// Re-export core-migrated tools
pub use list_agents::ListAgentsTool;
pub use registry_builder::{BashConfig, FileConfig, ToolRegistryBuilder, default_registry};
pub use spawn::SpawnTool;
pub use spawn_agent::SpawnAgentTool;
pub use use_skill::UseSkillTool;
pub use wait_agents::WaitAgentsTool;
