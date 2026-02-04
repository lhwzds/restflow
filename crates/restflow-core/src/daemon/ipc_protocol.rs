use crate::auth::{AuthProvider, Credential, CredentialSource, ProfileUpdate};
use crate::models::{AgentNode, ChatRole, Skill, TaskSchedule};
use crate::storage::SystemConfig;
use serde::{Deserialize, Serialize};

/// Message frame: [4 bytes length LE][JSON payload]
pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

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
    DeleteSkill {
        id: String,
    },

    ListTasks,
    GetTask {
        id: String,
    },
    CreateTask {
        name: String,
        agent_id: String,
        schedule: TaskSchedule,
    },
    RunTask {
        id: String,
    },
    StopTask {
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
    ListMemory {
        agent_id: Option<String>,
        tag: Option<String>,
    },
    AddMemory {
        content: String,
        agent_id: Option<String>,
        tags: Vec<String>,
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

    ListSessions,
    GetSession {
        id: String,
    },
    CreateSession {
        agent_id: Option<String>,
        model: Option<String>,
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
    GetSessionMessages {
        session_id: String,
        limit: Option<usize>,
    },

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
    GetApiKey {
        provider: AuthProvider,
    },
    TestAuthProfile {
        id: String,
    },

    PauseTask {
        id: String,
    },
    ResumeTask {
        id: String,
    },
    ListTasksByStatus {
        status: String,
    },
    GetTaskHistory {
        id: String,
    },
    SubscribeTaskEvents {
        task_id: String,
    },

    ExecuteAgent {
        id: String,
        input: String,
        session_id: Option<String>,
    },
    ExecuteAgentStream {
        id: String,
        input: String,
        session_id: Option<String>,
    },
    CancelExecution {
        execution_id: String,
    },

    GetSystemInfo,
    GetAvailableModels,
    GetAvailableTools,
    ListMcpServers,
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
    Start { stream_id: String },
    Data { content: String },
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    ToolResult { id: String, result: String },
    Done { total_tokens: Option<u32> },
    Error { code: i32, message: String },
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
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::CreateSession { agent_id, model } = parsed {
            assert_eq!(agent_id, Some("agent-1".to_string()));
            assert_eq!(model, Some("claude-sonnet-4".to_string()));
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
    fn test_pause_task_serialization() {
        let request = IpcRequest::PauseTask {
            id: "task-1".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::PauseTask { id } = parsed {
            assert_eq!(id, "task-1");
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_list_tasks_by_status_serialization() {
        let request = IpcRequest::ListTasksByStatus {
            status: "active".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::ListTasksByStatus { status } = parsed {
            assert_eq!(status, "active");
        } else {
            panic!("Wrong variant");
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
    fn test_execute_agent_serialization() {
        let request = IpcRequest::ExecuteAgent {
            id: "agent-1".to_string(),
            input: "Hello".to_string(),
            session_id: Some("session-1".to_string()),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        if let IpcRequest::ExecuteAgent {
            id,
            input,
            session_id,
        } = parsed
        {
            assert_eq!(id, "agent-1");
            assert_eq!(input, "Hello");
            assert_eq!(session_id, Some("session-1".to_string()));
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
}
