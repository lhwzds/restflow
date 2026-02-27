use crate::auth::{AuthProvider, Credential, CredentialSource, ProfileUpdate};
use crate::daemon::session_events::ChatSessionEvent;
use crate::models::{
    AgentNode, BackgroundAgentControlAction, BackgroundAgentPatch, BackgroundAgentSpec,
    BackgroundMessageSource, ChatMessage, ChatRole, ChatSessionUpdate, Hook, ItemQuery,
    MemoryChunk, MemorySession, Skill, TerminalSession, WorkItemPatch, WorkItemSpec,
};
use crate::runtime::TaskStreamEvent;
use crate::storage::SystemConfig;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Message frame: [4 bytes length LE][JSON payload]
pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionResult {
    pub success: bool,
    pub result: Value,
    pub error: Option<String>,
}

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
    DeleteSession {
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
    ListChatExecutionEvents {
        session_id: String,
        turn_id: Option<String>,
        limit: Option<usize>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum IpcResponse {
    Pong,
    Success(serde_json::Value),
    Error { code: i32, message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum StreamFrame {
    Start {
        stream_id: String,
    },
    Data {
        content: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    ToolResult {
        id: String,
        result: String,
        success: bool,
    },
    BackgroundAgentEvent {
        event: TaskStreamEvent,
    },
    SessionEvent {
        event: ChatSessionEvent,
    },
    Done {
        total_tokens: Option<u32>,
    },
    Error {
        code: i32,
        message: String,
    },
}

impl IpcResponse {
    pub fn success<T: Serialize>(data: T) -> Self {
        match serde_json::to_value(data) {
            Ok(value) => Self::Success(value),
            Err(_) => Self::Success(serde_json::Value::Null),
        }
    }

    pub fn error(code: i32, message: impl Into<String>) -> Self {
        Self::Error {
            code,
            message: message.into(),
        }
    }

    pub fn not_found(what: &str) -> Self {
        Self::Error {
            code: 404,
            message: format!("{} not found", what),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let frame = StreamFrame::BackgroundAgentEvent {
            event: event.clone(),
        };
        let json = serde_json::to_string(&frame).unwrap();
        let parsed: StreamFrame = serde_json::from_str(&json).unwrap();

        match parsed {
            StreamFrame::BackgroundAgentEvent {
                event: parsed_event,
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

        if let IpcResponse::Error { code, message } = parsed {
            assert_eq!(code, 404);
            assert_eq!(message, "Not found");
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_stream_frame_start() {
        let frame = StreamFrame::Start {
            stream_id: "stream-1".to_string(),
        };
        let json = serde_json::to_string(&frame).unwrap();
        let parsed: StreamFrame = serde_json::from_str(&json).unwrap();

        if let StreamFrame::Start { stream_id } = parsed {
            assert_eq!(stream_id, "stream-1");
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
        let frame = StreamFrame::Error {
            code: 500,
            message: "Internal error".to_string(),
        };
        let json = serde_json::to_string(&frame).unwrap();
        let parsed: StreamFrame = serde_json::from_str(&json).unwrap();

        if let StreamFrame::Error { code, message } = parsed {
            assert_eq!(code, 500);
            assert_eq!(message, "Internal error");
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
