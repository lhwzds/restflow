//! Unified tool registry for agent execution.
//!
//! Most components are canonical in restflow-core. Only platform-specific
//! tools (show_panel) and Tauri's LLM-only spawn remain local.

pub use restflow_ai::tools::{SecretResolver, Tool, ToolOutput, ToolRegistry};
pub use restflow_tools::{PythonTool, RunPythonTool, TranscribeTool, VisionTool};

// Independent tools from restflow-core (canonical implementations).
pub use restflow_core::runtime::agent::tools::{
    BashConfig, BashTool, EmailTool, FileConfig, FileTool, HttpTool, TelegramTool,
};

// Shared infrastructure from restflow-core.
pub use restflow_core::runtime::agent::tools::{
    ListAgentsTool, SubagentDeps, ToolRegistryBuilder, UseSkillTool, WaitAgentsTool,
    default_registry, registry_from_allowlist, secret_resolver_from_storage,
};

// Local: Tauri-specific tools and LLM-only spawn.
mod show_panel;
mod spawn;
mod spawn_agent;

pub use show_panel::ShowPanelTool;
pub use spawn::{SpawnTool, SubagentSpawner};
pub use spawn_agent::SpawnAgentTool;

pub type ToolResult = ToolOutput;

/// Tauri-specific default tools (adds show_panel on top of core defaults).
pub fn main_agent_default_tool_names() -> Vec<String> {
    let mut names = restflow_core::runtime::agent::tools::main_agent_default_tool_names();
    if !names.iter().any(|n| n == "show_panel") {
        names.push("show_panel".to_string());
    }
    names
}

/// Merge the default main-agent tools with agent-specific additions (includes show_panel).
pub fn effective_main_agent_tool_names(tool_names: Option<&[String]>) -> Vec<String> {
    let mut merged = main_agent_default_tool_names();
    if let Some(extra) = tool_names {
        for name in extra {
            if !merged.iter().any(|item| item == name) {
                merged.push(name.clone());
            }
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_main_agent_default_tool_names_includes_show_panel() {
        let tools = main_agent_default_tool_names();
        assert!(tools.iter().any(|name| name == "show_panel"));
        // Also includes core defaults
        assert!(tools.iter().any(|name| name == "bash"));
        assert!(tools.iter().any(|name| name == "web_search"));
    }

    #[test]
    fn test_effective_main_agent_tool_names_merges_without_duplicates() {
        let extra = vec!["custom_tool".to_string(), "bash".to_string()];
        let merged = effective_main_agent_tool_names(Some(&extra));
        assert!(merged.iter().any(|name| name == "custom_tool"));
        assert!(merged.iter().any(|name| name == "show_panel"));
        assert_eq!(
            merged.iter().filter(|name| name.as_str() == "bash").count(),
            1
        );
    }
}
