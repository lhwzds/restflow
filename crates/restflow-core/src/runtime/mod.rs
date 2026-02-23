pub mod agent;
pub mod background_agent;
pub mod channel;
mod output;
pub mod subagent;

pub use agent::{
    BashConfig, BashTool, EmailTool, FileConfig, FileTool, HttpTool, ListAgentsTool,
    SpawnAgentTool, SpawnTool, SubagentDeps, SubagentManager, SubagentManagerImpl, SubagentSpawner,
    TelegramTool, Tool, ToolRegistry, ToolRegistryBuilder, ToolResult, UseSkillTool,
    WaitAgentsTool, build_agent_system_prompt, default_registry,
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
pub use subagent::{
    AgentDefinition, AgentDefinitionRegistry, SpawnHandle, SpawnPriority, SpawnRequest,
    SubagentCompletion, SubagentConfig, SubagentResult, SubagentState, SubagentStatus,
    builtin_agents, spawn_subagent,
};
// SubagentTracker from restflow-ai (runtime implementation with DashMap, abort handles, etc.)
pub use restflow_ai::agent::SubagentTracker;
