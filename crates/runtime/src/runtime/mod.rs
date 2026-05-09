pub mod agent;
pub mod execution_context;
pub mod orchestrator;
pub(crate) mod session_turn;
pub mod subagent;
pub mod task_runtime;

// Public surface rule:
// - `runtime::runtime` re-exports durable runtime and core-owned adapters.
// - AI-owned subagent runtime state stays exported from `ai` /
//   `types` so ownership remains unambiguous.
pub use agent::{
    BashConfig, BashTool, FileConfig, FileTool, ListSubagentsTool, LoadSkillTool,
    SpawnSubagentTool, SpawnTool, Tool, ToolRegistry, ToolRegistryBuilder, ToolResult,
    WaitSubagentsTool, build_agent_system_prompt, default_registry,
    effective_main_agent_tool_names, main_agent_default_tool_names, registry_from_allowlist,
    secret_resolver_from_storage,
};
pub use execution_context::{ExecutionContext, ExecutionRole};
pub use orchestrator::{AgentOrchestratorImpl, OrchestratingAgentExecutor};
pub use subagent::{
    AgentDefinition, AgentDefinitionRegistry, StorageBackedSubagentLookup, builtin_agents,
};
pub use task_runtime::{
    AgentExecutor, AgentRuntimeExecutor, ExecutionResult, NoopHeartbeatEmitter,
    SessionExecutionResult, SessionInputMode, TaskEventEmitter, TaskRunner, TaskRunnerConfig,
    TaskRunnerHandle, TaskStreamEvent,
};
