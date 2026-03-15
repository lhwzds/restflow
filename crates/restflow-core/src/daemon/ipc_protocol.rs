use crate::auth::{AuthProvider, Credential, CredentialSource, ProfileUpdate};
use crate::daemon::session_events::ChatSessionEvent;
use crate::models::{
    AgentNode, BackgroundAgentControlAction, BackgroundAgentPatch, BackgroundAgentSpec,
    BackgroundMessageSource, ChatMessage, ChatRole, ChatSessionUpdate, ExecutionTraceQuery, Hook,
    ItemQuery, MemoryChunk, MemorySession, Skill, TerminalSession, WorkItemPatch, WorkItemSpec,
};
use crate::runtime::TaskStreamEvent;
use crate::storage::SystemConfig;
pub use restflow_contracts::{IpcDaemonStatus, ToolDefinition, ToolExecutionResult};
use restflow_contracts::{ResponseEnvelope, StreamEnvelope};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Message frame: [4 bytes length LE][JSON payload]
pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;
pub const IPC_PROTOCOL_VERSION: &str = "2";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum IpcRequest {
    Ping,
    GetStatus,
    Shutdown,

    ListAgents,
    GetAgent {
        id: String,
    },
    CreateAgent {
        name: String,
        agent: AgentNode,
    },
    UpdateAgent {
        id: String,
        name: Option<String>,
        agent: Option<AgentNode>,
    },
    DeleteAgent {
        id: String,
    },

    ListSkills,
    GetSkill {
        id: String,
    },
    CreateSkill {
        skill: Skill,
    },
    UpdateSkill {
        id: String,
        skill: Skill,
    },
    GetSkillReference {
        skill_id: String,
        ref_id: String,
    },
    DeleteSkill {
        id: String,
    },
    ListWorkItems {
        query: ItemQuery,
    },
    ListWorkItemFolders,
    GetWorkItem {
        id: String,
    },
    CreateWorkItem {
        spec: WorkItemSpec,
    },
    UpdateWorkItem {
        id: String,
        patch: WorkItemPatch,
    },
    DeleteWorkItem {
        id: String,
    },

    ListBackgroundAgents {
        status: Option<String>,
    },
    ListRunnableBackgroundAgents {
        current_time: Option<i64>,
    },
    GetBackgroundAgent {
        id: String,
    },
    ListHooks,
    CreateHook {
        hook: Hook,
    },
    UpdateHook {
        id: String,
        hook: Hook,
    },
    DeleteHook {
        id: String,
    },
    TestHook {
        id: String,
    },

    ListSecrets,
    GetSecret {
        key: String,
    },
    SetSecret {
        key: String,
        value: String,
        description: Option<String>,
    },
    CreateSecret {
        key: String,
        value: String,
        description: Option<String>,
    },
    UpdateSecret {
        key: String,
        value: String,
        description: Option<String>,
    },
    DeleteSecret {
        key: String,
    },

    GetConfig,
    GetGlobalConfig,
    SetConfig {
        config: SystemConfig,
    },

    SearchMemory {
        query: String,
        agent_id: Option<String>,
        limit: Option<u32>,
    },
    SearchMemoryRanked {
        query: crate::models::memory::MemorySearchQuery,
        min_score: Option<f64>,
        scoring_preset: Option<String>,
    },
    GetMemoryChunk {
        id: String,
    },
    ListMemory {
        agent_id: Option<String>,
        tag: Option<String>,
    },
    ListMemoryBySession {
        session_id: String,
    },
    AddMemory {
        content: String,
        agent_id: Option<String>,
        tags: Vec<String>,
    },
    CreateMemoryChunk {
        chunk: MemoryChunk,
    },
    DeleteMemory {
        id: String,
    },
    ClearMemory {
        agent_id: Option<String>,
    },
    GetMemoryStats {
        agent_id: Option<String>,
    },
    ExportMemory {
        agent_id: Option<String>,
    },
    ExportMemorySession {
        session_id: String,
    },
    ExportMemoryAdvanced {
        agent_id: String,
        session_id: Option<String>,
        preset: Option<String>,
        include_metadata: Option<bool>,
        include_timestamps: Option<bool>,
        include_source: Option<bool>,
        include_tags: Option<bool>,
    },
    GetMemorySession {
        session_id: String,
    },
    ListMemorySessions {
        agent_id: String,
    },
    CreateMemorySession {
        session: MemorySession,
    },
    DeleteMemorySession {
        session_id: String,
        delete_chunks: bool,
    },

    ListSessions,
    ListFullSessions,
    ListSessionsByAgent {
        agent_id: String,
    },
    ListSessionsBySkill {
        skill_id: String,
    },
    CountSessions,
    DeleteSessionsOlderThan {
        older_than_ms: i64,
    },
    GetSession {
        id: String,
    },
    CreateSession {
        agent_id: Option<String>,
        model: Option<String>,
        name: Option<String>,
        skill_id: Option<String>,
    },
    UpdateSession {
        id: String,
        updates: ChatSessionUpdate,
    },
    RenameSession {
        id: String,
        name: String,
    },
    ArchiveSession {
        id: String,
    },
    DeleteSession {
        id: String,
    },
    RebuildExternalSession {
        id: String,
    },
    SearchSessions {
        query: String,
    },
    AddMessage {
        session_id: String,
        role: ChatRole,
        content: String,
    },
    AppendMessage {
        session_id: String,
        message: ChatMessage,
    },
    ExecuteChatSession {
        session_id: String,
        user_input: Option<String>,
    },
    ExecuteChatSessionStream {
        session_id: String,
        user_input: Option<String>,
        stream_id: String,
    },
    SteerChatSessionStream {
        session_id: String,
        instruction: String,
    },
    CancelChatSessionStream {
        stream_id: String,
    },
    GetSessionMessages {
        session_id: String,
        limit: Option<usize>,
    },
    ListToolTraces {
        session_id: String,
        turn_id: Option<String>,
        limit: Option<usize>,
    },
    QueryExecutionTraces {
        #[serde(default)]
        query: ExecutionTraceQuery,
    },
    GetExecutionTraceStats {
        task_id: Option<String>,
    },
    GetExecutionTraceById {
        #[serde(alias = "event_id")]
        id: String,
    },

    ListTerminalSessions,
    GetTerminalSession {
        id: String,
    },
    CreateTerminalSession,
    RenameTerminalSession {
        id: String,
        name: String,
    },
    UpdateTerminalSession {
        id: String,
        name: Option<String>,
        working_directory: Option<String>,
        startup_command: Option<String>,
    },
    SaveTerminalSession {
        session: TerminalSession,
    },
    DeleteTerminalSession {
        id: String,
    },
    MarkAllTerminalSessionsStopped,

    ListAuthProfiles,
    GetAuthProfile {
        id: String,
    },
    AddAuthProfile {
        name: String,
        credential: Credential,
        source: CredentialSource,
        provider: AuthProvider,
    },
    RemoveAuthProfile {
        id: String,
    },
    UpdateAuthProfile {
        id: String,
        updates: ProfileUpdate,
    },
    DiscoverAuth,
    EnableAuthProfile {
        id: String,
    },
    DisableAuthProfile {
        id: String,
        reason: String,
    },
    GetApiKey {
        provider: AuthProvider,
    },
    GetApiKeyForProfile {
        id: String,
    },
    TestAuthProfile {
        id: String,
    },
    MarkAuthSuccess {
        id: String,
    },
    MarkAuthFailure {
        id: String,
    },
    ClearAuthProfiles,

    GetBackgroundAgentHistory {
        id: String,
    },
    CreateBackgroundAgent {
        spec: BackgroundAgentSpec,
    },
    UpdateBackgroundAgent {
        id: String,
        patch: BackgroundAgentPatch,
    },
    DeleteBackgroundAgent {
        id: String,
    },
    ControlBackgroundAgent {
        id: String,
        action: BackgroundAgentControlAction,
    },
    GetBackgroundAgentProgress {
        id: String,
        event_limit: Option<usize>,
    },
    SendBackgroundAgentMessage {
        id: String,
        message: String,
        source: Option<BackgroundMessageSource>,
    },
    HandleBackgroundAgentApproval {
        id: String,
        approved: bool,
    },
    ListBackgroundAgentMessages {
        id: String,
        limit: Option<usize>,
    },
    SubscribeBackgroundAgentEvents {
        background_agent_id: String,
    },
    SubscribeSessionEvents,

    GetSystemInfo,
    GetAvailableModels,
    GetAvailableTools,
    GetAvailableToolDefinitions,
    ExecuteTool {
        name: String,
        input: Value,
    },
    ListMcpServers,

    BuildAgentSystemPrompt {
        agent_node: AgentNode,
    },
}

pub type IpcResponse = ResponseEnvelope<serde_json::Value>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcStreamEvent {
    BackgroundAgent(TaskStreamEvent),
    Session(ChatSessionEvent),
}

pub type StreamFrame = StreamEnvelope<IpcStreamEvent>;

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_traits::ToolErrorCategory;

    #[test]
    fn test_ping_serialization() {
        let request = IpcRequest::Ping;
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("Ping"));

        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, IpcRequest::Ping));
    }

    #[test]
    fn test_search_memory_serialization() {
        let request = IpcRequest::SearchMemory {
            query: "test query".to_string(),
            agent_id: Some("agent-1".to_string()),
            limit: Some(10),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::SearchMemory {
            query,
            agent_id,
            limit,
        } = parsed
        {
            assert_eq!(query, "test query");
            assert_eq!(agent_id, Some("agent-1".to_string()));
            assert_eq!(limit, Some(10));
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_list_memory_serialization() {
        let request = IpcRequest::ListMemory {
            agent_id: Some("agent-1".to_string()),
            tag: Some("rust".to_string()),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::ListMemory { agent_id, tag } = parsed {
            assert_eq!(agent_id, Some("agent-1".to_string()));
            assert_eq!(tag, Some("rust".to_string()));
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_add_memory_serialization() {
        let request = IpcRequest::AddMemory {
            content: "test content".to_string(),
            agent_id: None,
            tags: vec!["tag1".to_string(), "tag2".to_string()],
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::AddMemory {
            content,
            agent_id,
            tags,
        } = parsed
        {
            assert_eq!(content, "test content");
            assert_eq!(agent_id, None);
            assert_eq!(tags, vec!["tag1", "tag2"]);
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_create_session_serialization() {
        let request = IpcRequest::CreateSession {
            agent_id: Some("agent-1".to_string()),
            model: Some("claude-sonnet-4".to_string()),
            name: Some("My Chat".to_string()),
            skill_id: Some("skill-1".to_string()),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::CreateSession {
            agent_id,
            model,
            name,
            skill_id,
        } = parsed
        {
            assert_eq!(agent_id, Some("agent-1".to_string()));
            assert_eq!(model, Some("claude-sonnet-4".to_string()));
            assert_eq!(name, Some("My Chat".to_string()));
            assert_eq!(skill_id, Some("skill-1".to_string()));
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_add_message_serialization() {
        let request = IpcRequest::AddMessage {
            session_id: "session-1".to_string(),
            role: crate::models::ChatRole::User,
            content: "Hello".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::AddMessage {
            session_id,
            role,
            content,
        } = parsed
        {
            assert_eq!(session_id, "session-1");
            assert!(matches!(role, crate::models::ChatRole::User));
            assert_eq!(content, "Hello");
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_append_message_serialization() {
        let message = crate::models::ChatMessage::user("Hello");
        let request = IpcRequest::AppendMessage {
            session_id: "session-2".to_string(),
            message,
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::AppendMessage {
            session_id,
            message,
        } = parsed
        {
            assert_eq!(session_id, "session-2");
            assert!(matches!(message.role, crate::models::ChatRole::User));
            assert_eq!(message.content, "Hello");
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_execute_chat_session_serialization() {
        let request = IpcRequest::ExecuteChatSession {
            session_id: "session-3".to_string(),
            user_input: Some("Please summarize the previous answer".to_string()),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::ExecuteChatSession {
            session_id,
            user_input,
        } = parsed
        {
            assert_eq!(session_id, "session-3");
            assert_eq!(
                user_input,
                Some("Please summarize the previous answer".to_string())
            );
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_execute_chat_session_stream_serialization() {
        let request = IpcRequest::ExecuteChatSessionStream {
            session_id: "session-4".to_string(),
            user_input: Some("stream this response".to_string()),
            stream_id: "stream-123".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::ExecuteChatSessionStream {
            session_id,
            user_input,
            stream_id,
        } = parsed
        {
            assert_eq!(session_id, "session-4");
            assert_eq!(user_input, Some("stream this response".to_string()));
            assert_eq!(stream_id, "stream-123");
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_cancel_chat_session_stream_serialization() {
        let request = IpcRequest::CancelChatSessionStream {
            stream_id: "stream-456".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::CancelChatSessionStream { stream_id } = parsed {
            assert_eq!(stream_id, "stream-456");
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_steer_chat_session_stream_serialization() {
        let request = IpcRequest::SteerChatSessionStream {
            session_id: "session-9".to_string(),
            instruction: "Focus on root cause".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::SteerChatSessionStream {
            session_id,
            instruction,
        } = parsed
        {
            assert_eq!(session_id, "session-9");
            assert_eq!(instruction, "Focus on root cause");
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_get_background_agent_serialization() {
        let request = IpcRequest::GetBackgroundAgent {
            id: "agent-1".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::GetBackgroundAgent { id } = parsed {
            assert_eq!(id, "agent-1");
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_update_terminal_session_serialization() {
        let request = IpcRequest::UpdateTerminalSession {
            id: "terminal-1".to_string(),
            name: Some("Terminal A".to_string()),
            working_directory: Some("/tmp".to_string()),
            startup_command: Some("ls".to_string()),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::UpdateTerminalSession {
            id,
            name,
            working_directory,
            startup_command,
        } = parsed
        {
            assert_eq!(id, "terminal-1");
            assert_eq!(name, Some("Terminal A".to_string()));
            assert_eq!(working_directory, Some("/tmp".to_string()));
            assert_eq!(startup_command, Some("ls".to_string()));
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_save_terminal_session_serialization() {
        let session =
            crate::models::TerminalSession::new("terminal-9".to_string(), "Terminal 9".to_string());
        let request = IpcRequest::SaveTerminalSession { session };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::SaveTerminalSession { session } = parsed {
            assert_eq!(session.id, "terminal-9");
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_build_agent_system_prompt_serialization() {
        let request = IpcRequest::BuildAgentSystemPrompt {
            agent_node: crate::models::AgentNode::new(),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::BuildAgentSystemPrompt { agent_node: _ } = parsed {
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_create_memory_session_serialization() {
        let session = crate::models::memory::MemorySession::new(
            "agent-1".to_string(),
            "Session A".to_string(),
        );
        let request = IpcRequest::CreateMemorySession { session };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::CreateMemorySession { session } = parsed {
            assert_eq!(session.name, "Session A");
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_list_background_agents_serialization() {
        let request = IpcRequest::ListBackgroundAgents {
            status: Some("active".to_string()),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::ListBackgroundAgents { status } = parsed {
            assert_eq!(status, Some("active".to_string()));
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_subscribe_background_agent_events_serialization() {
        let request = IpcRequest::SubscribeBackgroundAgentEvents {
            background_agent_id: "agent-42".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::SubscribeBackgroundAgentEvents {
            background_agent_id,
        } = parsed
        {
            assert_eq!(background_agent_id, "agent-42");
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_background_agent_stream_frame_serialization() {
        let event = TaskStreamEvent::progress(
            "agent-42",
            "notification",
            Some(100),
            Some("done".to_string()),
        );
        let frame = StreamFrame::Event {
            event: IpcStreamEvent::BackgroundAgent(event.clone()),
        };
        let json = serde_json::to_string(&frame).unwrap();
        let parsed: StreamFrame = serde_json::from_str(&json).unwrap();

        match parsed {
            StreamFrame::Event {
                event: IpcStreamEvent::BackgroundAgent(parsed_event),
            } => {
                assert_eq!(parsed_event.task_id, event.task_id);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_response_success() {
        let response = IpcResponse::success(serde_json::json!({ "id": "test-123" }));
        let json = serde_json::to_string(&response).unwrap();
        let parsed: IpcResponse = serde_json::from_str(&json).unwrap();

        assert!(json.contains("response_type"));
        if let IpcResponse::Success(value) = parsed {
            assert_eq!(value["id"], "test-123");
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_response_error() {
        let response = IpcResponse::error(404, "Not found");
        let json = serde_json::to_string(&response).unwrap();
        let parsed: IpcResponse = serde_json::from_str(&json).unwrap();

        if let IpcResponse::Error(error) = parsed {
            assert_eq!(error.code, 404);
            assert_eq!(error.message, "Not found");
            assert_eq!(error.details, None);
            assert_eq!(error.kind, restflow_contracts::ErrorKind::NotFound);
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_response_error_roundtrip_with_structured_details() {
        let details = serde_json::json!({
            "error_category": "Execution",
            "retryable": false,
            "retry_after_ms": 1200,
            "metadata": { "exit_code": 7 }
        });
        let response = IpcResponse::error_with_details(500, "Tool execution failed", Some(details));
        let json = serde_json::to_string(&response).unwrap();
        let parsed: IpcResponse = serde_json::from_str(&json).unwrap();

        if let IpcResponse::Error(error) = parsed {
            assert_eq!(error.code, 500);
            assert_eq!(error.message, "Tool execution failed");
            assert_eq!(error.details.unwrap()["metadata"]["exit_code"], 7);
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_response_error_v2_shape_without_details() {
        let encoded = serde_json::json!({
            "response_type": "Error",
            "data": {
                "code": 409,
                "kind": "conflict",
                "message": "conflict"
            }
        });

        let parsed: IpcResponse = serde_json::from_value(encoded).unwrap();
        if let IpcResponse::Error(error) = parsed {
            assert_eq!(error.code, 409);
            assert_eq!(error.message, "conflict");
            assert_eq!(error.details, None);
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_protocol_version_is_v2() {
        assert_eq!(IPC_PROTOCOL_VERSION, "2");
    }

    #[test]
    fn test_daemon_status_roundtrip() {
        let status = IpcDaemonStatus {
            status: "running".to_string(),
            protocol_version: IPC_PROTOCOL_VERSION.to_string(),
            daemon_version: "0.3.5".to_string(),
            pid: 1234,
            started_at_ms: 1_700_000_000_000,
            uptime_secs: 42,
        };

        let value = serde_json::to_value(&status).unwrap();
        let parsed: IpcDaemonStatus = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, status);
    }

    #[test]
    fn test_stream_frame_start() {
        let frame = StreamFrame::Start {
            stream_id: "stream-1".to_string(),
        };
        let json = serde_json::to_string(&frame).unwrap();
        let parsed: StreamFrame = serde_json::from_str(&json).unwrap();

        assert!(json.contains("stream_type"));
        if let StreamFrame::Start { stream_id } = parsed {
            assert_eq!(stream_id, "stream-1");
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_stream_frame_ack() {
        let frame = StreamFrame::Ack {
            content: "Acknowledged".to_string(),
        };
        let json = serde_json::to_string(&frame).unwrap();
        let parsed: StreamFrame = serde_json::from_str(&json).unwrap();

        if let StreamFrame::Ack { content } = parsed {
            assert_eq!(content, "Acknowledged");
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_stream_frame_data() {
        let frame = StreamFrame::Data {
            content: "Hello, world!".to_string(),
        };
        let json = serde_json::to_string(&frame).unwrap();
        let parsed: StreamFrame = serde_json::from_str(&json).unwrap();

        if let StreamFrame::Data { content } = parsed {
            assert_eq!(content, "Hello, world!");
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_stream_frame_tool_call() {
        let frame = StreamFrame::ToolCall {
            id: "call-1".to_string(),
            name: "search".to_string(),
            arguments: serde_json::json!({ "query": "test" }),
        };
        let json = serde_json::to_string(&frame).unwrap();
        let parsed: StreamFrame = serde_json::from_str(&json).unwrap();

        if let StreamFrame::ToolCall {
            id,
            name,
            arguments,
        } = parsed
        {
            assert_eq!(id, "call-1");
            assert_eq!(name, "search");
            assert_eq!(arguments["query"], "test");
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_stream_frame_tool_result() {
        let frame = StreamFrame::ToolResult {
            id: "call-1".to_string(),
            result: "{\"ok\":true}".to_string(),
            success: true,
        };
        let json = serde_json::to_string(&frame).unwrap();
        let parsed: StreamFrame = serde_json::from_str(&json).unwrap();

        if let StreamFrame::ToolResult {
            id,
            result,
            success,
        } = parsed
        {
            assert_eq!(id, "call-1");
            assert_eq!(result, "{\"ok\":true}");
            assert!(success);
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_stream_frame_done() {
        let frame = StreamFrame::Done {
            total_tokens: Some(100),
        };
        let json = serde_json::to_string(&frame).unwrap();
        let parsed: StreamFrame = serde_json::from_str(&json).unwrap();

        if let StreamFrame::Done { total_tokens } = parsed {
            assert_eq!(total_tokens, Some(100));
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_stream_frame_error() {
        let frame = StreamFrame::error(500, "Internal error");
        let json = serde_json::to_string(&frame).unwrap();
        let parsed: StreamFrame = serde_json::from_str(&json).unwrap();

        if let StreamFrame::Error(error) = parsed {
            assert_eq!(error.code, 500);
            assert_eq!(error.message, "Internal error");
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_get_api_key_serialization() {
        let request = IpcRequest::GetApiKey {
            provider: AuthProvider::Anthropic,
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::GetApiKey { provider } = parsed {
            assert!(matches!(provider, AuthProvider::Anthropic));
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_get_api_key_for_profile_serialization() {
        let request = IpcRequest::GetApiKeyForProfile {
            id: "profile-1".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::GetApiKeyForProfile { id } = parsed {
            assert_eq!(id, "profile-1");
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_get_system_info_serialization() {
        let request = IpcRequest::GetSystemInfo;
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        assert!(matches!(parsed, IpcRequest::GetSystemInfo));
    }

    #[test]
    fn test_get_available_tool_definitions_serialization() {
        let request = IpcRequest::GetAvailableToolDefinitions;
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        assert!(matches!(parsed, IpcRequest::GetAvailableToolDefinitions));
    }

    #[test]
    fn test_execute_tool_serialization() {
        let request = IpcRequest::ExecuteTool {
            name: "manage_background_agents".to_string(),
            input: serde_json::json!({ "operation": "list" }),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        match parsed {
            IpcRequest::ExecuteTool { name, input } => {
                assert_eq!(name, "manage_background_agents");
                assert_eq!(input["operation"], "list");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_tool_execution_result_serialization_preserves_error_metadata() {
        let result = ToolExecutionResult {
            success: false,
            result: serde_json::json!({
                "stderr": "permission denied"
            }),
            error: Some("execution failed".to_string()),
            error_category: Some(ToolErrorCategory::Execution),
            retryable: Some(false),
            retry_after_ms: Some(2500),
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: ToolExecutionResult = serde_json::from_str(&json).unwrap();

        assert!(!parsed.success);
        assert_eq!(parsed.error.as_deref(), Some("execution failed"));
        assert_eq!(parsed.error_category, Some(ToolErrorCategory::Execution));
        assert_eq!(parsed.retryable, Some(false));
        assert_eq!(parsed.retry_after_ms, Some(2500));
        assert_eq!(parsed.result["stderr"], "permission denied");
    }

    #[test]
    fn test_tool_execution_result_roundtrip_preserves_retry_fields() {
        let result = ToolExecutionResult {
            success: false,
            result: serde_json::json!({
                "status": 429
            }),
            error: Some("rate limited".to_string()),
            error_category: Some(ToolErrorCategory::RateLimit),
            retryable: Some(true),
            retry_after_ms: Some(1200),
        };

        let encoded = serde_json::to_value(&result).unwrap();
        let decoded: ToolExecutionResult = serde_json::from_value(encoded.clone()).unwrap();

        assert!(!decoded.success);
        assert_eq!(decoded.error.as_deref(), Some("rate limited"));
        assert_eq!(decoded.error_category, Some(ToolErrorCategory::RateLimit));
        assert_eq!(decoded.retryable, Some(true));
        assert_eq!(decoded.retry_after_ms, Some(1200));
        assert_eq!(encoded["error_category"], "RateLimit");
        assert_eq!(encoded["retryable"], true);
        assert_eq!(encoded["retry_after_ms"], 1200);
    }

    #[test]
    fn test_query_execution_traces_serialization() {
        let request = IpcRequest::QueryExecutionTraces {
            query: ExecutionTraceQuery {
                task_id: Some("task-1".to_string()),
                limit: Some(25),
                ..Default::default()
            },
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::QueryExecutionTraces { query } = parsed {
            assert_eq!(query.task_id.as_deref(), Some("task-1"));
            assert_eq!(query.limit, Some(25));
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_query_execution_traces_backward_compat_with_missing_query() {
        let legacy = serde_json::json!({
            "type": "QueryExecutionTraces",
            "data": {}
        });

        let parsed: IpcRequest = serde_json::from_value(legacy).unwrap();
        if let IpcRequest::QueryExecutionTraces { query } = parsed {
            assert_eq!(query.task_id, None);
            assert_eq!(query.agent_id, None);
            assert_eq!(query.category, None);
            assert_eq!(query.source, None);
            assert_eq!(query.from_timestamp, None);
            assert_eq!(query.to_timestamp, None);
            assert_eq!(query.limit, None);
            assert_eq!(query.offset, None);
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_get_execution_trace_stats_serialization() {
        let request = IpcRequest::GetExecutionTraceStats {
            task_id: Some("task-42".to_string()),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::GetExecutionTraceStats { task_id } = parsed {
            assert_eq!(task_id.as_deref(), Some("task-42"));
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_get_execution_trace_by_id_serialization() {
        let request = IpcRequest::GetExecutionTraceById {
            id: "trace-1".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::GetExecutionTraceById { id } = parsed {
            assert_eq!(id, "trace-1");
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_get_execution_trace_by_id_backward_compat_event_id_alias() {
        let legacy = serde_json::json!({
            "type": "GetExecutionTraceById",
            "data": {
                "event_id": "trace-legacy"
            }
        });

        let parsed: IpcRequest = serde_json::from_value(legacy).unwrap();
        if let IpcRequest::GetExecutionTraceById { id } = parsed {
            assert_eq!(id, "trace-legacy");
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_list_runnable_background_agents_serialization() {
        let request = IpcRequest::ListRunnableBackgroundAgents {
            current_time: Some(1_700_000_000_000),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::ListRunnableBackgroundAgents { current_time } = parsed {
            assert_eq!(current_time, Some(1_700_000_000_000));
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_create_hook_serialization() {
        let hook = crate::models::Hook::new(
            "Test Hook".to_string(),
            crate::models::HookEvent::TaskCompleted,
            crate::models::HookAction::Webhook {
                url: "https://example.com/hook".to_string(),
                method: None,
                headers: None,
            },
        );

        let request = IpcRequest::CreateHook { hook: hook.clone() };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        match parsed {
            IpcRequest::CreateHook { hook: decoded } => {
                assert_eq!(decoded.name, hook.name);
                assert_eq!(decoded.event, hook.event);
                assert_eq!(decoded.action, hook.action);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_handle_background_agent_approval_serialization() {
        let request = IpcRequest::HandleBackgroundAgentApproval {
            id: "task-1".to_string(),
            approved: true,
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::HandleBackgroundAgentApproval { id, approved } = parsed {
            assert_eq!(id, "task-1");
            assert!(approved);
        } else {
            panic!("Wrong variant");
        }
    }
}
