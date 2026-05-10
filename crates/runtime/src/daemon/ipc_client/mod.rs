use super::ipc_protocol::{
    IpcDaemonStatus, IpcRequest, IpcResponse, IpcStreamEvent, MAX_MESSAGE_SIZE, StreamFrame,
    ToolDefinition, ToolExecutionResult,
};
use crate::RunTimeline;
use crate::StoredAgent;
use crate::daemon::request_mapper::to_contract;
use crate::daemon::session_events::ChatSessionEvent;
use anyhow::{Context, Result, bail};
use serde::de::DeserializeOwned;
use std::path::Path;
use types::{
    AgentNode, ArchiveResponse, CancelResponse, ChatMessage, ChatRole, ChatSession,
    ChatSessionSummary, ChatSessionUpdate, DeleteResponse, ErrorPayload, ExecutionScope,
    PromptResponse, RunListQuery, RunSummary, Skill, SteerResponse,
};

#[cfg(unix)]
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(unix)]
use tokio::net::UnixStream;

#[cfg(unix)]
pub struct IpcClient {
    stream: UnixStream,
}

#[cfg(unix)]
fn read_stream_frame_or_ipc_error(
    buf: &[u8],
    deserialize_context: &str,
    unexpected_success: &str,
    unexpected_pong: &str,
) -> Result<StreamFrame> {
    if let Ok(frame) = serde_json::from_slice::<StreamFrame>(buf) {
        return Ok(frame);
    }

    let response: IpcResponse =
        serde_json::from_slice(buf).with_context(|| deserialize_context.to_string())?;
    match response {
        IpcResponse::Error(error) => bail!("{}", IpcClient::format_ipc_error(&error)),
        IpcResponse::Success(_) => bail!("{}", unexpected_success),
        IpcResponse::Pong => bail!("{}", unexpected_pong),
    }
}

#[cfg(unix)]
impl IpcClient {
    pub async fn connect(socket_path: &Path) -> Result<Self> {
        let stream = UnixStream::connect(socket_path)
            .await
            .context("Failed to connect to daemon. Is it running?")?;
        Ok(Self { stream })
    }

    async fn send_request_frame(&mut self, req: &IpcRequest) -> Result<()> {
        let json = serde_json::to_vec(&req)?;
        self.stream
            .write_all(&(json.len() as u32).to_le_bytes())
            .await?;
        self.stream.write_all(&json).await?;
        Ok(())
    }

    async fn read_raw_frame(&mut self) -> Result<Vec<u8>> {
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let len = u32::from_le_bytes(len_buf) as usize;
        if len > MAX_MESSAGE_SIZE {
            anyhow::bail!("Response too large");
        }

        let mut buf = vec![0u8; len];
        self.stream.read_exact(&mut buf).await?;
        Ok(buf)
    }

    pub async fn request(&mut self, req: IpcRequest) -> Result<IpcResponse> {
        self.send_request_frame(&req).await?;
        let buf = self.read_raw_frame().await?;
        Ok(serde_json::from_slice(&buf)?)
    }

    pub async fn ping(&mut self) -> bool {
        matches!(self.request(IpcRequest::Ping).await, Ok(IpcResponse::Pong))
    }

    pub async fn get_status(&mut self) -> Result<IpcDaemonStatus> {
        self.request_typed(IpcRequest::GetStatus).await
    }

    pub async fn request_typed<T: DeserializeOwned>(&mut self, req: IpcRequest) -> Result<T> {
        match self.request(req).await? {
            IpcResponse::Success(value) => {
                serde_json::from_value(value).context("Failed to deserialize response")
            }
            IpcResponse::Pong => bail!("Unexpected Pong response"),
            IpcResponse::Error(error) => bail!(Self::format_ipc_error(&error)),
        }
    }

    pub async fn request_optional<T: DeserializeOwned>(
        &mut self,
        req: IpcRequest,
    ) -> Result<Option<T>> {
        match self.request(req).await? {
            IpcResponse::Success(value) => Ok(Some(serde_json::from_value(value)?)),
            IpcResponse::Error(error) if error.code == 404 => Ok(None),
            IpcResponse::Error(error) => bail!(Self::format_ipc_error(&error)),
            IpcResponse::Pong => bail!("Unexpected Pong response"),
        }
    }

    pub fn format_ipc_error(error: &ErrorPayload) -> String {
        match &error.details {
            Some(details) => serde_json::json!({
                "code": error.code,
                "kind": error.kind,
                "message": error.message,
                "details": details
            })
            .to_string(),
            None => format!("IPC error {}: {}", error.code, error.message),
        }
    }

    pub async fn list_skills(&mut self) -> Result<Vec<Skill>> {
        self.request_typed(IpcRequest::ListSkills).await
    }

    pub async fn get_skill(&mut self, id: String) -> Result<Option<Skill>> {
        self.request_optional(IpcRequest::GetSkill { id }).await
    }

    pub async fn get_skill_reference(
        &mut self,
        skill_id: String,
        ref_id: String,
    ) -> Result<Option<String>> {
        self.request_optional(IpcRequest::GetSkillReference { skill_id, ref_id })
            .await
    }

    pub async fn list_agents(&mut self) -> Result<Vec<StoredAgent>> {
        self.request_typed(IpcRequest::ListAgents).await
    }

    pub async fn get_agent(&mut self, id: String) -> Result<StoredAgent> {
        self.request_typed(IpcRequest::GetAgent { id }).await
    }

    pub async fn list_sessions(&mut self) -> Result<Vec<ChatSessionSummary>> {
        self.request_typed(IpcRequest::ListSessions).await
    }

    pub async fn list_full_sessions(&mut self) -> Result<Vec<ChatSession>> {
        self.request_typed(IpcRequest::ListFullSessions).await
    }

    pub async fn list_sessions_by_agent(&mut self, agent_id: String) -> Result<Vec<ChatSession>> {
        self.request_typed(IpcRequest::ListSessionsByAgent { agent_id })
            .await
    }

    pub async fn list_sessions_by_skill(&mut self, skill_id: String) -> Result<Vec<ChatSession>> {
        self.request_typed(IpcRequest::ListSessionsBySkill { skill_id })
            .await
    }

    pub async fn count_sessions(&mut self) -> Result<usize> {
        self.request_typed(IpcRequest::CountSessions).await
    }

    pub async fn delete_sessions_older_than(&mut self, older_than_ms: i64) -> Result<usize> {
        self.request_typed(IpcRequest::DeleteSessionsOlderThan { older_than_ms })
            .await
    }

    pub async fn get_session(&mut self, id: String) -> Result<ChatSession> {
        self.request_typed(IpcRequest::GetSession { id }).await
    }

    pub async fn create_session(
        &mut self,
        agent_id: Option<String>,
        model: Option<String>,
        name: Option<String>,
        skill_id: Option<String>,
    ) -> Result<ChatSession> {
        self.request_typed(IpcRequest::CreateSession {
            agent_id,
            model,
            name,
            skill_id,
        })
        .await
    }

    pub async fn update_session(
        &mut self,
        id: String,
        updates: ChatSessionUpdate,
    ) -> Result<ChatSession> {
        let updates = to_contract(updates)?;
        self.request_typed(IpcRequest::UpdateSession { id, updates })
            .await
    }

    pub async fn rename_session(&mut self, id: String, name: String) -> Result<ChatSession> {
        self.request_typed(IpcRequest::RenameSession { id, name })
            .await
    }

    pub async fn archive_session(&mut self, id: String) -> Result<bool> {
        let resp: ArchiveResponse = self
            .request_typed(IpcRequest::ArchiveSession { id })
            .await?;
        Ok(resp.archived)
    }

    pub async fn delete_session(&mut self, id: String) -> Result<bool> {
        let resp: DeleteResponse = self.request_typed(IpcRequest::DeleteSession { id }).await?;
        Ok(resp.deleted)
    }

    pub async fn search_sessions(
        &mut self,
        query: String,
        agent_id: Option<String>,
        limit: Option<usize>,
    ) -> Result<Vec<ChatSessionSummary>> {
        self.request_typed(IpcRequest::SearchSessions {
            query,
            agent_id,
            limit,
        })
        .await
    }

    pub async fn add_message(
        &mut self,
        session_id: String,
        role: ChatRole,
        content: String,
    ) -> Result<ChatSession> {
        let role = to_contract(role)?;
        self.request_typed(IpcRequest::AddMessage {
            session_id,
            role,
            content,
        })
        .await
    }

    pub async fn append_message(
        &mut self,
        session_id: String,
        message: ChatMessage,
    ) -> Result<ChatSession> {
        let message = to_contract(message)?;
        self.request_typed(IpcRequest::AppendMessage {
            session_id,
            message,
        })
        .await
    }

    pub async fn execute_chat_session(
        &mut self,
        session_id: String,
        user_input: Option<String>,
        workspace_root: Option<String>,
    ) -> Result<ChatSession> {
        self.request_typed(IpcRequest::ExecuteChatSession {
            session_id,
            user_input,
            workspace_root,
        })
        .await
    }

    pub async fn execute_chat_session_stream<F>(
        &mut self,
        session_id: String,
        user_input: Option<String>,
        stream_id: String,
        workspace_root: Option<String>,
        scope: Option<ExecutionScope>,
        mut on_frame: F,
    ) -> Result<()>
    where
        F: FnMut(StreamFrame) -> Result<()>,
    {
        self.send_request_frame(&IpcRequest::ExecuteChatSessionStream {
            session_id,
            user_input,
            stream_id,
            workspace_root,
            scope,
        })
        .await?;

        loop {
            let buf = self.read_raw_frame().await?;
            let frame = read_stream_frame_or_ipc_error(
                &buf,
                "Failed to deserialize streaming IPC frame",
                "Unexpected success response while reading stream",
                "Unexpected Pong response while reading stream",
            )?;
            let terminal = matches!(frame, StreamFrame::Done { .. } | StreamFrame::Error(_));
            on_frame(frame)?;
            if terminal {
                break;
            }
        }

        Ok(())
    }

    pub async fn cancel_chat_session_stream(&mut self, stream_id: String) -> Result<bool> {
        let resp: CancelResponse = self
            .request_typed(IpcRequest::CancelChatSessionStream { stream_id })
            .await?;
        Ok(resp.canceled)
    }

    pub async fn steer_chat_session_stream(
        &mut self,
        session_id: String,
        instruction: String,
        scope: Option<ExecutionScope>,
    ) -> Result<bool> {
        let resp: SteerResponse = self
            .request_typed(IpcRequest::SteerChatSessionStream {
                session_id,
                instruction,
                scope,
            })
            .await?;
        Ok(resp.steered)
    }

    pub async fn subscribe_session_events<F>(&mut self, mut on_event: F) -> Result<()>
    where
        F: FnMut(ChatSessionEvent) -> Result<()>,
    {
        self.send_request_frame(&IpcRequest::SubscribeSessionEvents)
            .await?;

        loop {
            let buf = self.read_raw_frame().await?;
            match read_stream_frame_or_ipc_error(
                &buf,
                "Failed to deserialize session event stream frame",
                "Unexpected success response while reading session event stream",
                "Unexpected Pong response while reading session event stream",
            )? {
                StreamFrame::Start { .. } => {}
                StreamFrame::Event {
                    event: IpcStreamEvent::Session(event),
                } => {
                    on_event(event)?;
                }
                StreamFrame::Error(error) => {
                    bail!(
                        "Session event stream error: {}",
                        Self::format_ipc_error(&error)
                    );
                }
                StreamFrame::Done { .. } => break Ok(()),
                _ => {}
            }
        }
    }

    pub async fn get_session_messages(
        &mut self,
        session_id: String,
        limit: Option<usize>,
    ) -> Result<Vec<ChatMessage>> {
        self.request_typed(IpcRequest::GetSessionMessages { session_id, limit })
            .await
    }

    pub async fn list_runs(&mut self, query: RunListQuery) -> Result<Vec<RunSummary>> {
        let query = to_contract(query)?;
        self.request_typed(IpcRequest::ListRuns { query }).await
    }

    pub async fn get_execution_run_timeline(&mut self, run_id: String) -> Result<RunTimeline> {
        self.request_typed(IpcRequest::GetExecutionRunTimeline { run_id })
            .await
    }

    pub async fn build_agent_system_prompt(&mut self, agent_node: AgentNode) -> Result<String> {
        let agent_node = types::request::AgentNode::from(agent_node);
        let resp: PromptResponse = self
            .request_typed(IpcRequest::BuildAgentSystemPrompt { agent_node })
            .await?;
        Ok(resp.prompt)
    }

    pub async fn get_available_tool_definitions(&mut self) -> Result<Vec<ToolDefinition>> {
        self.request_typed(IpcRequest::GetAvailableToolDefinitions)
            .await
    }

    pub async fn execute_tool(
        &mut self,
        name: String,
        input: serde_json::Value,
    ) -> Result<ToolExecutionResult> {
        self.request_typed(IpcRequest::ExecuteTool { name, input })
            .await
    }
}

#[cfg(unix)]
pub async fn is_daemon_available(socket_path: &Path) -> bool {
    if !socket_path.exists() {
        return false;
    }
    match IpcClient::connect(socket_path).await {
        Ok(mut client) => client.ping().await,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
pub struct IpcClient;

#[cfg(not(unix))]
macro_rules! unsupported_result_methods {
    ($(fn $name:ident(&mut self $(, $arg:ident : $arg_ty:ty )* ) -> $ret:ty;)+) => {
        $(
            pub async fn $name(&mut self, $($arg: $arg_ty),*) -> Result<$ret> {
                $(let _ = &$arg;)*
                Self::unsupported()
            }
        )+
    };
}

#[cfg(not(unix))]
impl IpcClient {
    fn unsupported<T>() -> Result<T> {
        Err(anyhow::anyhow!("IPC is not supported on this platform"))
    }

    pub async fn connect(_socket_path: &Path) -> Result<Self> {
        Self::unsupported()
    }

    pub async fn request(&mut self, _req: IpcRequest) -> Result<IpcResponse> {
        Self::unsupported()
    }

    pub async fn ping(&mut self) -> bool {
        false
    }

    pub async fn get_status(&mut self) -> Result<IpcDaemonStatus> {
        Self::unsupported()
    }

    pub async fn request_typed<T: DeserializeOwned>(&mut self, _req: IpcRequest) -> Result<T> {
        Self::unsupported()
    }

    pub async fn request_optional<T: DeserializeOwned>(
        &mut self,
        _req: IpcRequest,
    ) -> Result<Option<T>> {
        Self::unsupported()
    }

    unsupported_result_methods! {
        fn list_skills(&mut self) -> Vec<Skill>;
        fn get_skill(&mut self, _id: String) -> Option<Skill>;
        fn get_skill_reference(&mut self, _skill_id: String, _ref_id: String) -> Option<String>;
        fn list_agents(&mut self) -> Vec<StoredAgent>;
        fn get_agent(&mut self, _id: String) -> StoredAgent;
        fn list_sessions(&mut self) -> Vec<ChatSessionSummary>;
        fn list_full_sessions(&mut self) -> Vec<ChatSession>;
        fn list_sessions_by_agent(&mut self, _agent_id: String) -> Vec<ChatSession>;
        fn list_sessions_by_skill(&mut self, _skill_id: String) -> Vec<ChatSession>;
        fn count_sessions(&mut self) -> usize;
        fn delete_sessions_older_than(&mut self, _older_than_ms: i64) -> usize;
        fn get_session(&mut self, _id: String) -> ChatSession;
        fn create_session(&mut self, _agent_id: Option<String>, _model: Option<String>, _name: Option<String>, _skill_id: Option<String>) -> ChatSession;
        fn update_session(&mut self, _id: String, _updates: ChatSessionUpdate) -> ChatSession;
        fn rename_session(&mut self, _id: String, _name: String) -> ChatSession;
        fn archive_session(&mut self, _id: String) -> bool;
        fn delete_session(&mut self, _id: String) -> bool;
        fn search_sessions(&mut self, _query: String, _agent_id: Option<String>, _limit: Option<usize>) -> Vec<ChatSessionSummary>;
        fn add_message(&mut self, _session_id: String, _role: ChatRole, _content: String) -> ChatSession;
        fn append_message(&mut self, _session_id: String, _message: ChatMessage) -> ChatSession;
        fn execute_chat_session(
            &mut self,
            _session_id: String,
            _user_input: Option<String>,
            _workspace_root: Option<String>,
        ) -> ChatSession;
        fn cancel_chat_session_stream(&mut self, _stream_id: String) -> bool;
        fn steer_chat_session_stream(&mut self, _session_id: String, _instruction: String, _scope: Option<types::ExecutionScope>) -> bool;
        fn get_session_messages(&mut self, _session_id: String, _limit: Option<usize>) -> Vec<ChatMessage>;
        fn list_runs(&mut self, _query: RunListQuery) -> Vec<RunSummary>;
        fn get_execution_run_timeline(&mut self, _run_id: String) -> crate::RunTimeline;
        fn build_agent_system_prompt(&mut self, _agent_node: AgentNode) -> String;
        fn init_python(&mut self) -> bool;
        fn get_available_tool_definitions(&mut self) -> Vec<ToolDefinition>;
        fn execute_tool(&mut self, _name: String, _input: serde_json::Value) -> ToolExecutionResult;
    }

    pub async fn execute_chat_session_stream<F>(
        &mut self,
        _session_id: String,
        _user_input: Option<String>,
        _stream_id: String,
        _workspace_root: Option<String>,
        _scope: Option<types::ExecutionScope>,
        _on_frame: F,
    ) -> Result<()>
    where
        F: FnMut(StreamFrame) -> Result<()>,
    {
        Self::unsupported()
    }

    pub async fn subscribe_session_events<F>(&mut self, _on_event: F) -> Result<()>
    where
        F: FnMut(ChatSessionEvent) -> Result<()>,
    {
        Self::unsupported()
    }
}

#[cfg(not(unix))]
pub async fn is_daemon_available(_socket_path: &Path) -> bool {
    false
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn format_ipc_error_without_details_uses_simple_message() {
        assert_eq!(
            IpcClient::format_ipc_error(&ErrorPayload::new(500, "boom", None)),
            "IPC error 500: boom"
        );
    }

    #[test]
    fn format_ipc_error_with_details_serializes_json() {
        let formatted = IpcClient::format_ipc_error(&ErrorPayload::new(
            400,
            "bad request",
            Some(serde_json::json!({ "field": "agent_id" })),
        ));

        assert!(formatted.contains('"'.to_string().as_str()));
        assert!(formatted.contains("bad request"));
        assert!(formatted.contains("agent_id"));
    }

    #[test]
    fn read_stream_frame_or_ipc_error_accepts_done_frame() {
        let encoded = serde_json::to_vec(&StreamFrame::Done {
            total_tokens: Some(7),
        })
        .unwrap();

        let decoded = read_stream_frame_or_ipc_error(
            &encoded,
            "decode failed",
            "unexpected success",
            "unexpected pong",
        )
        .unwrap();

        assert!(matches!(
            decoded,
            StreamFrame::Done {
                total_tokens: Some(7)
            }
        ));
    }

    #[test]
    fn read_stream_frame_or_ipc_error_accepts_error_frame() {
        let encoded = serde_json::to_vec(&StreamFrame::error(500, "boom")).unwrap();

        let decoded = read_stream_frame_or_ipc_error(
            &encoded,
            "decode failed",
            "unexpected success",
            "unexpected pong",
        )
        .unwrap();

        assert!(matches!(decoded, StreamFrame::Error(_)));
    }

    #[test]
    fn read_stream_frame_or_ipc_error_surfaces_ipc_error() {
        let encoded = serde_json::to_vec(&IpcResponse::error(404, "missing session")).unwrap();

        let err = read_stream_frame_or_ipc_error(
            &encoded,
            "decode failed",
            "unexpected success",
            "unexpected pong",
        )
        .unwrap_err();

        assert!(err.to_string().contains("missing session"));
    }

    #[test]
    fn read_stream_frame_or_ipc_error_rejects_unexpected_success() {
        let encoded =
            serde_json::to_vec(&IpcResponse::success(serde_json::json!({ "ok": true }))).unwrap();

        let err = read_stream_frame_or_ipc_error(
            &encoded,
            "decode failed",
            "unexpected success",
            "unexpected pong",
        )
        .unwrap_err();

        assert!(err.to_string().contains("unexpected success"));
    }

    #[test]
    fn read_stream_frame_or_ipc_error_rejects_unexpected_pong() {
        let encoded = serde_json::to_vec(&IpcResponse::Pong).unwrap();

        let err = read_stream_frame_or_ipc_error(
            &encoded,
            "decode failed",
            "unexpected success",
            "unexpected pong",
        )
        .unwrap_err();

        assert!(err.to_string().contains("unexpected pong"));
    }
}
