pub mod agent;
pub mod execution_context;
pub mod orchestrator;
pub mod session_runner;
pub(crate) mod session_turn;
pub mod subagent;

// Public surface rule:
// - `runtime::runtime` re-exports durable runtime and core-owned adapters.
// - AI-owned subagent runtime state stays exported from `ai` /
//   `types` so ownership remains unambiguous.
pub use agent::{
    BashConfig, BashTool, FileConfig, FileTool, ListSubagentsTool, LoadSkillTool,
    SpawnSubagentTool, Tool, ToolRegistry, ToolRegistryBuilder, ToolResult, WaitSubagentsTool,
    build_agent_system_prompt, default_registry, effective_main_agent_tool_names,
    main_agent_default_tool_names, registry_from_allowlist, secret_resolver_from_storage,
};
pub use execution_context::{ExecutionContext, ExecutionRole};
pub use orchestrator::AgentOrchestratorImpl;
pub use session_runner::{AgentRuntimeExecutor, SessionExecutionResult, SessionInputMode};
pub use subagent::{
    AgentDefinition, AgentDefinitionRegistry, StorageBackedSubagentLookup, builtin_agents,
};
