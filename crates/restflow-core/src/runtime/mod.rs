pub mod agent;
pub mod background_agent;
pub mod channel;
pub mod execution_context;
pub mod orchestrator;
mod output;
pub mod subagent;

// Public surface rule:
// - `restflow-core::runtime` re-exports durable runtime and core-owned adapters.
// - AI-owned subagent runtime state stays exported from `restflow-ai` /
//   `restflow-traits` so ownership remains unambiguous.
pub use agent::{
    BashConfig, BashTool, EmailTool, FileConfig, FileTool, HttpTool, ListSubagentsTool,
    SpawnSubagentTool, SpawnTool, TelegramTool, Tool, ToolRegistry, ToolRegistryBuilder,
    ToolResult, UseSkillTool, WaitSubagentsTool, build_agent_system_prompt, default_registry,
    effective_main_agent_tool_names, main_agent_default_tool_names, registry_from_allowlist,
    secret_resolver_from_storage,
};
pub use background_agent::{
    AgentExecutor, AgentRuntimeExecutor, ExecutionResult, NoopHeartbeatEmitter,
    NoopNotificationSender, NotificationSender, SessionExecutionResult, SessionInputMode,
    TaskEventEmitter, TaskRunner, TaskRunnerConfig, TaskRunnerHandle, TaskStreamEvent,
    TelegramNotifier,
};
pub use channel::{
    ChatDispatcher, ChatDispatcherConfig, ChatError, ChatSessionManager, MessageDebouncer,
    MessageHandlerConfig, MessageHandlerHandle, MessageRouter, RouteDecision, SystemStatus,
    TaskTrigger, start_message_handler, start_message_handler_with_chat,
};
pub use execution_context::{ExecutionContext, ExecutionRole};
pub use orchestrator::{AgentOrchestratorImpl, OrchestratingAgentExecutor};
pub use restflow_telemetry::RestflowTrace;
pub use subagent::{
    AgentDefinition, AgentDefinitionRegistry, StorageBackedSubagentLookup, builtin_agents,
};
