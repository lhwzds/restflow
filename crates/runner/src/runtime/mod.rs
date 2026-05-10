pub mod agent;
pub mod execution_context;
pub mod orchestrator;
pub mod session_runner;
pub mod session_turn;
pub mod subagent;

// Public surface rule:
// - `runner::runtime` re-exports durable runner-owned execution APIs.
// - AI-owned subagent runtime state stays exported from `agent` /
//   `types` so ownership remains unambiguous.
pub use self::agent::build_agent_system_prompt;
pub use self::agent::tools::{
    BashConfig, BashTool, FileConfig, FileTool, ListSubagentsTool, LoadSkillTool,
    SpawnSubagentTool, Tool, ToolRegistry, ToolRegistryBuilder, ToolResult, WaitSubagentsTool,
    default_registry, effective_main_agent_tool_names, main_agent_default_tool_names,
    registry_from_allowlist, secret_resolver_from_storage,
};
pub use execution_context::{ExecutionContext, ExecutionRole};
pub use orchestrator::AgentOrchestratorImpl;
pub use session_runner::{AgentRuntimeExecutor, SessionExecutionResult, SessionInputMode};
pub use subagent::{
    AgentDefinition, AgentDefinitionRegistry, StorageBackedSubagentLookup, builtin_agents,
};
