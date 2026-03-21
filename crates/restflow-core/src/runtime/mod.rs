pub mod agent;
pub mod background_agent;
pub mod channel;
pub mod execution_context;
pub mod orchestrator;
mod output;
pub mod subagent;
pub mod trace;

pub use agent::{
    BashConfig, BashTool, EmailTool, FileConfig, FileTool, HttpTool, ListSubagentsTool,
    SpawnSubagentTool, SpawnTool, SubagentDeps, SubagentManager, SubagentManagerImpl,
    SubagentSpawner, TelegramTool, Tool, ToolRegistry, ToolRegistryBuilder, ToolResult,
    UseSkillTool, WaitSubagentsTool, build_agent_system_prompt, default_registry,
    effective_main_agent_tool_names, main_agent_default_tool_names, registry_from_allowlist,
    secret_resolver_from_storage,
};
pub use background_agent::{
    AgentExecutor, AgentRuntimeExecutor, BackgroundAgentRunner, ExecutionResult,
    NoopHeartbeatEmitter, NoopNotificationSender, NotificationSender, RunnerConfig, RunnerHandle,
    SessionExecutionResult, SessionInputMode, TaskEventEmitter, TaskStreamEvent, TelegramNotifier,
};
pub use channel::{
    BackgroundAgentTrigger, ChatDispatcher, ChatDispatcherConfig, ChatError, ChatSessionManager,
    MessageDebouncer, MessageHandlerConfig, MessageHandlerHandle, MessageRouter, RouteDecision,
    SystemStatus, start_message_handler, start_message_handler_with_chat,
};
pub use execution_context::{ExecutionContext, ExecutionRole};
pub use orchestrator::{AgentOrchestratorImpl, OrchestratingAgentExecutor};
pub use restflow_ai::agent::{
    SpawnHandle, SpawnPriority, SpawnRequest, SubagentCompletion, SubagentConfig, SubagentResult,
    SubagentState, SubagentStatus, SubagentTracker, spawn_subagent,
};
pub use subagent::{
    AgentDefinition, AgentDefinitionRegistry, StorageBackedSubagentLookup, builtin_agents,
};
pub use trace::{
    RestflowTrace, append_message_trace, append_restflow_trace_completed,
    append_restflow_trace_failed, append_restflow_trace_interrupted, append_restflow_trace_started,
    build_restflow_trace_emitter,
};
