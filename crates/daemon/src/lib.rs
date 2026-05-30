//! RestFlow daemon, IPC, launcher, and foreground stream services.

use restflow_core::{AppCore, Secret, StoredAgent};
#[doc(hidden)]
pub mod runtime {
    pub use runner::{
        AgentOrchestratorImpl, AgentRuntimeExecutor, InteractiveSessionRequest, SessionInputMode,
        StorageBackedSubagentLookup, StreamDisplayMode, StreamEmitter, build_agent_system_prompt,
        build_turn_persistence_payload,
    };
}

#[doc(hidden)]
pub mod daemon {
    mod core_access {
        use super::ipc_client::IpcClient;
        use super::ipc_protocol::IpcRequest;
        use super::launcher::ensure_daemon_running;
        use super::request_mapper::to_contract;
        use crate::{AppCore, Secret};
        use anyhow::Result;
        use restflow_core::paths;
        use restflow_core::services::skills as skills_service;
        use restflow_core::storage::SystemConfig;
        use std::sync::Arc;
        use types::OkResponse;
        use types::{AgentNode, Skill};

        pub enum CoreAccess {
            Local(Arc<AppCore>),
            Remote(IpcClient),
        }

        impl CoreAccess {
            pub async fn connect() -> Result<Self> {
                let socket_path = paths::socket_path()?;
                ensure_daemon_running().await?;
                let client = IpcClient::connect(&socket_path).await?;
                Ok(CoreAccess::Remote(client))
            }

            pub async fn list_agents(&mut self) -> Result<Vec<crate::StoredAgent>> {
                match self {
                    CoreAccess::Local(core) => core.list_agents().await,
                    CoreAccess::Remote(client) => {
                        client.request_typed(IpcRequest::ListAgents).await
                    }
                }
            }

            pub async fn get_agent(&mut self, id: &str) -> Result<crate::StoredAgent> {
                match self {
                    CoreAccess::Local(core) => core.get_agent(id).await,
                    CoreAccess::Remote(client) => {
                        client
                            .request_typed(IpcRequest::GetAgent { id: id.to_string() })
                            .await
                    }
                }
            }

            pub async fn create_agent(
                &mut self,
                name: String,
                agent: AgentNode,
            ) -> Result<crate::StoredAgent> {
                match self {
                    CoreAccess::Local(core) => core.create_agent(name, agent).await,
                    CoreAccess::Remote(client) => {
                        let agent = types::request::AgentNode::from(agent);
                        client
                            .request_typed(IpcRequest::CreateAgent { name, agent })
                            .await
                    }
                }
            }

            pub async fn update_agent(
                &mut self,
                id: &str,
                name: Option<String>,
                agent: Option<AgentNode>,
            ) -> Result<crate::StoredAgent> {
                match self {
                    CoreAccess::Local(core) => core.update_agent(id, name, agent).await,
                    CoreAccess::Remote(client) => {
                        let agent = agent.map(types::request::AgentNode::from);
                        client
                            .request_typed(IpcRequest::UpdateAgent {
                                id: id.to_string(),
                                name,
                                agent,
                            })
                            .await
                    }
                }
            }

            pub async fn delete_agent(&mut self, id: &str) -> Result<()> {
                match self {
                    CoreAccess::Local(core) => core.delete_agent(id).await,
                    CoreAccess::Remote(client) => {
                        let _: OkResponse = client
                            .request_typed(IpcRequest::DeleteAgent { id: id.to_string() })
                            .await?;
                        Ok(())
                    }
                }
            }

            pub async fn list_skills(&mut self) -> Result<Vec<Skill>> {
                match self {
                    CoreAccess::Local(core) => skills_service::list_skills(core).await,
                    CoreAccess::Remote(client) => {
                        client.request_typed(IpcRequest::ListSkills).await
                    }
                }
            }

            pub async fn get_skill(&mut self, id: &str) -> Result<Option<Skill>> {
                match self {
                    CoreAccess::Local(core) => skills_service::get_skill(core, id).await,
                    CoreAccess::Remote(client) => {
                        client
                            .request_optional(IpcRequest::GetSkill { id: id.to_string() })
                            .await
                    }
                }
            }

            pub async fn list_secrets(&mut self) -> Result<Vec<Secret>> {
                match self {
                    CoreAccess::Local(core) => core.storage.secrets.list_secrets(),
                    CoreAccess::Remote(client) => {
                        client.request_typed(IpcRequest::ListSecrets).await
                    }
                }
            }

            pub async fn get_secret(&mut self, key: &str) -> Result<Option<String>> {
                match self {
                    CoreAccess::Local(core) => core.storage.secrets.get_secret(key),
                    CoreAccess::Remote(client) => {
                        let response: types::SecretResponse = client
                            .request_typed(IpcRequest::GetSecret {
                                key: key.to_string(),
                            })
                            .await?;
                        Ok(response.value)
                    }
                }
            }

            pub async fn set_secret(
                &mut self,
                key: &str,
                value: &str,
                description: Option<String>,
            ) -> Result<()> {
                match self {
                    CoreAccess::Local(core) => {
                        core.storage.secrets.set_secret(key, value, description)
                    }
                    CoreAccess::Remote(client) => {
                        let _: OkResponse = client
                            .request_typed(IpcRequest::SetSecret {
                                key: key.to_string(),
                                value: value.to_string(),
                                description,
                            })
                            .await?;
                        Ok(())
                    }
                }
            }

            pub async fn delete_secret(&mut self, key: &str) -> Result<()> {
                match self {
                    CoreAccess::Local(core) => core.storage.secrets.delete_secret(key),
                    CoreAccess::Remote(client) => {
                        let _: OkResponse = client
                            .request_typed(IpcRequest::DeleteSecret {
                                key: key.to_string(),
                            })
                            .await?;
                        Ok(())
                    }
                }
            }

            pub async fn get_config(&mut self) -> Result<SystemConfig> {
                match self {
                    CoreAccess::Local(core) => core.storage.config.get_effective_config(),
                    CoreAccess::Remote(client) => client.request_typed(IpcRequest::GetConfig).await,
                }
            }

            pub async fn get_global_config(&mut self) -> Result<SystemConfig> {
                match self {
                    CoreAccess::Local(core) => core.storage.config.get_global_config(),
                    CoreAccess::Remote(client) => {
                        client.request_typed(IpcRequest::GetGlobalConfig).await
                    }
                }
            }

            pub async fn set_config(&mut self, config: SystemConfig) -> Result<()> {
                match self {
                    CoreAccess::Local(core) => {
                        config.validate()?;
                        core.storage.config.update_config(config)
                    }
                    CoreAccess::Remote(client) => {
                        let config = to_contract(config)?;
                        let _: OkResponse = client
                            .request_typed(IpcRequest::SetConfig { config })
                            .await?;
                        Ok(())
                    }
                }
            }
        }
    }
    mod health {
        use super::ipc_client;
        use anyhow::Result;
        use chrono::{DateTime, Utc};
        use serde::Serialize;
        use std::path::PathBuf;

        pub struct HealthChecker {
            ipc_socket: PathBuf,
            http_url: Option<String>,
        }

        impl HealthChecker {
            pub fn new(ipc_socket: PathBuf, http_url: Option<String>) -> Self {
                Self {
                    ipc_socket,
                    http_url,
                }
            }

            pub async fn check(&self) -> HealthStatus {
                let ipc_ok = self.check_ipc().await;
                let http_ok = self.check_http().await;
                HealthStatus {
                    healthy: ipc_ok && http_ok.unwrap_or(true),
                    ipc: ipc_ok,
                    http: http_ok,
                    timestamp: Utc::now(),
                }
            }

            async fn check_ipc(&self) -> bool {
                ipc_client::is_daemon_available(&self.ipc_socket).await
            }

            async fn check_http(&self) -> Option<bool> {
                let url = self.http_url.as_ref()?;
                let client = reqwest::Client::new();
                let response = client.get(format!("{}/health", url)).send().await;
                Some(
                    response
                        .map(|resp| resp.status().is_success())
                        .unwrap_or(false),
                )
            }
        }

        #[derive(Serialize)]
        pub struct HealthStatus {
            pub healthy: bool,
            pub ipc: bool,
            pub http: Option<bool>,
            pub timestamp: DateTime<Utc>,
        }

        pub async fn check_health(
            ipc_socket: PathBuf,
            http_url: Option<String>,
        ) -> Result<HealthStatus> {
            let checker = HealthChecker::new(ipc_socket, http_url);
            Ok(checker.check().await)
        }
    }
    mod ipc_client {
        use super::ipc_protocol::{
            IPC_PROTOCOL_VERSION, IpcDaemonStatus, IpcRequest, IpcResponse, IpcStreamEvent,
            MAX_MESSAGE_SIZE, StreamFrame, ToolDefinition,
        };
        use crate::StoredAgent;
        use crate::daemon::request_mapper::to_contract;
        use anyhow::{Context, Result, bail};
        use restflow_core::session_events::ChatSessionEvent;
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

            pub async fn protocol_compatible(&mut self) -> bool {
                self.get_status()
                    .await
                    .is_ok_and(|status| status.protocol_version == IPC_PROTOCOL_VERSION)
            }

            pub async fn get_status(&mut self) -> Result<IpcDaemonStatus> {
                self.request_typed(IpcRequest::GetStatus).await
            }

            pub async fn request_typed<T: DeserializeOwned>(
                &mut self,
                req: IpcRequest,
            ) -> Result<T> {
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

            pub async fn list_sessions_by_agent(
                &mut self,
                agent_id: String,
            ) -> Result<Vec<ChatSession>> {
                self.request_typed(IpcRequest::ListSessionsByAgent { agent_id })
                    .await
            }

            pub async fn list_sessions_by_skill(
                &mut self,
                skill_id: String,
            ) -> Result<Vec<ChatSession>> {
                self.request_typed(IpcRequest::ListSessionsBySkill { skill_id })
                    .await
            }

            pub async fn count_sessions(&mut self) -> Result<usize> {
                self.request_typed(IpcRequest::CountSessions).await
            }

            pub async fn delete_sessions_older_than(
                &mut self,
                older_than_ms: i64,
            ) -> Result<usize> {
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

            pub async fn create_session_with_provider(
                &mut self,
                agent_id: Option<String>,
                provider: Option<String>,
                model: Option<String>,
                name: Option<String>,
                skill_id: Option<String>,
            ) -> Result<ChatSession> {
                let Some(provider) = provider else {
                    return self.create_session(agent_id, model, name, skill_id).await;
                };
                self.request_typed(IpcRequest::CreateSessionWithProvider {
                    agent_id,
                    provider: Some(provider),
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

            pub async fn rename_session(
                &mut self,
                id: String,
                name: String,
            ) -> Result<ChatSession> {
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
                let resp: DeleteResponse =
                    self.request_typed(IpcRequest::DeleteSession { id }).await?;
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

            pub async fn cancel_chat_session_stream(&mut self, stream_id: String) -> Result<bool> {
                let resp: CancelResponse = self
                    .request_typed(IpcRequest::CancelChatSessionStream { stream_id })
                    .await?;
                Ok(resp.canceled)
            }

            pub async fn cancel_chat_session_stream_scoped(
                &mut self,
                stream_id: String,
                session_id: Option<String>,
                scope: Option<ExecutionScope>,
            ) -> Result<bool> {
                if session_id.is_none() && scope.is_none() {
                    return self.cancel_chat_session_stream(stream_id).await;
                }
                let resp: CancelResponse = self
                    .request_typed(IpcRequest::CancelChatSessionStreamScoped {
                        stream_id,
                        session_id,
                        scope,
                    })
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

            pub async fn build_agent_system_prompt(
                &mut self,
                agent_node: AgentNode,
            ) -> Result<String> {
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
        }

        #[cfg(unix)]
        pub async fn is_daemon_available(socket_path: &Path) -> bool {
            if !socket_path.exists() {
                return false;
            }
            match IpcClient::connect(socket_path).await {
                Ok(mut client) => client.protocol_compatible().await,
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

            pub async fn request_typed<T: DeserializeOwned>(
                &mut self,
                _req: IpcRequest,
            ) -> Result<T> {
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
                fn create_session_with_provider(&mut self, _agent_id: Option<String>, _provider: Option<String>, _model: Option<String>, _name: Option<String>, _skill_id: Option<String>) -> ChatSession;
                fn update_session(&mut self, _id: String, _updates: ChatSessionUpdate) -> ChatSession;
                fn rename_session(&mut self, _id: String, _name: String) -> ChatSession;
                fn archive_session(&mut self, _id: String) -> bool;
                fn delete_session(&mut self, _id: String) -> bool;
                fn search_sessions(&mut self, _query: String, _agent_id: Option<String>, _limit: Option<usize>) -> Vec<ChatSessionSummary>;
                fn add_message(&mut self, _session_id: String, _role: ChatRole, _content: String) -> ChatSession;
                fn append_message(&mut self, _session_id: String, _message: ChatMessage) -> ChatSession;
                fn cancel_chat_session_stream(&mut self, _stream_id: String) -> bool;
                fn cancel_chat_session_stream_scoped(&mut self, _stream_id: String, _session_id: Option<String>, _scope: Option<types::ExecutionScope>) -> bool;
                fn steer_chat_session_stream(&mut self, _session_id: String, _instruction: String, _scope: Option<types::ExecutionScope>) -> bool;
                fn get_session_messages(&mut self, _session_id: String, _limit: Option<usize>) -> Vec<ChatMessage>;
                fn list_runs(&mut self, _query: RunListQuery) -> Vec<RunSummary>;
                fn build_agent_system_prompt(&mut self, _agent_node: AgentNode) -> String;
                fn init_python(&mut self) -> bool;
                fn get_available_tool_definitions(&mut self) -> Vec<ToolDefinition>;
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
                let encoded =
                    serde_json::to_vec(&IpcResponse::error(404, "missing session")).unwrap();

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
                    serde_json::to_vec(&IpcResponse::success(serde_json::json!({ "ok": true })))
                        .unwrap();

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
    }
    mod ipc_protocol {
        use serde_json::Value;
        use types::ResponseEnvelope;
        pub use types::{IpcDaemonStatus, IpcRequest, IpcStreamEvent, StreamFrame, ToolDefinition};

        /// Message frame: [4 bytes length LE][JSON payload]
        pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;
        pub const IPC_PROTOCOL_VERSION: &str = "4";

        pub type IpcResponse = ResponseEnvelope<Value>;

        #[cfg(test)]
        mod tests {
            use super::*;

            #[test]
            fn test_ipc_request_reexport_roundtrip() {
                let request = IpcRequest::Ping;

                let json = serde_json::to_string(&request).unwrap();
                let parsed: IpcRequest = serde_json::from_str(&json).unwrap();
                assert_eq!(parsed, request);
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
                    assert_eq!(error.kind, types::ErrorKind::NotFound);
                } else {
                    panic!("Wrong variant");
                }
            }

            #[test]
            fn test_protocol_version_is_v3() {
                assert_eq!(IPC_PROTOCOL_VERSION, "4");
            }

            #[test]
            fn test_daemon_status_roundtrip() {
                let status = IpcDaemonStatus {
                    status: types::DaemonRuntimeStatus::Running,
                    protocol_version: IPC_PROTOCOL_VERSION.to_string(),
                    daemon_version: "0.4.0".to_string(),
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
        }
    }
    mod ipc_server {
        use super::ipc_protocol::{
            IPC_PROTOCOL_VERSION, IpcDaemonStatus, IpcRequest, IpcResponse, IpcStreamEvent,
            MAX_MESSAGE_SIZE, StreamFrame, ToolDefinition,
        };
        use crate::AppCore;
        use crate::runtime::{
            AgentOrchestratorImpl, AgentRuntimeExecutor, InteractiveSessionRequest,
            SessionInputMode, StreamDisplayMode, StreamEmitter, build_turn_persistence_payload,
        };
        use anyhow::Result;
        use async_trait::async_trait;
        use chrono::Utc;
        use fs2::FileExt;
        use restflow_core::AgentDefaults;
        use restflow_core::services::{
            session::{
                PersistInteractiveTurnRequest, RuntimeIdentityGuard, SessionService,
                WorkspaceSessionCreateRequest,
            },
            skills as skills_service,
        };
        use restflow_core::session_events::subscribe_session_events;
        use std::collections::hash_map::DefaultHasher;
        use std::collections::{HashMap, HashSet, VecDeque};
        use std::fs::{File, OpenOptions};
        use std::future::Future;
        use std::hash::{Hash, Hasher};
        use std::io::{ErrorKind, Write};
        use std::path::PathBuf;
        use std::pin::Pin;
        use std::sync::Arc;
        use std::sync::OnceLock;
        use std::sync::atomic::{AtomicBool, Ordering};
        use tokio::sync::{Mutex, broadcast, mpsc};
        use tokio::task::JoinHandle;
        use tokio::time::{Duration, Instant, interval};
        use tracing::{debug, error, info, warn};
        use types::ExecutionScope;
        use types::store::ReplySender;
        use types::{
            AgentNode, ChatExecutionStatus, ChatMessage, ChatRole, ChatSession, ChatSessionSummary,
            ChatTurnEventKind, MessageExecution, SteerMessage, SteerSource,
        };
        use types::{ChatSessionEvent, DEFAULT_CHAT_MAX_SESSION_HISTORY};
        use uuid::Uuid;

        #[derive(Debug, Clone)]
        struct SessionAgentIdRepair {
            expected_existing_agent_id: String,
            repaired_agent_id: String,
        }

        #[path = "ipc_server/dispatch.rs"]
        mod dispatch {
            use super::runtime::{
                build_agent_system_prompt, cancel_chat_stream, get_runtime_tool_registry,
                resolve_agent_id, steer_chat_stream,
            };
            use super::*;
            use crate::daemon::request_mapper::{
                from_contract, invalid_request_response, invalid_validation_response,
            };
            use restflow_core::provider_policy::available_model_catalog;
            use types::request::WireModelRef;
            use types::{
                ArchiveResponse, CancelResponse, CleanupReportResponse, DeleteResponse, OkResponse,
                PromptResponse, SecretResponse, SteerResponse,
            };

            fn message_for_role(role: ChatRole, content: String) -> ChatMessage {
                let mut message = match role {
                    ChatRole::User => ChatMessage::user(content),
                    ChatRole::Assistant => ChatMessage::assistant(content),
                    ChatRole::System => ChatMessage::system(content),
                };
                if message.role == ChatRole::Assistant && message.execution.is_none() {
                    message.execution = Some(MessageExecution {
                        steps: Vec::new(),
                        duration_ms: 0,
                        tokens_used: 0,
                        cost_usd: None,
                        input_tokens: None,
                        output_tokens: None,
                        status: ChatExecutionStatus::Completed,
                    });
                }
                message
            }

            fn validate_and_update_config(
                core: &Arc<AppCore>,
                config: restflow_core::storage::SystemConfig,
            ) -> anyhow::Result<()> {
                config.validate()?;
                core.storage.config.update_config(config)
            }

            fn append_message_to_session(
                storage: &restflow_core::storage::Storage,
                session: &mut ChatSession,
                mut message: ChatMessage,
            ) -> IpcResponse {
                if message.id.trim().is_empty() {
                    message.id = Uuid::new_v4().to_string();
                }
                if message.role == ChatRole::Assistant && message.execution.is_none() {
                    message.execution = Some(MessageExecution {
                        steps: Vec::new(),
                        duration_ms: 0,
                        tokens_used: 0,
                        cost_usd: None,
                        input_tokens: None,
                        output_tokens: None,
                        status: ChatExecutionStatus::Completed,
                    });
                }
                session.add_message(message);
                if session.name == "New Chat" && session.messages.len() == 1 {
                    session.auto_name_from_first_message();
                }
                match SessionService::from_storage(storage).save_existing_session(session, "ipc") {
                    Ok(()) => IpcResponse::success(session.clone()),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }

            impl IpcServer {
                pub(super) async fn handle_ping() -> IpcResponse {
                    IpcResponse::Pong
                }

                pub(super) async fn handle_get_status() -> IpcResponse {
                    IpcResponse::success(build_daemon_status())
                }

                pub(super) async fn handle_list_agents(core: &Arc<AppCore>) -> IpcResponse {
                    match core.list_agents().await {
                        Ok(agents) => IpcResponse::success(agents),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_get_agent(
                    core: &Arc<AppCore>,
                    id: String,
                ) -> IpcResponse {
                    match core.get_agent(&id).await {
                        Ok(agent) => IpcResponse::success(agent),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_create_agent(
                    core: &Arc<AppCore>,
                    name: String,
                    agent: types::AgentNode,
                ) -> IpcResponse {
                    match core.create_agent(name, agent).await {
                        Ok(agent) => IpcResponse::success(agent),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_update_agent(
                    core: &Arc<AppCore>,
                    id: String,
                    name: Option<String>,
                    agent: Option<types::AgentNode>,
                ) -> IpcResponse {
                    match core.update_agent(&id, name, agent).await {
                        Ok(agent) => IpcResponse::success(agent),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_delete_agent(
                    core: &Arc<AppCore>,
                    id: String,
                ) -> IpcResponse {
                    match core.delete_agent(&id).await {
                        Ok(()) => IpcResponse::success(OkResponse { ok: true }),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_list_skills(core: &Arc<AppCore>) -> IpcResponse {
                    match skills_service::list_skills(core).await {
                        Ok(skills) => IpcResponse::success(skills),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_get_skill(
                    core: &Arc<AppCore>,
                    id: String,
                ) -> IpcResponse {
                    match skills_service::get_skill(core, &id).await {
                        Ok(Some(skill)) => IpcResponse::success(skill),
                        Ok(None) => IpcResponse::not_found("Skill"),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_get_skill_reference(
                    core: &Arc<AppCore>,
                    skill_id: String,
                    ref_id: String,
                ) -> IpcResponse {
                    match skills_service::get_skill_reference(core, &skill_id, &ref_id).await {
                        Ok(Some(content)) => IpcResponse::success(content),
                        Ok(None) => IpcResponse::not_found("Skill reference"),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_run_cleanup(core: &Arc<AppCore>) -> IpcResponse {
                    match restflow_core::services::cleanup::run_cleanup(core).await {
                        Ok(report) => IpcResponse::success(CleanupReportResponse {
                            chat_sessions: report.chat_sessions,
                            daemon_log_files: report.daemon_log_files,
                        }),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_list_secrets(core: &Arc<AppCore>) -> IpcResponse {
                    match core.storage.secrets.list_secrets() {
                        Ok(secrets) => IpcResponse::success(secrets),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_get_secret(
                    core: &Arc<AppCore>,
                    key: String,
                ) -> IpcResponse {
                    match core.storage.secrets.get_secret(&key) {
                        Ok(Some(value)) => {
                            IpcResponse::success(SecretResponse { value: Some(value) })
                        }
                        Ok(None) => IpcResponse::not_found("Secret"),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_set_secret(
                    core: &Arc<AppCore>,
                    key: String,
                    value: String,
                    description: Option<String>,
                ) -> IpcResponse {
                    match core.storage.secrets.set_secret(&key, &value, description) {
                        Ok(()) => IpcResponse::success(OkResponse { ok: true }),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_delete_secret(
                    core: &Arc<AppCore>,
                    key: String,
                ) -> IpcResponse {
                    match core.storage.secrets.delete_secret(&key) {
                        Ok(()) => IpcResponse::success(OkResponse { ok: true }),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_get_config(core: &Arc<AppCore>) -> IpcResponse {
                    match core.storage.config.get_effective_config() {
                        Ok(config) => IpcResponse::success(config),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_get_global_config(core: &Arc<AppCore>) -> IpcResponse {
                    match core.storage.config.get_global_config() {
                        Ok(config) => IpcResponse::success(config),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_set_config(
                    core: &Arc<AppCore>,
                    config: restflow_core::storage::SystemConfig,
                ) -> IpcResponse {
                    match validate_and_update_config(core, config) {
                        Ok(()) => IpcResponse::success(OkResponse { ok: true }),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_list_execution_containers(
                    core: &Arc<AppCore>,
                ) -> IpcResponse {
                    let service = SessionService::from_storage(&core.storage);
                    match service.list_execution_containers() {
                        Ok(containers) => IpcResponse::success(containers),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_list_runs(
                    core: &Arc<AppCore>,
                    query: types::RunListQuery,
                ) -> IpcResponse {
                    let service = SessionService::from_storage(&core.storage);
                    match service.list_runs(&query) {
                        Ok(sessions) => IpcResponse::success(sessions),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_list_sessions(core: &Arc<AppCore>) -> IpcResponse {
                    let session_service = SessionService::from_storage(&core.storage);
                    match session_service.list_session_summaries(None, None, false) {
                        Ok(summaries) => IpcResponse::success(summaries),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_list_full_sessions(core: &Arc<AppCore>) -> IpcResponse {
                    let session_service = SessionService::from_storage(&core.storage);
                    match session_service.list_session_views(None, None, false) {
                        Ok(sessions) => IpcResponse::success(sessions),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_list_sessions_by_agent(
                    core: &Arc<AppCore>,
                    agent_id: String,
                ) -> IpcResponse {
                    let session_service = SessionService::from_storage(&core.storage);
                    match session_service.list_session_views(Some(&agent_id), None, false) {
                        Ok(sessions) => IpcResponse::success(sessions),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_list_sessions_by_skill(
                    core: &Arc<AppCore>,
                    skill_id: String,
                ) -> IpcResponse {
                    let session_service = SessionService::from_storage(&core.storage);
                    match session_service.list_session_views(None, Some(&skill_id), false) {
                        Ok(sessions) => IpcResponse::success(sessions),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_count_sessions(core: &Arc<AppCore>) -> IpcResponse {
                    let session_service = SessionService::from_storage(&core.storage);
                    match session_service.list_session_summaries(None, None, false) {
                        Ok(sessions) => IpcResponse::success(sessions.len()),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_delete_sessions_older_than(
                    core: &Arc<AppCore>,
                    older_than_ms: i64,
                ) -> IpcResponse {
                    let session_service = SessionService::from_storage(&core.storage);
                    match session_service.cleanup_workspace_sessions_older_than(older_than_ms) {
                        Ok(stats) => IpcResponse::success(stats.deleted),
                        Err(err) => ipc_session_lifecycle_error(err),
                    }
                }

                pub(super) async fn handle_get_session(
                    core: &Arc<AppCore>,
                    id: String,
                ) -> IpcResponse {
                    let session_service = SessionService::from_storage(&core.storage);
                    match session_service.get_session_view(&id) {
                        Ok(Some(session)) => IpcResponse::success(session),
                        Ok(None) => IpcResponse::not_found("Session"),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_create_session(
                    core: &Arc<AppCore>,
                    agent_id: Option<String>,
                    provider: Option<String>,
                    model: Option<String>,
                    name: Option<String>,
                    skill_id: Option<String>,
                ) -> IpcResponse {
                    let session_service = SessionService::from_storage(&core.storage);
                    let agent_id = match resolve_agent_id(core, agent_id) {
                        Ok(agent_id) => agent_id,
                        Err(err) => return IpcResponse::error(400, err.to_string()),
                    };
                    match session_service.create_workspace_session_from_request(
                        &core.storage,
                        WorkspaceSessionCreateRequest::new(agent_id)
                            .with_provider(provider)
                            .with_model(model)
                            .with_name(name)
                            .with_skill_id(skill_id),
                    ) {
                        Ok(session) => IpcResponse::success(session),
                        Err(err) => ipc_session_lifecycle_error(err),
                    }
                }

                pub(super) async fn handle_update_session(
                    core: &Arc<AppCore>,
                    id: String,
                    updates: types::ChatSessionUpdate,
                ) -> IpcResponse {
                    let session_service = SessionService::from_storage(&core.storage);
                    let validated_updates = types::ChatSessionUpdate {
                        agent_id: match updates.agent_id {
                            Some(agent_id) => {
                                match core.storage.agents.resolve_existing_agent_id(&agent_id) {
                                    Ok(resolved) => Some(resolved),
                                    Err(err) => return IpcResponse::error(400, err.to_string()),
                                }
                            }
                            None => None,
                        },
                        model: updates.model,
                        name: updates.name,
                    };
                    match session_service.update_session(&id, validated_updates) {
                        Ok(Some(session)) => IpcResponse::success(session),
                        Ok(None) => IpcResponse::not_found("Session"),
                        Err(err) => ipc_session_lifecycle_error(err),
                    }
                }

                pub(super) async fn handle_rename_session(
                    core: &Arc<AppCore>,
                    id: String,
                    name: String,
                ) -> IpcResponse {
                    let session_service = SessionService::from_storage(&core.storage);
                    match session_service.rename_session(&id, name) {
                        Ok(Some(session)) => IpcResponse::success(session),
                        Ok(None) => IpcResponse::not_found("Session"),
                        Err(err) => ipc_session_lifecycle_error(err),
                    }
                }

                pub(super) async fn handle_archive_session(
                    core: &Arc<AppCore>,
                    id: String,
                ) -> IpcResponse {
                    let session_service = SessionService::from_storage(&core.storage);
                    match session_service.archive_session(&id) {
                        Ok(archived) => IpcResponse::success(ArchiveResponse { archived }),
                        Err(err) => ipc_session_lifecycle_error(err),
                    }
                }

                pub(super) async fn handle_delete_session(
                    core: &Arc<AppCore>,
                    id: String,
                ) -> IpcResponse {
                    let session_service = SessionService::from_storage(&core.storage);
                    match session_service.delete_session(&id) {
                        Ok(deleted) => IpcResponse::success(DeleteResponse { deleted }),
                        Err(err) => ipc_session_lifecycle_error(err),
                    }
                }

                pub(super) async fn handle_search_sessions(
                    core: &Arc<AppCore>,
                    query: String,
                    agent_id: Option<String>,
                    limit: Option<usize>,
                ) -> IpcResponse {
                    let session_service = SessionService::from_storage(&core.storage);
                    match session_service.search_session_views(
                        &query,
                        agent_id.as_deref(),
                        None,
                        false,
                        limit.unwrap_or(20).max(1),
                    ) {
                        Ok(sessions) => {
                            let matches: Vec<ChatSessionSummary> =
                                sessions.iter().map(ChatSessionSummary::from).collect();
                            IpcResponse::success(matches)
                        }
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_add_message(
                    core: &Arc<AppCore>,
                    session_id: String,
                    role: ChatRole,
                    content: String,
                ) -> IpcResponse {
                    let session_service = SessionService::from_storage(&core.storage);
                    let mut session = match session_service.get_session_view(&session_id) {
                        Ok(Some(session)) => session,
                        Ok(None) => return IpcResponse::not_found("Session"),
                        Err(err) => return IpcResponse::error(500, err.to_string()),
                    };
                    let message = message_for_role(role, content);
                    append_message_to_session(&core.storage, &mut session, message)
                }

                pub(super) async fn handle_append_message(
                    core: &Arc<AppCore>,
                    session_id: String,
                    message: ChatMessage,
                ) -> IpcResponse {
                    let session_service = SessionService::from_storage(&core.storage);
                    let mut session = match session_service.get_session_view(&session_id) {
                        Ok(Some(session)) => session,
                        Ok(None) => return IpcResponse::not_found("Session"),
                        Err(err) => return IpcResponse::error(500, err.to_string()),
                    };
                    append_message_to_session(&core.storage, &mut session, message)
                }

                pub(super) async fn handle_steer_chat_session_stream(
                    core: &Arc<AppCore>,
                    session_id: String,
                    instruction: String,
                    scope: Option<ExecutionScope>,
                ) -> IpcResponse {
                    match steer_chat_stream(core, &session_id, &instruction, scope.as_ref()).await {
                        Ok(steered) => IpcResponse::success(SteerResponse { steered }),
                        Err(error) => IpcResponse::error(500, error.to_string()),
                    }
                }

                pub(super) async fn handle_cancel_chat_session_stream(
                    core: &Arc<AppCore>,
                    stream_id: String,
                    session_id: Option<String>,
                    scope: Option<ExecutionScope>,
                ) -> IpcResponse {
                    match cancel_chat_stream(
                        core,
                        &stream_id,
                        session_id.as_deref(),
                        scope.as_ref(),
                    )
                    .await
                    {
                        Ok(canceled) => IpcResponse::success(CancelResponse { canceled }),
                        Err(error) => IpcResponse::error(500, error.to_string()),
                    }
                }

                pub(super) async fn handle_get_session_messages(
                    core: &Arc<AppCore>,
                    session_id: String,
                    limit: Option<usize>,
                ) -> IpcResponse {
                    let session_service = SessionService::from_storage(&core.storage);
                    let session = match session_service.get_session_view(&session_id) {
                        Ok(Some(session)) => session,
                        Ok(None) => return IpcResponse::not_found("Session"),
                        Err(err) => return IpcResponse::error(500, err.to_string()),
                    };
                    let count = limit.unwrap_or(session.messages.len());
                    let messages = session
                        .messages
                        .iter()
                        .cloned()
                        .rev()
                        .take(count)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect::<Vec<_>>();
                    IpcResponse::success(messages)
                }

                pub(super) async fn handle_subscribe_session_events_unsupported() -> IpcResponse {
                    IpcResponse::error(-3, "Session event streaming requires stream mode")
                }

                pub(super) async fn handle_switch_session_model(
                    core: &Arc<AppCore>,
                    session_id: String,
                    model_ref: WireModelRef,
                ) -> IpcResponse {
                    let session_service = SessionService::from_storage(&core.storage);
                    match session_service.switch_session_model(
                        &session_id,
                        model_ref.provider,
                        model_ref.model,
                    ) {
                        Ok(Some(session)) => IpcResponse::success(session),
                        Ok(None) => IpcResponse::not_found("session"),
                        Err(error) => ipc_session_lifecycle_error(error),
                    }
                }

                pub(super) async fn handle_get_system_info() -> IpcResponse {
                    IpcResponse::success(serde_json::json!({
                        "pid": std::process::id(),
                    }))
                }

                pub(super) async fn handle_get_available_models(
                    core: &Arc<AppCore>,
                ) -> IpcResponse {
                    IpcResponse::success(available_model_catalog(&core.storage.secrets))
                }

                pub(super) async fn handle_get_available_tools(
                    core: &Arc<AppCore>,
                    runtime_tool_registry: &OnceLock<types::toolset::ToolRegistry>,
                ) -> IpcResponse {
                    match get_runtime_tool_registry(core, runtime_tool_registry) {
                        Ok(registry) => {
                            let tools: Vec<String> = registry
                                .list()
                                .iter()
                                .map(|name| name.to_string())
                                .collect();
                            IpcResponse::success(tools)
                        }
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_get_available_tool_definitions(
                    core: &Arc<AppCore>,
                    runtime_tool_registry: &OnceLock<types::toolset::ToolRegistry>,
                ) -> IpcResponse {
                    match get_runtime_tool_registry(core, runtime_tool_registry) {
                        Ok(registry) => {
                            let tools: Vec<ToolDefinition> = registry
                                .schemas()
                                .into_iter()
                                .map(|schema| ToolDefinition {
                                    name: schema.name,
                                    description: schema.description,
                                    parameters: schema.parameters,
                                })
                                .collect();
                            IpcResponse::success(tools)
                        }
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_list_mcp_servers() -> IpcResponse {
                    IpcResponse::success(Vec::<String>::new())
                }

                pub(super) async fn handle_build_agent_system_prompt(
                    core: &Arc<AppCore>,
                    agent_node: types::AgentNode,
                ) -> IpcResponse {
                    match build_agent_system_prompt(core, agent_node) {
                        Ok(prompt) => IpcResponse::success(PromptResponse { prompt }),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                }

                pub(super) async fn handle_shutdown() -> IpcResponse {
                    IpcResponse::success(serde_json::json!({ "shutting_down": true }))
                }

                pub(crate) async fn process(
                    core: &Arc<AppCore>,
                    runtime_tool_registry: &OnceLock<types::toolset::ToolRegistry>,
                    request: IpcRequest,
                ) -> IpcResponse {
                    match request {
                        IpcRequest::Ping => Self::handle_ping().await,
                        IpcRequest::GetStatus => Self::handle_get_status().await,
                        IpcRequest::ListAgents => Self::handle_list_agents(core).await,
                        IpcRequest::GetAgent { id } => Self::handle_get_agent(core, id).await,
                        IpcRequest::CreateAgent { name, agent } => {
                            match types::AgentNode::try_from(agent) {
                                Ok(agent) => Self::handle_create_agent(core, name, agent).await,
                                Err(errors) => invalid_validation_response(errors),
                            }
                        }
                        IpcRequest::UpdateAgent { id, name, agent } => {
                            let agent = match agent.map(types::AgentNode::try_from).transpose() {
                                Ok(agent) => agent,
                                Err(errors) => return invalid_validation_response(errors),
                            };
                            Self::handle_update_agent(core, id, name, agent).await
                        }
                        IpcRequest::DeleteAgent { id } => Self::handle_delete_agent(core, id).await,
                        IpcRequest::ListSkills => Self::handle_list_skills(core).await,
                        IpcRequest::GetSkill { id } => Self::handle_get_skill(core, id).await,
                        IpcRequest::GetSkillReference { skill_id, ref_id } => {
                            Self::handle_get_skill_reference(core, skill_id, ref_id).await
                        }
                        IpcRequest::RunCleanup => Self::handle_run_cleanup(core).await,
                        IpcRequest::ListSecrets => Self::handle_list_secrets(core).await,
                        IpcRequest::GetSecret { key } => Self::handle_get_secret(core, key).await,
                        IpcRequest::SetSecret {
                            key,
                            value,
                            description,
                        } => Self::handle_set_secret(core, key, value, description).await,
                        IpcRequest::DeleteSecret { key } => {
                            Self::handle_delete_secret(core, key).await
                        }
                        IpcRequest::GetConfig => Self::handle_get_config(core).await,
                        IpcRequest::GetGlobalConfig => Self::handle_get_global_config(core).await,
                        IpcRequest::SetConfig { config } => match from_contract(config) {
                            Ok(config) => Self::handle_set_config(core, config).await,
                            Err(err) => invalid_request_response(err),
                        },
                        IpcRequest::ListSessions => Self::handle_list_sessions(core).await,
                        IpcRequest::ListFullSessions => Self::handle_list_full_sessions(core).await,
                        IpcRequest::ListSessionsByAgent { agent_id } => {
                            Self::handle_list_sessions_by_agent(core, agent_id).await
                        }
                        IpcRequest::ListSessionsBySkill { skill_id } => {
                            Self::handle_list_sessions_by_skill(core, skill_id).await
                        }
                        IpcRequest::CountSessions => Self::handle_count_sessions(core).await,
                        IpcRequest::DeleteSessionsOlderThan { older_than_ms } => {
                            Self::handle_delete_sessions_older_than(core, older_than_ms).await
                        }
                        IpcRequest::GetSession { id } => Self::handle_get_session(core, id).await,
                        IpcRequest::CreateSession {
                            agent_id,
                            model,
                            name,
                            skill_id,
                        } => {
                            Self::handle_create_session(core, agent_id, None, model, name, skill_id)
                                .await
                        }
                        IpcRequest::CreateSessionWithProvider {
                            agent_id,
                            provider,
                            model,
                            name,
                            skill_id,
                        } => {
                            Self::handle_create_session(
                                core, agent_id, provider, model, name, skill_id,
                            )
                            .await
                        }
                        IpcRequest::UpdateSession { id, updates } => match from_contract(updates) {
                            Ok(updates) => Self::handle_update_session(core, id, updates).await,
                            Err(err) => invalid_request_response(err),
                        },
                        IpcRequest::RenameSession { id, name } => {
                            Self::handle_rename_session(core, id, name).await
                        }
                        IpcRequest::ArchiveSession { id } => {
                            Self::handle_archive_session(core, id).await
                        }
                        IpcRequest::DeleteSession { id } => {
                            Self::handle_delete_session(core, id).await
                        }
                        IpcRequest::SearchSessions {
                            query,
                            agent_id,
                            limit,
                        } => Self::handle_search_sessions(core, query, agent_id, limit).await,
                        IpcRequest::AddMessage {
                            session_id,
                            role,
                            content,
                        } => Self::handle_add_message(core, session_id, role, content).await,
                        IpcRequest::AppendMessage {
                            session_id,
                            message,
                        } => match from_contract(message) {
                            Ok(message) => {
                                Self::handle_append_message(core, session_id, message).await
                            }
                            Err(err) => invalid_request_response(err),
                        },
                        IpcRequest::SteerChatSessionStream {
                            session_id,
                            instruction,
                            scope,
                        } => {
                            Self::handle_steer_chat_session_stream(
                                core,
                                session_id,
                                instruction,
                                scope,
                            )
                            .await
                        }
                        IpcRequest::CancelChatSessionStream { stream_id } => {
                            Self::handle_cancel_chat_session_stream(core, stream_id, None, None)
                                .await
                        }
                        IpcRequest::CancelChatSessionStreamScoped {
                            stream_id,
                            session_id,
                            scope,
                        } => {
                            Self::handle_cancel_chat_session_stream(
                                core, stream_id, session_id, scope,
                            )
                            .await
                        }
                        IpcRequest::GetSessionMessages { session_id, limit } => {
                            Self::handle_get_session_messages(core, session_id, limit).await
                        }
                        IpcRequest::ListExecutionContainers => {
                            Self::handle_list_execution_containers(core).await
                        }
                        IpcRequest::ListRuns { query } => match from_contract(query) {
                            Ok(query) => Self::handle_list_runs(core, query).await,
                            Err(err) => invalid_request_response(err),
                        },
                        IpcRequest::SubscribeSessionEvents => {
                            Self::handle_subscribe_session_events_unsupported().await
                        }
                        IpcRequest::SwitchSessionModel {
                            session_id,
                            model_ref,
                            reason: _,
                        } => Self::handle_switch_session_model(core, session_id, model_ref).await,
                        IpcRequest::GetSystemInfo => Self::handle_get_system_info().await,
                        IpcRequest::GetAvailableModels => {
                            Self::handle_get_available_models(core).await
                        }
                        IpcRequest::GetAvailableTools => {
                            Self::handle_get_available_tools(core, runtime_tool_registry).await
                        }
                        IpcRequest::GetAvailableToolDefinitions => {
                            Self::handle_get_available_tool_definitions(core, runtime_tool_registry)
                                .await
                        }
                        IpcRequest::ListMcpServers => Self::handle_list_mcp_servers().await,
                        IpcRequest::BuildAgentSystemPrompt { agent_node } => {
                            match types::AgentNode::try_from(agent_node) {
                                Ok(agent_node) => {
                                    Self::handle_build_agent_system_prompt(core, agent_node).await
                                }
                                Err(errors) => invalid_validation_response(errors),
                            }
                        }
                        IpcRequest::Shutdown => Self::handle_shutdown().await,
                        _ => IpcResponse::error(400, "Unsupported IPC request"),
                    }
                }
            }
        }
        #[path = "ipc_server/runtime.rs"]
        mod runtime {
            use super::*;
            use thiserror::Error;

            #[derive(Debug, Error)]
            pub(super) enum ExecuteChatSessionError {
                #[error("{}", types::SESSION_NOT_FOUND)]
                SessionNotFound,
                #[error("No user message found in session")]
                MissingUserMessage,
                #[error("Interactive execution completed without assistant output")]
                EmptyAssistantOutput,
                #[error("Interactive execution was canceled")]
                Canceled,
                #[error(transparent)]
                Internal(#[from] anyhow::Error),
            }

            impl ExecuteChatSessionError {
                pub(super) fn status_code(&self) -> i32 {
                    match self {
                        Self::SessionNotFound => 404,
                        Self::MissingUserMessage => 400,
                        Self::EmptyAssistantOutput => 500,
                        Self::Canceled => 409,
                        Self::Internal(_) => 500,
                    }
                }
            }

            pub(super) struct ExecuteChatSessionRequest {
                pub session_id: String,
                pub user_input: Option<String>,
                pub turn_id: String,
                pub workspace_root: Option<String>,
                pub ack_frame_tx: Option<mpsc::UnboundedSender<StreamFrame>>,
                pub emitter: Option<Box<dyn StreamEmitter>>,
                pub steer_rx: Option<mpsc::Receiver<SteerMessage>>,
                pub has_text_streamed: Option<Arc<AtomicBool>>,
            }

            pub(super) fn create_runtime_tool_registry_with_assessment(
                core: &Arc<AppCore>,
            ) -> anyhow::Result<types::toolset::ToolRegistry> {
                let agent_defaults = load_agent_defaults_from_core(core);
                runner::default_runtime_tool_registry(
                    &core.storage,
                    agent_defaults.bash_timeout_secs,
                )
            }

            pub(super) fn get_runtime_tool_registry<'a>(
                core: &Arc<AppCore>,
                runtime_tool_registry: &'a OnceLock<types::toolset::ToolRegistry>,
            ) -> Result<&'a types::toolset::ToolRegistry, String> {
                if let Some(registry) = runtime_tool_registry.get() {
                    return Ok(registry);
                }

                let registry = create_runtime_tool_registry_with_assessment(core)
                    .map_err(|error| error.to_string())?;
                let _ = runtime_tool_registry.set(registry);
                runtime_tool_registry
                    .get()
                    .ok_or_else(|| "runtime tool registry initialization failed".to_string())
            }

            pub(super) fn load_agent_defaults_from_core(core: &Arc<AppCore>) -> AgentDefaults {
                match core.storage.config.get_effective_config() {
                    Ok(config) => config.agent,
                    Err(error) => {
                        warn!(
                            error = %error,
                            "Failed to load system config for chat runtime; falling back to default agent config"
                        );
                        AgentDefaults::default()
                    }
                }
            }

            pub(super) fn load_chat_max_session_history_from_core(core: &Arc<AppCore>) -> usize {
                match core.storage.config.get_effective_config() {
                    Ok(config) => config.runtime_defaults.chat_max_session_history,
                    Err(error) => {
                        warn!(
                            error = %error,
                            "Failed to load runtime config for chat history; falling back to default history size"
                        );
                        DEFAULT_CHAT_MAX_SESSION_HISTORY
                    }
                }
            }

            pub(super) fn create_chat_executor(core: &Arc<AppCore>) -> AgentRuntimeExecutor {
                let agent_defaults = load_agent_defaults_from_core(core);
                runner::chat_runtime_executor(core.storage.clone(), agent_defaults)
            }

            pub(super) async fn cancel_chat_stream(
                core: &Arc<AppCore>,
                stream_id: &str,
                session_id: Option<&str>,
                scope: Option<&types::ExecutionScope>,
            ) -> Result<bool> {
                let binding = {
                    let session_streams = active_chat_stream_sessions().lock().await;
                    session_streams
                        .iter()
                        .find(|(_, binding)| binding.stream_id == stream_id)
                        .map(|(session_id, binding)| (session_id.clone(), binding.clone()))
                };

                let Some((bound_session_id, binding)) = binding else {
                    if let Some(session_id) = session_id {
                        let mut pending = pending_canceled_chat_streams().lock().await;
                        purge_expired_pending_canceled_streams(&mut pending);
                        pending.insert(
                            stream_id.to_string(),
                            PendingCanceledChatStream::new(session_id, scope.cloned()),
                        );
                        return Ok(true);
                    }
                    if scope.is_some() {
                        return Ok(false);
                    }
                    if let Some(handle) = active_chat_streams().lock().await.remove(stream_id) {
                        handle.abort();
                        let _ = handle.await;
                        completed_chat_streams().lock().await.remove(stream_id);
                        active_chat_stream_steers().lock().await.remove(stream_id);
                        return Ok(true);
                    }
                    return Ok(false);
                };

                if session_id.is_some_and(|expected| expected != bound_session_id) {
                    return Ok(false);
                }
                if binding.scope.is_some() && scope.is_none() {
                    return Ok(false);
                }
                if scope.is_some() && binding.scope.as_ref() != scope {
                    return Ok(false);
                }

                if !active_chat_streams().lock().await.contains_key(stream_id) {
                    cancel_pending_turn_in_session_store(
                        core,
                        &bound_session_id,
                        stream_id,
                        binding.user_input.as_deref(),
                    )?;
                    let mut pending = pending_canceled_chat_streams().lock().await;
                    purge_expired_pending_canceled_streams(&mut pending);
                    pending.insert(
                        stream_id.to_string(),
                        PendingCanceledChatStream::new(&bound_session_id, binding.scope.clone()),
                    );
                    active_chat_stream_steers().lock().await.remove(stream_id);
                    let mut session_streams = active_chat_stream_sessions().lock().await;
                    if session_streams
                        .get(&bound_session_id)
                        .is_some_and(|active| active.stream_id == stream_id)
                    {
                        session_streams.remove(&bound_session_id);
                    }
                    return Ok(true);
                }
                cancel_pending_turn_in_session_store(
                    core,
                    &bound_session_id,
                    stream_id,
                    binding.user_input.as_deref(),
                )?;
                if let Some(handle) = active_chat_streams().lock().await.remove(stream_id) {
                    handle.abort();
                    let _ = handle.await;
                }
                completed_chat_streams().lock().await.remove(stream_id);
                active_chat_stream_steers().lock().await.remove(stream_id);
                let mut session_streams = active_chat_stream_sessions().lock().await;
                if session_streams
                    .get(&bound_session_id)
                    .is_some_and(|active| active.stream_id == stream_id)
                {
                    session_streams.remove(&bound_session_id);
                }
                Ok(true)
            }

            pub(super) async fn steer_chat_stream(
                core: &Arc<AppCore>,
                session_id: &str,
                instruction: &str,
                scope: Option<&types::ExecutionScope>,
            ) -> Result<bool> {
                let instruction = instruction.trim();
                if instruction.is_empty() {
                    return Ok(false);
                }
                let binding = {
                    let session_streams = active_chat_stream_sessions().lock().await;
                    session_streams
                        .get(session_id)
                        .filter(|binding| binding.scope.as_ref() == scope)
                        .cloned()
                };

                let Some(binding) = binding else {
                    return Ok(false);
                };

                let sender = {
                    let steers = active_chat_stream_steers().lock().await;
                    steers.get(&binding.stream_id).cloned()
                };
                let Some(sender) = sender else {
                    return Ok(false);
                };

                if !persist_steer_if_stream_still_registered(
                    core,
                    session_id,
                    &binding,
                    instruction,
                )
                .await?
                {
                    return Ok(false);
                }

                let steer = SteerMessage::message(instruction.to_string(), SteerSource::User);
                match sender.send(steer).await {
                    Ok(()) => Ok(true),
                    Err(_) => {
                        record_turn_event_in_session_store(
                            core,
                            session_id,
                            &binding.turn_id,
                            ChatTurnEventKind::Error {
                                message: "Steer update was not delivered because the active stream closed".to_string(),
                            },
                        )?;
                        active_chat_stream_steers()
                            .lock()
                            .await
                            .remove(&binding.stream_id);
                        let mut session_streams = active_chat_stream_sessions().lock().await;
                        if session_streams
                            .get(session_id)
                            .is_some_and(|active| active.stream_id == binding.stream_id)
                        {
                            session_streams.remove(session_id);
                        }
                        Ok(false)
                    }
                }
            }

            pub(super) async fn persist_steer_if_stream_still_registered(
                core: &Arc<AppCore>,
                session_id: &str,
                binding: &ActiveChatStreamBinding,
                instruction: &str,
            ) -> Result<bool> {
                let session_streams = active_chat_stream_sessions().lock().await;
                if !session_streams
                    .get(session_id)
                    .is_some_and(|active| active == binding)
                {
                    return Ok(false);
                }

                let steers = active_chat_stream_steers().lock().await;
                if !steers.contains_key(&binding.stream_id) {
                    return Ok(false);
                }
                drop(steers);

                core.storage
                    .file_sessions
                    .append_turn_user_message_if_non_terminal(
                        session_id,
                        &binding.turn_id,
                        instruction,
                    )
            }

            pub(super) fn latest_assistant_payload(
                session: &ChatSession,
            ) -> Option<(String, Option<u32>)> {
                session
                    .messages
                    .iter()
                    .rev()
                    .find(|message| {
                        message.role == ChatRole::Assistant && !message.content.trim().is_empty()
                    })
                    .map(|message| {
                        (
                            message.content.trim().to_string(),
                            message.execution.as_ref().map(|exec| exec.tokens_used),
                        )
                    })
            }

            fn latest_turn_assistant_output(
                session: &ChatSession,
                turn_start_index: usize,
            ) -> Option<String> {
                session
                    .messages
                    .iter()
                    .skip(turn_start_index)
                    .rev()
                    .find(|message| {
                        message.role == ChatRole::Assistant && !message.content.trim().is_empty()
                    })
                    .map(|message| message.content.trim().to_string())
            }

            fn select_final_assistant_output(
                execution_output: &str,
                buffered_replies: &[String],
                session: &ChatSession,
                turn_start_index: usize,
            ) -> Option<String> {
                let trimmed = execution_output.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }

                if let Some(content) = buffered_replies
                    .iter()
                    .rev()
                    .map(|reply| reply.trim())
                    .find(|reply| !reply.is_empty())
                    .map(ToOwned::to_owned)
                {
                    return Some(content);
                }

                latest_turn_assistant_output(session, turn_start_index)
            }

            fn output_comes_from_buffered_reply(
                execution_output: &str,
                buffered_replies: &[String],
                assistant_output: &str,
                session: &ChatSession,
                turn_start_index: usize,
            ) -> bool {
                execution_output.trim().is_empty()
                    && latest_turn_assistant_output(session, turn_start_index).is_none()
                    && buffered_replies
                        .iter()
                        .rev()
                        .map(|reply| reply.trim())
                        .find(|reply| !reply.is_empty())
                        == Some(assistant_output.trim())
            }

            fn promote_matching_progress_to_assistant(
                session: &mut ChatSession,
                turn_id: &str,
                assistant_output: &str,
            ) -> bool {
                let Some(turn) = session.turns.iter_mut().find(|turn| turn.id == turn_id) else {
                    return false;
                };
                let Some(event) = turn.events.iter_mut().rev().find(|event| {
                    matches!(
                        &event.kind,
                        ChatTurnEventKind::Progress { message }
                            if message.trim() == assistant_output.trim()
                    )
                }) else {
                    return false;
                };
                event.kind = ChatTurnEventKind::AssistantMessage {
                    content: assistant_output.trim().to_string(),
                };
                true
            }

            fn latest_turn_assistant_matches(
                session: &ChatSession,
                turn_start_index: usize,
                assistant_output: &str,
            ) -> bool {
                let trimmed = assistant_output.trim();
                !trimmed.is_empty()
                    && latest_turn_assistant_output(session, turn_start_index).as_deref()
                        == Some(trimmed)
            }

            pub(super) fn complete_turn_after_execution(
                session: &mut ChatSession,
                turn_id: &str,
                assistant_output: &str,
                has_text_streamed: &AtomicBool,
            ) {
                if has_text_streamed.load(Ordering::Relaxed) {
                    session.complete_turn(turn_id);
                } else {
                    session.complete_turn_with_assistant_message(turn_id, assistant_output);
                }
            }

            pub(super) async fn execute_chat_session(
                core: &Arc<AppCore>,
                request: ExecuteChatSessionRequest,
            ) -> std::result::Result<ChatSession, ExecuteChatSessionError> {
                let ExecuteChatSessionRequest {
                    session_id,
                    user_input,
                    turn_id,
                    workspace_root,
                    ack_frame_tx,
                    emitter,
                    steer_rx,
                    has_text_streamed: shared_text_streamed,
                } = request;
                let mut session = load_chat_session_for_execution(core, &session_id)?;

                let explicit_user_input = user_input.as_deref();
                let input = match explicit_user_input {
                    Some(input) if !input.trim().is_empty() => input.to_string(),
                    _ => session
                        .messages
                        .iter()
                        .rev()
                        .find(|msg| msg.role == ChatRole::User)
                        .map(|msg| msg.content.clone())
                        .ok_or(ExecuteChatSessionError::MissingUserMessage)?,
                };
                let persisted_input = input.clone();
                let agent_input = input.clone();

                if explicit_user_input.is_some() {
                    persist_ipc_user_message_if_needed(
                        core,
                        &mut session,
                        explicit_user_input,
                        &persisted_input,
                    )?;
                }
                if !record_turn_user_message_in_session_store(
                    core,
                    &mut session,
                    &turn_id,
                    &persisted_input,
                )? {
                    return Err(ExecuteChatSessionError::Canceled);
                }

                let turn_start_index = session.messages.len();
                let reply_buffer = Arc::new(Mutex::new(VecDeque::<String>::new()));
                let has_assistant_delta_streamed =
                    shared_text_streamed.unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
                let reply_sender = Arc::new(SessionReplySender::new(
                    core.clone(),
                    session_id.clone(),
                    turn_id.clone(),
                    reply_buffer.clone(),
                    ack_frame_tx.clone(),
                ));
                let executor = create_chat_executor(core).with_reply_sender(reply_sender);
                let chat_max_session_history = load_chat_max_session_history_from_core(core);

                let orchestrator = AgentOrchestratorImpl::from_runtime_executor(executor);
                let original_session_agent_id = session.agent_id.clone();
                orchestrator
                    .resolve_session_agent_for_execution(&mut session)
                    .map_err(ExecuteChatSessionError::Internal)?;
                let agent_id_repair =
                    agent_id_repair(&original_session_agent_id, session.agent_id.as_str());
                let execution_expected_provider = session.provider.clone();
                let execution_expected_model = session.model.clone();
                if let Some(repair) = agent_id_repair.as_ref() {
                    SessionService::from_storage(&core.storage)
                        .save_existing_session_with_agent_id_override(
                            &session,
                            "ipc",
                            repair.expected_existing_agent_id.as_str(),
                        )
                        .map_err(ExecuteChatSessionError::Internal)?;
                }
                let traced_execution = match orchestrator
                    .run_traced_interactive_session_turn(InteractiveSessionRequest {
                        session: &mut session,
                        user_input: &agent_input,
                        max_history: chat_max_session_history,
                        input_mode: SessionInputMode::PersistedInSession,
                        run_id: turn_id.clone(),
                        timeout_secs: None,
                        emitter,
                        steer_rx,
                        stream_display_mode: StreamDisplayMode::Streaming,
                        workspace_root: workspace_root.map(std::path::PathBuf::from),
                    })
                    .await
                {
                    Ok(execution) => execution,
                    Err(error) => {
                        let message = error.to_string();
                        if let Err(persist_error) = fail_turn_in_session_store(
                            core,
                            &session_id,
                            &turn_id,
                            &message,
                            agent_id_repair.clone(),
                        ) {
                            warn!(
                                session_id = %session_id,
                                turn_id = %turn_id,
                                error = %persist_error,
                                "Failed to persist failed chat turn"
                            );
                        }
                        return Err(anyhow::Error::new(error).into());
                    }
                };
                let duration_ms = traced_execution.duration_ms;
                let exec_result = traced_execution.execution;
                let original_persisted_input = persisted_input.clone();
                let (execution, final_persisted_input) = build_turn_persistence_payload(
                    &original_persisted_input,
                    duration_ms,
                    exec_result.iterations,
                );

                let buffered_replies = {
                    let mut guard = reply_buffer.lock().await;
                    std::mem::take(&mut *guard)
                };
                let buffered_replies = buffered_replies
                    .into_iter()
                    .filter(|reply| !reply.trim().is_empty())
                    .collect::<Vec<_>>();
                sync_session_view_from_session_store(core, &mut session)?;
                let assistant_output = select_final_assistant_output(
                    &exec_result.output,
                    &buffered_replies,
                    &session,
                    turn_start_index,
                )
                .ok_or_else(|| {
                    let _ = fail_turn_in_session_store(
                        core,
                        &session_id,
                        &turn_id,
                        "Interactive execution completed without assistant output",
                        agent_id_repair.clone(),
                    );
                    ExecuteChatSessionError::EmptyAssistantOutput
                })?;
                sync_turns_from_session_store(core, &mut session)?;
                let streamed_assistant_delta = has_assistant_delta_streamed.load(Ordering::Relaxed);
                let assistant_output_from_buffered_reply = output_comes_from_buffered_reply(
                    &exec_result.output,
                    &buffered_replies,
                    &assistant_output,
                    &session,
                    turn_start_index,
                );
                let promoted_buffered_reply = assistant_output_from_buffered_reply
                    && promote_matching_progress_to_assistant(
                        &mut session,
                        &turn_id,
                        &assistant_output,
                    );
                if promoted_buffered_reply {
                    session.complete_turn(&turn_id);
                } else {
                    complete_turn_after_execution(
                        &mut session,
                        &turn_id,
                        &assistant_output,
                        &has_assistant_delta_streamed,
                    );
                }
                if !streamed_assistant_delta
                    && latest_turn_assistant_matches(&session, turn_start_index, &assistant_output)
                {
                    if let Some(message) = session.messages.last_mut() {
                        message.execution = Some(execution);
                    }
                    session.set_model_identity(exec_result.final_model);
                    let runtime_guard = runtime_identity_guard(
                        &execution_expected_provider,
                        &execution_expected_model,
                        agent_id_repair.as_ref(),
                    );
                    let Some(persisted) = SessionService::from_storage(&core.storage)
                        .save_existing_session_with_runtime_identity_if_turn_non_terminal_returning_session(
                            &session,
                            &turn_id,
                            "ipc",
                            runtime_guard,
                        )?
                    else {
                        return Err(ExecuteChatSessionError::Canceled);
                    };
                    session = persisted;
                } else {
                    let runtime_guard = runtime_identity_guard(
                        &execution_expected_provider,
                        &execution_expected_model,
                        agent_id_repair.as_ref(),
                    );
                    let Some(persisted) = SessionService::from_storage(&core.storage)
                        .persist_interactive_turn_with_runtime_identity_if_turn_non_terminal_returning_session(
                            &mut session,
                            &turn_id,
                            PersistInteractiveTurnRequest::new(
                                &original_persisted_input,
                                &final_persisted_input,
                                &assistant_output,
                                execution,
                                "ipc",
                            )
                            .with_active_model(Some(&exec_result.active_model))
                            .with_final_model(Some(exec_result.final_model)),
                            runtime_guard,
                        )?
                    else {
                        return Err(ExecuteChatSessionError::Canceled);
                    };
                    session = persisted;
                }
                Ok(session)
            }

            pub(super) fn record_turn_event_in_session_store(
                core: &Arc<AppCore>,
                session_id: &str,
                turn_id: &str,
                event: ChatTurnEventKind,
            ) -> Result<bool> {
                core.storage
                    .file_sessions
                    .append_turn_event_if_non_terminal(session_id, turn_id, event)
            }

            pub(super) fn record_turn_user_message_in_session_store(
                core: &Arc<AppCore>,
                session: &mut ChatSession,
                turn_id: &str,
                content: &str,
            ) -> Result<bool> {
                let recorded = core
                    .storage
                    .file_sessions
                    .record_turn_user_message_if_non_terminal(&session.id, turn_id, content)?;
                if recorded {
                    sync_session_view_from_session_store(core, session)?;
                }
                Ok(recorded)
            }

            fn sync_turns_from_session_store(
                core: &Arc<AppCore>,
                session: &mut ChatSession,
            ) -> Result<()> {
                if let Some(stored) =
                    SessionService::from_storage(&core.storage).get_session_view(&session.id)?
                {
                    session.turns = stored.turns;
                }
                Ok(())
            }

            pub(super) fn sync_session_view_from_session_store(
                core: &Arc<AppCore>,
                session: &mut ChatSession,
            ) -> Result<()> {
                let execution_agent_id = session.agent_id.clone();
                let stored = SessionService::from_storage(&core.storage)
                    .get_session_view(&session.id)?
                    .ok_or_else(|| {
                        anyhow::anyhow!(types::session_not_found_message(&session.id))
                    })?;
                session.name = stored.name;
                session.agent_id = execution_agent_id;
                session.provider = stored.provider;
                session.model = stored.model;
                session.messages = stored.messages;
                session.turns = stored.turns;
                session.created_at = stored.created_at;
                session.updated_at = stored.updated_at;
                session.skill_id = stored.skill_id;
                session.retention = stored.retention;
                session.summary_message_id = stored.summary_message_id;
                session.prompt_tokens = stored.prompt_tokens;
                session.completion_tokens = stored.completion_tokens;
                session.cost = stored.cost;
                session.metadata = stored.metadata;
                session.archived_at = stored.archived_at;
                Ok(())
            }

            fn agent_id_repair(
                expected_existing_agent_id: &str,
                repaired_agent_id: &str,
            ) -> Option<SessionAgentIdRepair> {
                if repaired_agent_id.trim().is_empty()
                    || repaired_agent_id == expected_existing_agent_id
                {
                    return None;
                }
                Some(SessionAgentIdRepair {
                    expected_existing_agent_id: expected_existing_agent_id.to_string(),
                    repaired_agent_id: repaired_agent_id.to_string(),
                })
            }

            fn runtime_identity_guard<'a>(
                expected_existing_provider: &'a str,
                expected_existing_model: &'a str,
                agent_id_repair: Option<&'a SessionAgentIdRepair>,
            ) -> RuntimeIdentityGuard<'a> {
                let guard =
                    RuntimeIdentityGuard::new(expected_existing_provider, expected_existing_model);
                if let Some(repair) = agent_id_repair {
                    guard.with_agent_id_override(repair.expected_existing_agent_id.as_str())
                } else {
                    guard
                }
            }

            pub(super) fn fail_turn_in_session_store(
                core: &Arc<AppCore>,
                session_id: &str,
                turn_id: &str,
                message: &str,
                agent_id_repair: Option<SessionAgentIdRepair>,
            ) -> Result<()> {
                let session_service = SessionService::from_storage(&core.storage);
                let Some(mut session) = session_service.get_session_view(session_id)? else {
                    anyhow::bail!(types::session_not_found_message(session_id));
                };
                if let Some(repair) = agent_id_repair.as_ref() {
                    session.agent_id = repair.repaired_agent_id.clone();
                }
                session.fail_turn(turn_id, message);
                if let Some(repair) = agent_id_repair {
                    session_service.save_existing_session_with_agent_id_override(
                        &session,
                        "ipc",
                        repair.expected_existing_agent_id.as_str(),
                    )?;
                } else {
                    session_service.save_existing_session(&session, "ipc")?;
                }
                Ok(())
            }

            pub(super) fn cancel_orphaned_running_turns_in_session_store(
                core: &Arc<AppCore>,
                session_id: &str,
                current_turn_id: &str,
            ) -> Result<usize> {
                let session_service = SessionService::from_storage(&core.storage);
                let Some(mut session) = session_service.get_session_view(session_id)? else {
                    return Ok(0);
                };
                let running_turn_ids = session
                    .turns
                    .iter()
                    .filter(|turn| {
                        turn.id != current_turn_id && turn.status == types::ChatTurnStatus::Running
                    })
                    .map(|turn| turn.id.clone())
                    .collect::<Vec<_>>();
                if running_turn_ids.is_empty() {
                    return Ok(0);
                }
                for turn_id in &running_turn_ids {
                    session.cancel_turn(turn_id);
                }
                session_service.save_existing_session(&session, "ipc")?;
                Ok(running_turn_ids.len())
            }

            pub(super) fn cancel_pending_turn_in_session_store(
                core: &Arc<AppCore>,
                session_id: &str,
                turn_id: &str,
                user_input: Option<&str>,
            ) -> Result<()> {
                let session_service = SessionService::from_storage(&core.storage);
                let Some(mut session) = session_service.get_session_view(session_id)? else {
                    anyhow::bail!(types::session_not_found_message(session_id));
                };
                if session
                    .turns
                    .iter()
                    .find(|turn| turn.id == turn_id)
                    .is_some_and(|turn| turn.status.is_terminal())
                {
                    return Ok(());
                }
                if let Some(input) = user_input
                    && !input.trim().is_empty()
                {
                    let already_persisted = session
                        .messages
                        .last()
                        .map(|message| message.role == ChatRole::User && message.content == input)
                        .unwrap_or(false);
                    if !already_persisted {
                        session.add_message(ChatMessage::user(input));
                        if session.name == "New Chat" && session.messages.len() == 1 {
                            session.auto_name_from_first_message();
                        }
                    }
                    session.record_turn_user_message(turn_id, input);
                }
                session.cancel_turn(turn_id);
                session_service.save_existing_session(&session, "ipc")?;
                Ok(())
            }

            fn load_chat_session_for_execution(
                core: &Arc<AppCore>,
                session_id: &str,
            ) -> std::result::Result<ChatSession, ExecuteChatSessionError> {
                let Some(session) =
                    SessionService::from_storage(&core.storage).get_session_view(session_id)?
                else {
                    return Err(ExecuteChatSessionError::SessionNotFound);
                };
                Ok(session)
            }

            pub(super) fn persist_ipc_user_message_if_needed(
                core: &Arc<AppCore>,
                session: &mut ChatSession,
                explicit_user_input: Option<&str>,
                persisted_input: &str,
            ) -> Result<()> {
                let Some(raw_input) = explicit_user_input.map(str::trim) else {
                    return Ok(());
                };
                if raw_input.is_empty() {
                    return Ok(());
                }

                let already_persisted = session
                    .messages
                    .last()
                    .map(|message| {
                        message.role == ChatRole::User && message.content == persisted_input
                    })
                    .unwrap_or(false);
                if already_persisted {
                    return Ok(());
                }

                session.add_message(ChatMessage::user(persisted_input));
                if session.name == "New Chat" && session.messages.len() == 1 {
                    session.auto_name_from_first_message();
                }
                SessionService::from_storage(&core.storage)
                    .save_existing_session(session, "ipc")?;
                Ok(())
            }

            pub(super) fn resolve_agent_id(
                core: &Arc<AppCore>,
                agent_id: Option<String>,
            ) -> Result<String> {
                if let Some(agent_id) = agent_id {
                    return core.storage.agents.resolve_existing_agent_id(&agent_id);
                }

                core.storage.agents.resolve_default_agent_id()
            }

            pub(super) fn build_agent_system_prompt(
                core: &Arc<AppCore>,
                agent_node: AgentNode,
            ) -> Result<String> {
                crate::runtime::build_agent_system_prompt(core.storage.clone(), &agent_node, None)
            }

            #[cfg(test)]
            mod tests {
                use super::{
                    latest_assistant_payload, latest_turn_assistant_matches,
                    output_comes_from_buffered_reply, promote_matching_progress_to_assistant,
                    select_final_assistant_output,
                };
                use types::{ChatMessage, ChatSession, ChatTurnEventKind};

                #[test]
                fn final_output_prefers_non_empty_execution_output() {
                    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                    session.add_message(ChatMessage::assistant("buffered reply"));

                    let output = select_final_assistant_output(
                        "final answer",
                        &[],
                        &session,
                        session.messages.len(),
                    );

                    assert_eq!(output.as_deref(), Some("final answer"));
                }

                #[test]
                fn final_output_uses_latest_non_empty_buffered_reply_when_execution_output_is_blank()
                 {
                    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                    session.add_message(ChatMessage::assistant("older reply"));

                    let output = select_final_assistant_output(
                        "   ",
                        &["".to_string(), "ack reply".to_string()],
                        &session,
                        session.messages.len(),
                    );

                    assert_eq!(output.as_deref(), Some("ack reply"));
                }

                #[test]
                fn final_output_uses_current_turn_assistant_when_execution_output_is_blank() {
                    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                    session.add_message(ChatMessage::assistant("previous turn"));
                    let turn_start_index = session.messages.len();
                    session.add_message(ChatMessage::assistant("current turn"));

                    let output = select_final_assistant_output("", &[], &session, turn_start_index);

                    assert_eq!(output.as_deref(), Some("current turn"));
                }

                #[test]
                fn final_output_is_missing_when_no_non_empty_assistant_text_exists() {
                    let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());

                    let output = select_final_assistant_output("", &[], &session, 0);

                    assert!(output.is_none());
                }

                #[test]
                fn latest_turn_assistant_match_requires_matching_current_turn_assistant_message() {
                    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                    session.add_message(ChatMessage::assistant("previous turn"));
                    let turn_start_index = session.messages.len();
                    session.add_message(ChatMessage::assistant("ack reply"));

                    assert!(latest_turn_assistant_matches(
                        &session,
                        turn_start_index,
                        "ack reply"
                    ));
                    assert!(!latest_turn_assistant_matches(
                        &session,
                        turn_start_index,
                        "something else"
                    ));
                    assert!(!latest_turn_assistant_matches(
                        &session,
                        turn_start_index,
                        "previous turn"
                    ));
                }

                #[test]
                fn buffered_reply_source_can_promote_progress_to_assistant() {
                    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                    session.record_turn_user_message("turn-1", "hello");
                    session.record_turn_event(
                        "turn-1",
                        ChatTurnEventKind::Progress {
                            message: "ack reply".to_string(),
                        },
                    );

                    assert!(output_comes_from_buffered_reply(
                        "",
                        &["ack reply".to_string()],
                        "ack reply",
                        &session,
                        session.messages.len()
                    ));
                    assert!(promote_matching_progress_to_assistant(
                        &mut session,
                        "turn-1",
                        "ack reply"
                    ));

                    let turn = session
                        .turns
                        .iter()
                        .find(|turn| turn.id == "turn-1")
                        .expect("turn");
                    assert!(turn.events.iter().any(|event| {
                        matches!(
                            &event.kind,
                            ChatTurnEventKind::AssistantMessage { content }
                                if content == "ack reply"
                        )
                    }));
                    assert!(
                        !turn.events.iter().any(|event| {
                            matches!(event.kind, ChatTurnEventKind::Progress { .. })
                        })
                    );
                }

                #[test]
                fn latest_assistant_payload_skips_empty_assistant_messages() {
                    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                    session.add_message(ChatMessage::assistant("visible"));
                    session.add_message(ChatMessage::assistant("   "));

                    let payload = latest_assistant_payload(&session);

                    assert_eq!(
                        payload.as_ref().map(|(content, _)| content.as_str()),
                        Some("visible")
                    );
                }
            }
        }

        use self::runtime::{
            ExecuteChatSessionError, ExecuteChatSessionRequest, execute_chat_session,
            latest_assistant_payload, record_turn_event_in_session_store,
        };

        #[cfg(unix)]
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        #[cfg(unix)]
        use tokio::net::{UnixListener, UnixStream};

        pub struct IpcServer {
            core: Arc<AppCore>,
            socket_path: PathBuf,
            runtime_tool_registry: Arc<OnceLock<types::toolset::ToolRegistry>>,
        }

        fn active_chat_streams() -> &'static Mutex<HashMap<String, JoinHandle<()>>> {
            static STREAMS: OnceLock<Mutex<HashMap<String, JoinHandle<()>>>> = OnceLock::new();
            STREAMS.get_or_init(|| Mutex::new(HashMap::new()))
        }

        fn completed_chat_streams() -> &'static Mutex<HashSet<String>> {
            static COMPLETED_STREAMS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
            COMPLETED_STREAMS.get_or_init(|| Mutex::new(HashSet::new()))
        }

        const PENDING_CANCELED_CHAT_STREAM_TTL: Duration = Duration::from_secs(60);

        #[derive(Debug, Clone, PartialEq, Eq)]
        struct PendingCanceledChatStream {
            session_id: String,
            scope: Option<ExecutionScope>,
            expires_at: Instant,
        }

        impl PendingCanceledChatStream {
            fn new(session_id: &str, scope: Option<ExecutionScope>) -> Self {
                Self {
                    session_id: session_id.to_string(),
                    scope,
                    expires_at: Instant::now() + PENDING_CANCELED_CHAT_STREAM_TTL,
                }
            }

            fn is_expired(&self, now: Instant) -> bool {
                self.expires_at <= now
            }
        }

        fn pending_canceled_chat_streams()
        -> &'static Mutex<HashMap<String, PendingCanceledChatStream>> {
            static PENDING_CANCELED_STREAMS: OnceLock<
                Mutex<HashMap<String, PendingCanceledChatStream>>,
            > = OnceLock::new();
            PENDING_CANCELED_STREAMS.get_or_init(|| Mutex::new(HashMap::new()))
        }

        async fn take_pending_canceled_stream(
            stream_id: &str,
            session_id: &str,
            scope: &Option<ExecutionScope>,
        ) -> bool {
            let mut pending = pending_canceled_chat_streams().lock().await;
            purge_expired_pending_canceled_streams(&mut pending);
            if pending.get(stream_id).is_some_and(|canceled| {
                canceled.session_id == session_id && canceled.scope.as_ref() == scope.as_ref()
            }) {
                pending.remove(stream_id);
                return true;
            }
            false
        }

        fn purge_expired_pending_canceled_streams(
            pending: &mut HashMap<String, PendingCanceledChatStream>,
        ) {
            let now = Instant::now();
            pending.retain(|_, canceled| !canceled.is_expired(now));
        }

        #[derive(Debug, Clone, PartialEq, Eq)]
        struct ActiveChatStreamBinding {
            stream_id: String,
            turn_id: String,
            scope: Option<ExecutionScope>,
            user_input: Option<String>,
        }

        impl ActiveChatStreamBinding {
            fn new(
                stream_id: impl Into<String>,
                turn_id: impl Into<String>,
                scope: Option<ExecutionScope>,
            ) -> Self {
                Self {
                    stream_id: stream_id.into(),
                    turn_id: turn_id.into(),
                    scope,
                    user_input: None,
                }
            }

            fn with_user_input(mut self, user_input: Option<String>) -> Self {
                self.user_input = user_input;
                self
            }

            fn same_owner(&self, scope: &Option<ExecutionScope>) -> bool {
                self.scope == *scope
            }
        }

        fn active_chat_stream_sessions() -> &'static Mutex<HashMap<String, ActiveChatStreamBinding>>
        {
            static SESSION_STREAMS: OnceLock<Mutex<HashMap<String, ActiveChatStreamBinding>>> =
                OnceLock::new();
            SESSION_STREAMS.get_or_init(|| Mutex::new(HashMap::new()))
        }

        fn active_chat_stream_steers() -> &'static Mutex<HashMap<String, mpsc::Sender<SteerMessage>>>
        {
            static STEERS: OnceLock<Mutex<HashMap<String, mpsc::Sender<SteerMessage>>>> =
                OnceLock::new();
            STEERS.get_or_init(|| Mutex::new(HashMap::new()))
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        struct SessionEventSnapshot {
            updated_at: i64,
            message_count: u32,
            event_count: usize,
            event_digest: u64,
        }

        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        struct SessionEventEmissionKey {
            session_id: String,
            kind: &'static str,
            updated_at: Option<i64>,
            message_count: Option<u32>,
            event_count: Option<usize>,
            event_digest: Option<u64>,
        }

        #[derive(Debug, Default)]
        struct SessionEventDeduper {
            seen: HashSet<SessionEventEmissionKey>,
            order: VecDeque<SessionEventEmissionKey>,
        }

        impl SessionEventDeduper {
            const MAX_KEYS: usize = 512;

            fn should_send(
                &mut self,
                event: &ChatSessionEvent,
                snapshot: &HashMap<String, SessionEventSnapshot>,
            ) -> bool {
                let key = SessionEventEmissionKey::from_event(event, snapshot);
                if self.seen.contains(&key) {
                    return false;
                }
                self.seen.insert(key.clone());
                self.order.push_back(key);
                while self.order.len() > Self::MAX_KEYS {
                    if let Some(old) = self.order.pop_front() {
                        self.seen.remove(&old);
                    }
                }
                true
            }
        }

        impl SessionEventEmissionKey {
            fn from_event(
                event: &ChatSessionEvent,
                snapshot: &HashMap<String, SessionEventSnapshot>,
            ) -> Self {
                let (kind, session_id) = match event {
                    ChatSessionEvent::Created { session_id } => ("created", session_id),
                    ChatSessionEvent::Updated { session_id } => ("updated", session_id),
                    ChatSessionEvent::MessageAdded { session_id, .. } => {
                        ("message_added", session_id)
                    }
                    ChatSessionEvent::Deleted { session_id } => ("deleted", session_id),
                };
                let state = snapshot.get(session_id);
                Self {
                    session_id: session_id.clone(),
                    kind,
                    updated_at: state.map(|state| state.updated_at),
                    message_count: state.map(|state| state.message_count),
                    event_count: state.map(|state| state.event_count),
                    event_digest: state.map(|state| state.event_digest),
                }
            }
        }

        struct ForegroundSessionLease {
            file: File,
        }

        impl ForegroundSessionLease {
            fn acquire(session_id: &str, scope: &Option<ExecutionScope>) -> Result<Option<Self>> {
                if !matches!(scope, Some(ExecutionScope::Foreground { .. })) {
                    return Ok(None);
                }
                let dir = restflow_core::paths::ensure_restflow_dir()?.join("foreground-streams");
                std::fs::create_dir_all(&dir)?;
                let path = dir.join(format!("{}.lock", session_lock_file_stem(session_id)));
                let mut file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .truncate(false)
                    .open(&path)?;
                match file.try_lock_exclusive() {
                    Ok(()) => {
                        file.set_len(0)?;
                        writeln!(file, "pid={}", std::process::id())?;
                        writeln!(file, "session_id={session_id}")?;
                        writeln!(file, "scope={scope:?}")?;
                        Ok(Some(Self { file }))
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        anyhow::bail!(
                            "Session {session_id} already has an active foreground stream"
                        )
                    }
                    Err(error) => Err(error.into()),
                }
            }
        }

        impl Drop for ForegroundSessionLease {
            fn drop(&mut self) {
                let _ = self.file.unlock();
            }
        }

        fn session_lock_file_stem(session_id: &str) -> String {
            session_id
                .chars()
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                        ch
                    } else {
                        '_'
                    }
                })
                .collect()
        }

        pub async fn open_foreground_chat_session_stream(
            core: Arc<AppCore>,
            session_id: String,
            user_input: Option<String>,
            stream_id: String,
            workspace_root: Option<String>,
        ) -> Result<mpsc::UnboundedReceiver<StreamFrame>> {
            IpcServer::open_execute_chat_session_stream(
                core,
                session_id,
                user_input,
                stream_id,
                workspace_root,
                None,
            )
            .await
        }

        pub async fn open_foreground_chat_session_stream_for_scope(
            core: Arc<AppCore>,
            session_id: String,
            user_input: Option<String>,
            stream_id: String,
            workspace_root: Option<String>,
            scope: ExecutionScope,
        ) -> Result<mpsc::UnboundedReceiver<StreamFrame>> {
            IpcServer::open_execute_chat_session_stream(
                core,
                session_id,
                user_input,
                stream_id,
                workspace_root,
                Some(scope),
            )
            .await
        }

        pub async fn steer_foreground_chat_stream(
            core: &Arc<AppCore>,
            session_id: &str,
            instruction: &str,
        ) -> bool {
            runtime::steer_chat_stream(core, session_id, instruction, None)
                .await
                .unwrap_or(false)
        }

        pub async fn steer_foreground_chat_stream_for_scope(
            core: &Arc<AppCore>,
            session_id: &str,
            instruction: &str,
            scope: ExecutionScope,
        ) -> bool {
            runtime::steer_chat_stream(core, session_id, instruction, Some(&scope))
                .await
                .unwrap_or(false)
        }

        pub async fn cancel_foreground_chat_stream(core: &Arc<AppCore>, stream_id: &str) -> bool {
            let bound = {
                let session_streams = active_chat_stream_sessions().lock().await;
                session_streams
                    .iter()
                    .find(|(_, binding)| binding.stream_id == stream_id)
                    .map(|(session_id, binding)| (session_id.clone(), binding.scope.clone()))
            };

            if let Some((session_id, scope)) = bound {
                return runtime::cancel_chat_stream(
                    core,
                    stream_id,
                    Some(&session_id),
                    scope.as_ref(),
                )
                .await
                .unwrap_or(false);
            }

            runtime::cancel_chat_stream(core, stream_id, None, None)
                .await
                .unwrap_or(false)
        }

        pub async fn cancel_foreground_chat_stream_for_scope(
            core: &Arc<AppCore>,
            session_id: &str,
            stream_id: &str,
            scope: ExecutionScope,
        ) -> bool {
            runtime::cancel_chat_stream(core, stream_id, Some(session_id), Some(&scope))
                .await
                .unwrap_or(false)
        }

        fn daemon_started_at_ms() -> i64 {
            static STARTED_AT_MS: OnceLock<i64> = OnceLock::new();
            *STARTED_AT_MS.get_or_init(|| Utc::now().timestamp_millis())
        }

        pub(crate) fn build_daemon_status() -> IpcDaemonStatus {
            let started_at_ms = daemon_started_at_ms();
            let now_ms = Utc::now().timestamp_millis();
            let uptime_secs = ((now_ms - started_at_ms).max(0) / 1000) as u64;

            IpcDaemonStatus {
                status: types::DaemonRuntimeStatus::Running,
                protocol_version: IPC_PROTOCOL_VERSION.to_string(),
                daemon_version: env!("CARGO_PKG_VERSION").to_string(),
                pid: std::process::id(),
                started_at_ms,
                uptime_secs,
            }
        }

        struct IpcStreamEmitter {
            core: Arc<AppCore>,
            session_id: String,
            turn_id: String,
            tx: mpsc::UnboundedSender<StreamFrame>,
            has_assistant_delta_streamed: Arc<AtomicBool>,
            assistant_segment: String,
        }

        impl IpcStreamEmitter {
            fn new(
                core: Arc<AppCore>,
                session_id: String,
                turn_id: String,
                tx: mpsc::UnboundedSender<StreamFrame>,
                has_assistant_delta_streamed: Arc<AtomicBool>,
            ) -> Self {
                Self {
                    core,
                    session_id,
                    turn_id,
                    tx,
                    has_assistant_delta_streamed,
                    assistant_segment: String::new(),
                }
            }

            fn persist_assistant_segment(&mut self) -> bool {
                let content = self.assistant_segment.clone();
                if content.trim().is_empty() {
                    self.assistant_segment.clear();
                    return true;
                }
                match record_turn_event_in_session_store(
                    &self.core,
                    &self.session_id,
                    &self.turn_id,
                    ChatTurnEventKind::AssistantMessage { content },
                ) {
                    Ok(true) => {
                        self.assistant_segment.clear();
                        true
                    }
                    Ok(false) => {
                        self.assistant_segment.clear();
                        false
                    }
                    Err(error) => {
                        warn!(
                            session_id = %self.session_id,
                            turn_id = %self.turn_id,
                            error = %error,
                            "Failed to persist streamed assistant segment"
                        );
                        let _ = self.tx.send(StreamFrame::error(
                            500,
                            format!("Failed to persist streamed assistant segment: {error}"),
                        ));
                        false
                    }
                }
            }
        }

        impl Drop for IpcStreamEmitter {
            fn drop(&mut self) {
                self.persist_assistant_segment();
            }
        }

        struct SessionReplySender {
            core: Arc<AppCore>,
            session_id: String,
            turn_id: String,
            buffered_messages: Arc<Mutex<VecDeque<String>>>,
            stream_tx: Option<mpsc::UnboundedSender<StreamFrame>>,
        }

        impl SessionReplySender {
            fn new(
                core: Arc<AppCore>,
                session_id: String,
                turn_id: String,
                buffered_messages: Arc<Mutex<VecDeque<String>>>,
                stream_tx: Option<mpsc::UnboundedSender<StreamFrame>>,
            ) -> Self {
                Self {
                    core,
                    session_id,
                    turn_id,
                    buffered_messages,
                    stream_tx,
                }
            }
        }

        impl ReplySender for SessionReplySender {
            fn send(
                &self,
                message: String,
            ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
                let core = self.core.clone();
                let session_id = self.session_id.clone();
                let turn_id = self.turn_id.clone();
                let buffered_messages = self.buffered_messages.clone();
                let stream_tx = self.stream_tx.clone();

                Box::pin(async move {
                    if message.trim().is_empty() {
                        return Ok(());
                    }

                    let recorded = record_turn_event_in_session_store(
                        &core,
                        &session_id,
                        &turn_id,
                        ChatTurnEventKind::Progress {
                            message: message.clone(),
                        },
                    )?;
                    if !recorded {
                        return Ok(());
                    }

                    buffered_messages.lock().await.push_back(message.clone());

                    if let Some(tx) = stream_tx {
                        let _ = tx.send(StreamFrame::Progress {
                            content: message.clone(),
                        });
                    }

                    Ok(())
                })
            }
        }

        fn parse_tool_arguments(arguments: &str) -> serde_json::Value {
            if arguments.trim().is_empty() {
                return serde_json::Value::Null;
            }
            match serde_json::from_str::<serde_json::Value>(arguments) {
                Ok(value) => value,
                Err(_) => serde_json::Value::String(arguments.to_string()),
            }
        }

        #[cfg(test)]
        fn normalize_model_input(model: &str) -> Result<String> {
            types::ModelId::normalize_unambiguous_model_id(model).map_err(anyhow::Error::msg)
        }

        fn ipc_session_lifecycle_error(error: anyhow::Error) -> IpcResponse {
            let message = error.to_string();
            let code = if is_client_session_lifecycle_error(&message) {
                400
            } else {
                500
            };
            IpcResponse::error(code, message)
        }

        fn is_client_session_lifecycle_error(message: &str) -> bool {
            message.starts_with("Unknown provider:")
                || message.starts_with("Unknown model:")
                || message.starts_with("Ambiguous model identifier")
                || message.starts_with("Unsupported model identifier:")
        }

        fn stream_open_error_frame(error: anyhow::Error) -> StreamFrame {
            let message = error.to_string();
            let code = if message == "Foreground chat streaming runs in the TUI process"
                || message.starts_with("Unsupported streaming request:")
            {
                -3
            } else {
                500
            };
            StreamFrame::error(code, message)
        }

        #[async_trait]
        impl StreamEmitter for IpcStreamEmitter {
            async fn emit_text_delta(&mut self, text: &str) {
                if text.is_empty() {
                    return;
                }
                self.assistant_segment.push_str(text);
                if self.assistant_segment.trim().is_empty() {
                    return;
                }
                let content = self.assistant_segment.clone();
                if self.persist_assistant_segment() {
                    self.has_assistant_delta_streamed
                        .store(true, Ordering::Relaxed);
                    let _ = self.tx.send(StreamFrame::Data { content });
                }
            }

            async fn emit_thinking_delta(&mut self, _text: &str) {}

            async fn emit_tool_call_start(&mut self, id: &str, name: &str, arguments: &str) {
                if !self.persist_assistant_segment() {
                    return;
                }
                match record_turn_event_in_session_store(
                    &self.core,
                    &self.session_id,
                    &self.turn_id,
                    ChatTurnEventKind::ToolCall {
                        call_id: id.to_string(),
                        name: name.to_string(),
                        arguments: arguments.to_string(),
                    },
                ) {
                    Ok(true) => {}
                    Ok(false) => return,
                    Err(error) => {
                        warn!(
                            session_id = %self.session_id,
                            turn_id = %self.turn_id,
                            call_id = %id,
                            error = %error,
                            "Failed to persist turn tool call event"
                        );
                        let _ = self.tx.send(StreamFrame::error(
                            500,
                            format!("Failed to persist turn tool call event: {error}"),
                        ));
                        return;
                    }
                }
                let _ = self.tx.send(StreamFrame::ToolCall {
                    id: id.to_string(),
                    name: name.to_string(),
                    arguments: parse_tool_arguments(arguments),
                });
            }

            async fn emit_tool_call_result(
                &mut self,
                id: &str,
                _name: &str,
                result: &str,
                success: bool,
            ) {
                match record_turn_event_in_session_store(
                    &self.core,
                    &self.session_id,
                    &self.turn_id,
                    ChatTurnEventKind::ToolResult {
                        call_id: id.to_string(),
                        success,
                        result: result.to_string(),
                    },
                ) {
                    Ok(true) => {}
                    Ok(false) => return,
                    Err(error) => {
                        warn!(
                            session_id = %self.session_id,
                            turn_id = %self.turn_id,
                            call_id = %id,
                            error = %error,
                            "Failed to persist turn tool result event"
                        );
                        let _ = self.tx.send(StreamFrame::error(
                            500,
                            format!("Failed to persist turn tool result event: {error}"),
                        ));
                        return;
                    }
                }
                let _ = self.tx.send(StreamFrame::ToolResult {
                    id: id.to_string(),
                    result: result.to_string(),
                    success,
                });
            }

            async fn emit_complete(&mut self) {
                self.persist_assistant_segment();
            }
        }

        impl IpcServer {
            pub fn new(core: Arc<AppCore>, socket_path: PathBuf) -> Self {
                Self {
                    core,
                    socket_path,
                    runtime_tool_registry: Arc::new(OnceLock::new()),
                }
            }

            #[cfg(unix)]
            pub async fn run(&self, mut shutdown: broadcast::Receiver<()>) -> Result<()> {
                if self.socket_path.exists() {
                    std::fs::remove_file(&self.socket_path)?;
                }
                let listener = UnixListener::bind(&self.socket_path)?;

                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(
                        &self.socket_path,
                        std::fs::Permissions::from_mode(0o600),
                    )?;
                }

                info!(path = %self.socket_path.display(), "IPC server started");

                loop {
                    tokio::select! {
                        result = listener.accept() => {
                            match result {
                                Ok((stream, _)) => {
                                    let core = self.core.clone();
                                    let runtime_tool_registry = self.runtime_tool_registry.clone();
                                    tokio::spawn(async move {
                                        if let Err(err) =
                                            Self::handle_client(stream, core, runtime_tool_registry).await
                                        {
                                            debug!(error = %err, "Client disconnected");
                                        }
                                    });
                                }
                                Err(err) => error!(error = %err, "IPC accept error"),
                            }
                        }
                        _ = shutdown.recv() => {
                            info!("IPC server shutting down");
                            break;
                        }
                    }
                }

                let _ = std::fs::remove_file(&self.socket_path);
                Ok(())
            }

            #[cfg(not(unix))]
            pub async fn run(&self, _shutdown: broadcast::Receiver<()>) -> Result<()> {
                anyhow::bail!("IPC is not supported on this platform")
            }

            #[cfg(unix)]
            async fn handle_client(
                mut stream: UnixStream,
                core: Arc<AppCore>,
                runtime_tool_registry: Arc<OnceLock<types::toolset::ToolRegistry>>,
            ) -> Result<()> {
                loop {
                    let mut len_buf = [0u8; 4];
                    if stream.read_exact(&mut len_buf).await.is_err() {
                        break;
                    }
                    let len = u32::from_le_bytes(len_buf) as usize;
                    if len > MAX_MESSAGE_SIZE {
                        Self::send(&mut stream, &IpcResponse::error(-1, "Message too large"))
                            .await?;
                        continue;
                    }

                    let mut buf = vec![0u8; len];
                    stream.read_exact(&mut buf).await?;

                    match serde_json::from_slice::<IpcRequest>(&buf) {
                        Ok(request @ IpcRequest::SubscribeSessionEvents) => {
                            match Self::open_stream(core.clone(), request).await {
                                Ok(mut rx) => {
                                    while let Some(frame) = rx.recv().await {
                                        if let Err(err) =
                                            Self::send_stream_frame(&mut stream, &frame).await
                                        {
                                            debug!(error = %err, "Stream client disconnected");
                                            break;
                                        }
                                    }
                                }
                                Err(err) => {
                                    let frame = stream_open_error_frame(err);
                                    let _ = Self::send_stream_frame(&mut stream, &frame).await;
                                }
                            }
                        }
                        Ok(req) => {
                            let response =
                                Self::process(&core, runtime_tool_registry.as_ref(), req).await;
                            Self::send(&mut stream, &response).await?;
                        }
                        Err(err) => {
                            let response =
                                IpcResponse::error(-2, format!("Invalid request: {}", err));
                            Self::send(&mut stream, &response).await?;
                        }
                    }
                }
                Ok(())
            }

            #[cfg(unix)]
            async fn send(stream: &mut UnixStream, response: &IpcResponse) -> Result<()> {
                let json = serde_json::to_vec(response)?;
                stream.write_all(&(json.len() as u32).to_le_bytes()).await?;
                stream.write_all(&json).await?;
                Ok(())
            }

            #[cfg(unix)]
            async fn send_stream_frame(stream: &mut UnixStream, frame: &StreamFrame) -> Result<()> {
                let json = serde_json::to_vec(frame)?;
                stream.write_all(&(json.len() as u32).to_le_bytes()).await?;
                stream.write_all(&json).await?;
                Ok(())
            }

            pub(crate) async fn open_stream(
                core: Arc<AppCore>,
                request: IpcRequest,
            ) -> Result<mpsc::UnboundedReceiver<StreamFrame>> {
                match request {
                    IpcRequest::SubscribeSessionEvents => {
                        Self::open_session_event_stream(core).await
                    }
                    other => anyhow::bail!("Unsupported streaming request: {:?}", other),
                }
            }

            async fn open_execute_chat_session_stream(
                core: Arc<AppCore>,
                session_id: String,
                user_input: Option<String>,
                stream_id: String,
                workspace_root: Option<String>,
                scope: Option<ExecutionScope>,
            ) -> Result<mpsc::UnboundedReceiver<StreamFrame>> {
                let stream_id = if stream_id.trim().is_empty() {
                    Uuid::new_v4().to_string()
                } else {
                    stream_id
                };

                if take_pending_canceled_stream(&stream_id, &session_id, &scope).await {
                    runtime::cancel_pending_turn_in_session_store(
                        &core,
                        &session_id,
                        &stream_id,
                        user_input.as_deref(),
                    )?;
                    let (tx, rx) = mpsc::unbounded_channel::<StreamFrame>();
                    let _ = tx.send(StreamFrame::Start {
                        stream_id: stream_id.clone(),
                    });
                    let _ = tx.send(StreamFrame::Done { total_tokens: None });
                    return Ok(rx);
                }

                completed_chat_streams().lock().await.remove(&stream_id);

                {
                    let session_streams = active_chat_stream_sessions().lock().await;
                    if let Some((owner_session_id, existing)) = session_streams
                        .iter()
                        .find(|(_, binding)| binding.stream_id == stream_id)
                        && !existing.same_owner(&scope)
                    {
                        anyhow::bail!(
                            "Stream {stream_id} for session {owner_session_id} is owned by another client"
                        );
                    }
                    if let Some(existing) = session_streams.get(&session_id)
                        && existing.stream_id != stream_id
                        && !existing.same_owner(&scope)
                    {
                        anyhow::bail!(
                            "Session {session_id} already has an active stream owned by another client"
                        );
                    }
                }

                if active_chat_streams().lock().await.contains_key(&stream_id) {
                    anyhow::bail!("Stream {stream_id} is already active");
                }

                // Keep foreground streams scoped to their terminal owner. A second TUI on
                // the same session should not silently abort the first TUI's active turn.
                let previous_binding = {
                    let session_streams = active_chat_stream_sessions().lock().await;
                    match session_streams.get(&session_id) {
                        Some(existing)
                            if existing.stream_id != stream_id && !existing.same_owner(&scope) =>
                        {
                            anyhow::bail!(
                                "Session {session_id} already has an active stream owned by another client"
                            );
                        }
                        existing => existing.cloned(),
                    }
                };
                if let Some(previous_binding) = previous_binding
                    && previous_binding.stream_id != stream_id
                {
                    runtime::cancel_pending_turn_in_session_store(
                        &core,
                        &session_id,
                        &previous_binding.stream_id,
                        previous_binding.user_input.as_deref(),
                    )?;
                    if let Some(previous) = active_chat_streams()
                        .lock()
                        .await
                        .remove(&previous_binding.stream_id)
                    {
                        previous.abort();
                        let _ = previous.await;
                        completed_chat_streams()
                            .lock()
                            .await
                            .remove(&previous_binding.stream_id);
                    }
                    active_chat_stream_steers()
                        .lock()
                        .await
                        .remove(&previous_binding.stream_id);
                }

                let foreground_lease = ForegroundSessionLease::acquire(&session_id, &scope)?;
                if foreground_lease.is_some() {
                    runtime::cancel_orphaned_running_turns_in_session_store(
                        &core,
                        &session_id,
                        &stream_id,
                    )?;
                }
                active_chat_stream_sessions().lock().await.insert(
                    session_id.clone(),
                    ActiveChatStreamBinding::new(
                        stream_id.clone(),
                        stream_id.clone(),
                        scope.clone(),
                    )
                    .with_user_input(user_input.clone()),
                );
                let (tx, rx) = mpsc::unbounded_channel::<StreamFrame>();
                tx.send(StreamFrame::Start {
                    stream_id: stream_id.clone(),
                })?;
                let has_assistant_delta_streamed = Arc::new(AtomicBool::new(false));
                let (steer_tx, steer_rx) = mpsc::channel::<SteerMessage>(64);
                active_chat_stream_steers()
                    .lock()
                    .await
                    .insert(stream_id.clone(), steer_tx);
                let worker_stream_id = stream_id.clone();
                let worker_turn_id = stream_id.clone();
                let worker_session_id = session_id.clone();
                let worker_session_registry_id = session_id.clone();
                let worker_user_input = user_input.clone();
                let worker_workspace_root = workspace_root.clone();
                let worker_core = core.clone();
                let handle = tokio::spawn(async move {
                    let _foreground_lease = foreground_lease;
                    let emitter = IpcStreamEmitter::new(
                        worker_core.clone(),
                        worker_session_id.clone(),
                        worker_turn_id.clone(),
                        tx.clone(),
                        has_assistant_delta_streamed.clone(),
                    );
                    let result = if take_pending_canceled_stream(
                        &worker_stream_id,
                        &worker_session_id,
                        &scope,
                    )
                    .await
                    {
                        let _ = runtime::cancel_pending_turn_in_session_store(
                            &worker_core,
                            &worker_session_id,
                            &worker_turn_id,
                            worker_user_input.as_deref(),
                        );
                        Err(ExecuteChatSessionError::Canceled)
                    } else {
                        execute_chat_session(
                            &worker_core,
                            ExecuteChatSessionRequest {
                                session_id: worker_session_id,
                                user_input: worker_user_input,
                                turn_id: worker_turn_id,
                                workspace_root: worker_workspace_root,
                                ack_frame_tx: Some(tx.clone()),
                                emitter: Some(Box::new(emitter)),
                                steer_rx: Some(steer_rx),
                                has_text_streamed: Some(has_assistant_delta_streamed.clone()),
                            },
                        )
                        .await
                    };

                    match result {
                        Ok(session) => {
                            if let Some((content, total_tokens)) =
                                latest_assistant_payload(&session)
                            {
                                if !has_assistant_delta_streamed.load(Ordering::Relaxed)
                                    && !content.is_empty()
                                {
                                    has_assistant_delta_streamed.store(true, Ordering::Relaxed);
                                    let _ = tx.send(StreamFrame::Data { content });
                                }
                                let _ = tx.send(StreamFrame::Done { total_tokens });
                            } else {
                                let _ = tx.send(StreamFrame::error(
                                    500,
                                    "Assistant response missing after execution",
                                ));
                            }
                        }
                        Err(ExecuteChatSessionError::Canceled) => {
                            let _ = tx.send(StreamFrame::Done { total_tokens: None });
                        }
                        Err(err) => {
                            let _ = tx.send(StreamFrame::error(err.status_code(), err.to_string()));
                        }
                    }

                    let mut streams = active_chat_streams().lock().await;
                    let removed_registered_handle = streams.remove(&worker_stream_id).is_some();
                    if !removed_registered_handle {
                        completed_chat_streams()
                            .lock()
                            .await
                            .insert(worker_stream_id.clone());
                    }
                    active_chat_stream_steers()
                        .lock()
                        .await
                        .remove(&worker_stream_id);
                    let mut session_streams = active_chat_stream_sessions().lock().await;
                    if session_streams
                        .get(&worker_session_registry_id)
                        .is_some_and(|binding| binding.stream_id == worker_stream_id)
                    {
                        session_streams.remove(&worker_session_registry_id);
                    }
                });

                let mut handle = Some(handle);
                let completed_before_registration = {
                    let mut streams = active_chat_streams().lock().await;
                    let mut completed = completed_chat_streams().lock().await;
                    if completed.remove(&stream_id) {
                        true
                    } else if let Some(handle) = handle.take() {
                        streams.insert(stream_id.clone(), handle);
                        false
                    } else {
                        false
                    }
                };
                if completed_before_registration {
                    active_chat_stream_steers().lock().await.remove(&stream_id);
                    if let Some(handle) = handle.take() {
                        let _ = handle.await;
                    }
                }

                Ok(rx)
            }

            async fn open_session_event_stream(
                core: Arc<AppCore>,
            ) -> Result<mpsc::UnboundedReceiver<StreamFrame>> {
                let stream_id = format!("session-events-{}", Uuid::new_v4());
                let (tx, rx) = mpsc::unbounded_channel::<StreamFrame>();
                let mut receiver = subscribe_session_events();
                let mut snapshot = Self::session_event_snapshot(&core).unwrap_or_default();
                let mut deduper = SessionEventDeduper::default();
                tx.send(StreamFrame::Start {
                    stream_id: stream_id.clone(),
                })?;

                tokio::spawn(async move {
                    let mut poll = interval(Duration::from_millis(750));
                    loop {
                        tokio::select! {
                            event = receiver.recv() => {
                                let event = match event {
                                    Ok(event) => event,
                                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                                        warn!(
                                            skipped,
                                            "Session event stream lagged; dropping oldest events"
                                        );
                                        continue;
                                    }
                                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                        let _ =
                                            tx.send(StreamFrame::error(500, "Session event stream closed"));
                                        break;
                                    }
                                };

                                if let Ok(next_snapshot) = Self::session_event_snapshot(&core) {
                                    if deduper.should_send(&event, &next_snapshot)
                                        && tx
                                            .send(StreamFrame::Event {
                                                event: IpcStreamEvent::Session(event),
                                            })
                                            .is_err()
                                    {
                                        break;
                                    }
                                    snapshot = next_snapshot;
                                } else if tx
                                    .send(StreamFrame::Event {
                                        event: IpcStreamEvent::Session(event),
                                    })
                                    .is_err()
                                {
                                    break;
                                }
                            }
                            _ = poll.tick() => {
                                let next_snapshot = match Self::session_event_snapshot(&core) {
                                    Ok(snapshot) => snapshot,
                                    Err(error) => {
                                        warn!(error = %error, "Failed to poll session event snapshot");
                                        continue;
                                    }
                                };
                                for (session_id, next) in &next_snapshot {
                                    let event = match snapshot.get(session_id) {
                                        None => Some(ChatSessionEvent::Created {
                                            session_id: session_id.clone(),
                                        }),
                                        Some(previous) if previous.message_count < next.message_count => {
                                            Some(ChatSessionEvent::MessageAdded {
                                                session_id: session_id.clone(),
                                                source: "file".to_string(),
                                            })
                                        }
                                        Some(previous)
                                            if previous.updated_at != next.updated_at
                                                || previous.event_count != next.event_count
                                                || previous.event_digest != next.event_digest =>
                                        {
                                            Some(ChatSessionEvent::Updated {
                                                session_id: session_id.clone(),
                                            })
                                        }
                                        _ => None,
                                    };
                                    if let Some(event) = event {
                                        if !deduper.should_send(&event, &next_snapshot) {
                                            continue;
                                        }
                                        if tx
                                            .send(StreamFrame::Event {
                                                event: IpcStreamEvent::Session(event),
                                            })
                                            .is_err()
                                        {
                                            return;
                                        }
                                    }
                                }
                                for session_id in snapshot.keys() {
                                    if !next_snapshot.contains_key(session_id) {
                                        let event = ChatSessionEvent::Deleted {
                                            session_id: session_id.clone(),
                                        };
                                        if !deduper.should_send(&event, &next_snapshot) {
                                            continue;
                                        }
                                        if tx
                                            .send(StreamFrame::Event {
                                                event: IpcStreamEvent::Session(event),
                                            })
                                            .is_err()
                                        {
                                            return;
                                        }
                                    }
                                }
                                snapshot = next_snapshot;
                            }
                        }
                    }

                    debug!(stream_id = %stream_id, "Session event subscription ended");
                });

                Ok(rx)
            }

            fn session_event_snapshot(
                core: &AppCore,
            ) -> Result<HashMap<String, SessionEventSnapshot>> {
                let sessions = core.storage.file_sessions.list_uncached()?;
                Ok(sessions
                    .into_iter()
                    .map(|session| {
                        let summary = types::ChatSessionSummary::from(&session.to_chat_session());
                        (
                            summary.id,
                            SessionEventSnapshot {
                                updated_at: summary.updated_at,
                                message_count: summary.message_count,
                                event_count: session.events.len(),
                                event_digest: session_event_digest(&session.events),
                            },
                        )
                    })
                    .collect())
            }
        }

        fn session_event_digest(events: &[restflow_core::session_log::SessionLogEvent]) -> u64 {
            let mut hasher = DefaultHasher::new();
            for event in events {
                if let Ok(bytes) = serde_json::to_vec(event) {
                    bytes.hash(&mut hasher);
                }
            }
            hasher.finish()
        }

        #[cfg(test)]
        #[path = "ipc_server/tests/mod.rs"]
        mod tests {
            pub(super) use super::runtime::{
                build_agent_system_prompt, cancel_chat_stream,
                cancel_orphaned_running_turns_in_session_store, complete_turn_after_execution,
                fail_turn_in_session_store, load_chat_max_session_history_from_core,
                persist_ipc_user_message_if_needed, persist_steer_if_stream_still_registered,
                record_turn_event_in_session_store, record_turn_user_message_in_session_store,
                steer_chat_stream, sync_session_view_from_session_store,
            };
            pub(super) use super::*;
            pub(super) use restflow_core::services::agent_catalog;
            pub(super) use restflow_core::test_support::RestflowTestEnv;
            pub(super) use types::AgentNode;
            pub(super) use types::SteerCommand;
            pub(super) use types::store::ReplySender;
            pub(super) use uuid::Uuid;

            pub(super) struct TestCoreEnv {
                pub state: RestflowTestEnv,
            }

            #[allow(clippy::await_holding_lock)]
            pub(super) async fn create_test_core() -> (Arc<AppCore>, TestCoreEnv) {
                let state = RestflowTestEnv::new();
                let db_path = state.db_path("ipc-server-test.db");
                let core = Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap());
                (core, TestCoreEnv { state })
            }

            #[test]
            fn purge_expired_pending_canceled_streams_removes_stale_entries() {
                let mut pending = HashMap::new();
                pending.insert(
                    "old-stream".to_string(),
                    PendingCanceledChatStream {
                        session_id: "session-1".to_string(),
                        scope: None,
                        expires_at: Instant::now() - Duration::from_secs(1),
                    },
                );
                pending.insert(
                    "fresh-stream".to_string(),
                    PendingCanceledChatStream::new("session-1", None),
                );

                purge_expired_pending_canceled_streams(&mut pending);

                assert!(!pending.contains_key("old-stream"));
                assert!(pending.contains_key("fresh-stream"));
            }

            #[test]
            fn session_event_deduper_suppresses_poll_broadcast_echo() {
                let mut deduper = SessionEventDeduper::default();
                let mut snapshot = HashMap::new();
                snapshot.insert(
                    "session-1".to_string(),
                    SessionEventSnapshot {
                        updated_at: 20,
                        message_count: 2,
                        event_count: 4,
                        event_digest: 400,
                    },
                );
                let poll_event = ChatSessionEvent::MessageAdded {
                    session_id: "session-1".to_string(),
                    source: "file".to_string(),
                };
                let broadcast_event = ChatSessionEvent::MessageAdded {
                    session_id: "session-1".to_string(),
                    source: "workspace".to_string(),
                };

                assert!(deduper.should_send(&poll_event, &snapshot));
                assert!(!deduper.should_send(&broadcast_event, &snapshot));

                snapshot.insert(
                    "session-1".to_string(),
                    SessionEventSnapshot {
                        updated_at: 30,
                        message_count: 3,
                        event_count: 5,
                        event_digest: 500,
                    },
                );
                assert!(deduper.should_send(&broadcast_event, &snapshot));

                snapshot.insert(
                    "session-1".to_string(),
                    SessionEventSnapshot {
                        updated_at: 30,
                        message_count: 3,
                        event_count: 6,
                        event_digest: 600,
                    },
                );
                assert!(deduper.should_send(&broadcast_event, &snapshot));

                snapshot.insert(
                    "session-1".to_string(),
                    SessionEventSnapshot {
                        updated_at: 30,
                        message_count: 3,
                        event_count: 6,
                        event_digest: 601,
                    },
                );
                assert!(deduper.should_send(&broadcast_event, &snapshot));
            }

            #[tokio::test]
            async fn create_test_core_isolates_restflow_dir_env() {
                let first_state_path = {
                    let (_core, env) = create_test_core().await;
                    let current = std::env::var("RESTFLOW_DIR").expect("restflow dir env");
                    assert_eq!(current, env.state.root().to_string_lossy());
                    assert!(std::env::var_os(agent_catalog::AGENTS_DIR_ENV).is_none());
                    current
                };

                let second_state_path = {
                    let (_core, env) = create_test_core().await;
                    let current = std::env::var("RESTFLOW_DIR").expect("restflow dir env");
                    assert_eq!(current, env.state.root().to_string_lossy());
                    assert!(std::env::var_os(agent_catalog::AGENTS_DIR_ENV).is_none());
                    current
                };

                assert_ne!(first_state_path, second_state_path);
            }

            mod agents {
                use super::*;
                use crate::daemon::request_mapper::to_contract;
                use types::CleanupReportResponse;
                use types::request::{AgentNode as ContractAgentNode, WireModelRef};

                #[tokio::test]
                async fn process_run_cleanup_returns_report() {
                    let (core, _temp) = create_test_core().await;
                    let runtime_tool_registry = OnceLock::new();

                    let response =
                        IpcServer::process(&core, &runtime_tool_registry, IpcRequest::RunCleanup)
                            .await;

                    match response {
                        IpcResponse::Success(value) => {
                            let report: CleanupReportResponse =
                                serde_json::from_value(value).expect("cleanup report");
                            assert_eq!(report.chat_sessions, 0);
                        }
                        other => panic!("expected success response, got {other:?}"),
                    }
                }

                #[tokio::test]
                async fn process_set_and_get_secret_round_trip() {
                    let (core, _temp) = create_test_core().await;
                    let runtime_tool_registry = OnceLock::new();

                    let set_response = IpcServer::process(
                        &core,
                        &runtime_tool_registry,
                        IpcRequest::SetSecret {
                            key: "TEST_SECRET".to_string(),
                            value: "secret-value".to_string(),
                            description: Some("test secret".to_string()),
                        },
                    )
                    .await;
                    match set_response {
                        IpcResponse::Success(_) => {}
                        other => panic!("expected success response, got {other:?}"),
                    }

                    let get_response = IpcServer::process(
                        &core,
                        &runtime_tool_registry,
                        IpcRequest::GetSecret {
                            key: "TEST_SECRET".to_string(),
                        },
                    )
                    .await;

                    match get_response {
                        IpcResponse::Success(value) => {
                            assert_eq!(value["value"], "secret-value");
                        }
                        other => panic!("expected success response, got {other:?}"),
                    }
                }

                #[tokio::test]
                async fn process_create_agent_returns_stored_agent() {
                    let (core, _temp) = create_test_core().await;
                    let runtime_tool_registry = OnceLock::new();

                    let response = IpcServer::process(
                        &core,
                        &runtime_tool_registry,
                        IpcRequest::CreateAgent {
                            name: "IPC Agent".to_string(),
                            agent: to_contract(AgentNode {
                                model_ref: Some(types::ModelRef::from_model(
                                    types::ModelId::ClaudeSonnet4_5,
                                )),
                                prompt: Some("You are a helpful assistant".to_string()),
                                temperature: Some(0.7),
                                codex_cli_reasoning_effort: None,
                                codex_cli_execution_mode: None,
                                api_key_config: Some(types::ApiKeyConfig::Direct(
                                    "test_key".to_string(),
                                )),
                                tools: None,
                                skills: None,
                                skill_variables: None,
                                skill_preflight_policy_mode: None,
                                model_routing: None,
                            })
                            .expect("contract agent node"),
                        },
                    )
                    .await;

                    match response {
                        IpcResponse::Success(value) => {
                            assert_eq!(value["name"], "IPC Agent");
                            assert!(value["id"].as_str().is_some());
                        }
                        other => panic!("expected success response, got {other:?}"),
                    }
                }

                #[tokio::test]
                async fn process_create_agent_with_warning_persists_without_confirmation() {
                    let (core, _temp) = create_test_core().await;
                    let runtime_tool_registry = OnceLock::new();

                    let response = IpcServer::process(
                        &core,
                        &runtime_tool_registry,
                        IpcRequest::CreateAgent {
                            name: "warning-agent".to_string(),
                            agent: to_contract(AgentNode::new()).expect("contract agent"),
                        },
                    )
                    .await;

                    match response {
                        IpcResponse::Success(value) => {
                            assert_eq!(value["name"], "warning-agent");
                            assert!(value["id"].as_str().is_some());
                            let agents = core.storage.agents.list_agents().unwrap();
                            assert_eq!(agents.len(), 2, "warning should not block persistence");
                        }
                        other => panic!("expected success response, got {other:?}"),
                    }
                }

                #[tokio::test]
                async fn process_create_agent_still_returns_stored_agent_when_provider_is_unconfigured()
                 {
                    let (core, _temp) = create_test_core().await;
                    let runtime_tool_registry = OnceLock::new();

                    let response = IpcServer::process(
                        &core,
                        &runtime_tool_registry,
                        IpcRequest::CreateAgent {
                            name: "warning-agent".to_string(),
                            agent: to_contract(AgentNode::new()).expect("contract agent"),
                        },
                    )
                    .await;

                    match response {
                        IpcResponse::Success(value) => {
                            assert_eq!(value["name"], "warning-agent");
                            assert!(value["id"].as_str().is_some());
                        }
                        other => panic!("expected success response, got {other:?}"),
                    }
                }

                #[tokio::test]
                async fn process_create_agent_rejects_invalid_wire_model_ref() {
                    let (core, _temp) = create_test_core().await;
                    let runtime_tool_registry = OnceLock::new();

                    let response = IpcServer::process(
                        &core,
                        &runtime_tool_registry,
                        IpcRequest::CreateAgent {
                            name: "invalid-agent".to_string(),
                            agent: ContractAgentNode {
                                model_ref: Some(WireModelRef {
                                    provider: "unknown-provider".to_string(),
                                    model: "gpt-5".to_string(),
                                }),
                                ..ContractAgentNode::default()
                            },
                        },
                    )
                    .await;

                    match response {
                        IpcResponse::Error(error) => {
                            assert_eq!(error.code, 400);
                            assert_eq!(error.kind, types::ErrorKind::Validation);
                            let details = error.details.expect("validation details");
                            assert_eq!(details["type"], "validation_error");
                            assert_eq!(details["errors"][0]["field"], "model_ref.provider");
                        }
                        other => panic!("expected validation error, got {other:?}"),
                    }
                }

                #[tokio::test]
                async fn process_get_config_returns_system_config() {
                    let (core, _temp) = create_test_core().await;
                    let runtime_tool_registry = OnceLock::new();

                    let response =
                        IpcServer::process(&core, &runtime_tool_registry, IpcRequest::GetConfig)
                            .await;

                    match response {
                        IpcResponse::Success(value) => {
                            let _config: restflow_core::storage::SystemConfig =
                                serde_json::from_value(value).expect("system config");
                        }
                        other => panic!("expected success response, got {other:?}"),
                    }
                }
            }
            mod runtime_tools {
                use super::*;

                #[tokio::test]
                /// Skills are now registered as callable tools, not injected into the system prompt.
                async fn build_agent_system_prompt_does_not_inject_skills() {
                    let (core, _temp) = create_test_core().await;

                    let mut variables = std::collections::HashMap::new();
                    variables.insert("name".to_string(), "World".to_string());

                    let agent_node = AgentNode::new()
                        .with_prompt("Base prompt")
                        .with_skills(vec!["skill-1".to_string()])
                        .with_skill_variables(variables);

                    let prompt = build_agent_system_prompt(&core, agent_node).unwrap();
                    assert!(prompt.contains("Base prompt"));
                    // Skills are now tools, not injected into prompt
                    assert!(!prompt.contains("## Skill: Test Skill"));
                }
            }
            mod sessions {
                use super::*;
                use types::ChatTurnStatus;

                fn save_chat_session(core: &AppCore, session: &ChatSession) {
                    core.storage
                        .file_sessions
                        .write_session(
                            &restflow_core::session_log::FileSession::from_chat_session(session),
                            true,
                        )
                        .unwrap();
                }

                fn load_chat_session(core: &AppCore, session_id: &str) -> ChatSession {
                    core.storage
                        .file_sessions
                        .get(session_id)
                        .unwrap()
                        .expect("session")
                        .to_chat_session()
                }

                #[tokio::test]
                async fn load_chat_max_session_history_from_core_uses_runtime_config() {
                    let (core, _temp) = create_test_core().await;
                    let mut config = core.storage.config.get_effective_config().unwrap();
                    config.runtime_defaults.chat_max_session_history = 42;
                    core.storage.config.update_config(config).unwrap();

                    assert_eq!(load_chat_max_session_history_from_core(&core), 42);
                }

                #[tokio::test]
                async fn persist_ipc_user_message_if_needed_adds_missing_user_turn() {
                    let (core, _temp) = create_test_core().await;
                    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                    save_chat_session(&core, &session);

                    persist_ipc_user_message_if_needed(&core, &mut session, Some("hello"), "hello")
                        .unwrap();

                    let stored = load_chat_session(&core, &session.id);
                    assert_eq!(stored.messages.len(), 1);
                    assert_eq!(stored.messages[0].role, ChatRole::User);
                    assert_eq!(stored.messages[0].content, "hello");
                }

                #[tokio::test]
                async fn persist_ipc_user_message_if_needed_deduplicates_latest_user_turn() {
                    let (core, _temp) = create_test_core().await;
                    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                    session.add_message(ChatMessage::user("hello"));
                    save_chat_session(&core, &session);

                    persist_ipc_user_message_if_needed(&core, &mut session, Some("hello"), "hello")
                        .unwrap();

                    let stored = load_chat_session(&core, &session.id);
                    assert_eq!(stored.messages.len(), 1);
                }

                #[tokio::test]
                async fn record_turn_event_in_session_store_persists_tool_events() {
                    let (core, _temp) = create_test_core().await;
                    let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                    save_chat_session(&core, &session);

                    record_turn_event_in_session_store(
                        &core,
                        &session.id,
                        "turn-1",
                        types::ChatTurnEventKind::ToolCall {
                            call_id: "call-1".to_string(),
                            name: "bash".to_string(),
                            arguments: "pwd".to_string(),
                        },
                    )
                    .unwrap();

                    let stored = load_chat_session(&core, &session.id);
                    assert_eq!(stored.turns.len(), 1);
                    assert_eq!(stored.turns[0].events.len(), 1);
                    assert!(matches!(
                        stored.turns[0].events[0].kind,
                        types::ChatTurnEventKind::ToolCall { .. }
                    ));
                }

                #[tokio::test]
                async fn record_turn_event_does_not_revive_canceled_turn() {
                    let (core, _temp) = create_test_core().await;
                    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                    session.record_turn_user_message("turn-1", "hello");
                    session.cancel_turn("turn-1");
                    save_chat_session(&core, &session);

                    record_turn_event_in_session_store(
                        &core,
                        &session.id,
                        "turn-1",
                        types::ChatTurnEventKind::ToolCall {
                            call_id: "call-after-cancel".to_string(),
                            name: "bash".to_string(),
                            arguments: "pwd".to_string(),
                        },
                    )
                    .unwrap();

                    let stored = load_chat_session(&core, &session.id);
                    let turn = stored
                        .turns
                        .iter()
                        .find(|turn| turn.id == "turn-1")
                        .expect("turn");
                    assert_eq!(turn.status, ChatTurnStatus::Canceled);
                    assert!(!turn.events.iter().any(|event| {
                        matches!(
                            &event.kind,
                            types::ChatTurnEventKind::ToolCall { call_id, .. }
                                if call_id == "call-after-cancel"
                        )
                    }));
                }

                #[tokio::test]
                async fn record_turn_user_message_does_not_revive_canceled_turn() {
                    let (core, _temp) = create_test_core().await;
                    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                    session.record_turn_user_message("turn-1", "hello");
                    session.cancel_turn("turn-1");
                    save_chat_session(&core, &session);

                    let recorded = record_turn_user_message_in_session_store(
                        &core,
                        &mut session,
                        "turn-1",
                        "late user",
                    )
                    .unwrap();

                    assert!(!recorded);
                    let stored = load_chat_session(&core, &session.id);
                    let turn = stored
                        .turns
                        .iter()
                        .find(|turn| turn.id == "turn-1")
                        .expect("turn");
                    assert_eq!(turn.status, ChatTurnStatus::Canceled);
                    assert!(!turn.events.iter().any(|event| {
                        matches!(
                            &event.kind,
                            ChatTurnEventKind::UserMessage { content } if content == "late user"
                        )
                    }));
                }

                #[tokio::test]
                async fn persist_ipc_user_message_if_needed_auto_names_new_chat() {
                    let (core, _temp) = create_test_core().await;
                    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                    save_chat_session(&core, &session);

                    persist_ipc_user_message_if_needed(
                        &core,
                        &mut session,
                        Some("hello from ipc"),
                        "hello from ipc",
                    )
                    .unwrap();

                    let stored = load_chat_session(&core, &session.id);
                    assert_eq!(stored.name, "hello from ipc");
                }

                #[tokio::test]
                async fn sync_session_view_preserves_execution_agent_id() {
                    let (core, _temp) = create_test_core().await;
                    let stored_session =
                        ChatSession::new("missing-agent".to_string(), "gpt-5.5".to_string());
                    save_chat_session(&core, &stored_session);

                    let mut execution_session = stored_session.clone();
                    execution_session.agent_id = "fallback-agent".to_string();
                    sync_session_view_from_session_store(&core, &mut execution_session).unwrap();

                    assert_eq!(execution_session.agent_id, "fallback-agent");
                    assert_eq!(execution_session.id, stored_session.id);
                }

                #[tokio::test]
                async fn fail_turn_persists_repaired_execution_agent_id() {
                    let (core, _temp) = create_test_core().await;
                    let mut session =
                        ChatSession::new("missing-agent".to_string(), "gpt-5.5".to_string());
                    session.record_turn_user_message("turn-1", "hello");
                    save_chat_session(&core, &session);

                    fail_turn_in_session_store(
                        &core,
                        &session.id,
                        "turn-1",
                        "model failed",
                        Some(SessionAgentIdRepair {
                            expected_existing_agent_id: "missing-agent".to_string(),
                            repaired_agent_id: "fallback-agent".to_string(),
                        }),
                    )
                    .unwrap();

                    let stored = load_chat_session(&core, &session.id);
                    assert_eq!(stored.agent_id, "fallback-agent");
                    assert_eq!(stored.turns[0].status, ChatTurnStatus::Failed);
                }

                #[test]
                fn normalize_model_input_converts_to_serialized_form() {
                    assert_eq!(normalize_model_input("openai:gpt-5.5").unwrap(), "gpt-5-5");
                    assert_eq!(normalize_model_input("gpt-5.1").unwrap(), "gpt-5-1");
                }

                #[test]
                fn normalize_model_input_rejects_unknown_value() {
                    assert!(normalize_model_input("not-a-real-model").is_err());
                }

                #[test]
                fn normalize_model_input_rejects_cross_provider_collision() {
                    let error = normalize_model_input("gpt-5.5").expect_err("ambiguous model");
                    assert!(error.to_string().contains("Ambiguous model identifier"));
                }

                #[tokio::test]
                async fn create_session_accepts_provider_qualified_api_model_name() {
                    let (core, _temp) = create_test_core().await;
                    let runtime_tool_registry = OnceLock::new();

                    let response = IpcServer::process(
                        &core,
                        &runtime_tool_registry,
                        IpcRequest::CreateSessionWithProvider {
                            agent_id: None,
                            provider: Some("openai".to_string()),
                            model: Some("gpt-5.5".to_string()),
                            name: None,
                            skill_id: None,
                        },
                    )
                    .await;

                    match response {
                        IpcResponse::Success(value) => {
                            let session: ChatSession =
                                serde_json::from_value(value).expect("chat session");
                            assert_eq!(session.provider, "openai");
                            assert_eq!(session.model, types::ModelId::Gpt5_5.as_serialized_str());
                        }
                        other => panic!("expected success response, got {other:?}"),
                    }
                }

                #[tokio::test]
                async fn create_session_preserves_legacy_providerless_api_model() {
                    let (core, _temp) = create_test_core().await;
                    let runtime_tool_registry = OnceLock::new();

                    let response = IpcServer::process(
                        &core,
                        &runtime_tool_registry,
                        IpcRequest::CreateSession {
                            agent_id: None,
                            model: Some("gpt-5.5".to_string()),
                            name: None,
                            skill_id: None,
                        },
                    )
                    .await;

                    match response {
                        IpcResponse::Success(value) => {
                            let session: ChatSession = serde_json::from_value(value).unwrap();
                            assert_eq!(session.provider, "openai");
                            assert_eq!(session.model, "gpt-5-5");
                        }
                        other => panic!("expected success response, got {other:?}"),
                    }
                }

                #[tokio::test]
                async fn create_session_provider_only_uses_provider_default_model() {
                    let (core, _temp) = create_test_core().await;
                    let runtime_tool_registry = OnceLock::new();

                    let response = IpcServer::process(
                        &core,
                        &runtime_tool_registry,
                        IpcRequest::CreateSessionWithProvider {
                            agent_id: None,
                            provider: Some("openai".to_string()),
                            model: None,
                            name: None,
                            skill_id: None,
                        },
                    )
                    .await;

                    match response {
                        IpcResponse::Success(value) => {
                            let session: ChatSession =
                                serde_json::from_value(value).expect("chat session");
                            assert_eq!(session.provider, "openai");
                            assert_eq!(session.model, types::ModelId::Gpt5_5.as_serialized_str());
                        }
                        other => panic!("expected success response, got {other:?}"),
                    }
                }

                #[tokio::test]
                async fn create_session_without_model_uses_available_secret_default() {
                    let (core, _temp) = create_test_core().await;
                    let runtime_tool_registry = OnceLock::new();
                    core.storage
                        .secrets
                        .set_secret("MINIMAX_API_KEY", "test-minimax-key", None)
                        .unwrap();
                    let agent = core
                        .storage
                        .agents
                        .create_agent("No Model Agent".to_string(), AgentNode::new())
                        .unwrap();

                    let response = IpcServer::process(
                        &core,
                        &runtime_tool_registry,
                        IpcRequest::CreateSession {
                            agent_id: Some(agent.id),
                            model: None,
                            name: None,
                            skill_id: None,
                        },
                    )
                    .await;

                    match response {
                        IpcResponse::Success(value) => {
                            let session: ChatSession =
                                serde_json::from_value(value).expect("chat session");
                            assert_eq!(session.provider, "minimax");
                            assert_eq!(
                                session.model,
                                restflow_core::provider_policy::provider_default_model(
                                    types::Provider::MiniMax
                                )
                                .as_serialized_str()
                            );
                        }
                        other => panic!("expected success response, got {other:?}"),
                    }
                }

                #[tokio::test]
                async fn create_session_without_model_prefers_configured_cli_default() {
                    let (core, _temp) = create_test_core().await;
                    let runtime_tool_registry = OnceLock::new();
                    let mut cli_config =
                        restflow_core::storage::load_global_cli_config().expect("cli config");
                    cli_config.model = Some("openai:gpt-5.5".to_string());
                    restflow_core::storage::write_cli_config(&cli_config)
                        .expect("write cli config");
                    core.storage
                        .secrets
                        .set_secret("MINIMAX_API_KEY", "test-minimax-key", None)
                        .unwrap();
                    let agent = core
                        .storage
                        .agents
                        .create_agent("No Model Agent".to_string(), AgentNode::new())
                        .unwrap();

                    let response = IpcServer::process(
                        &core,
                        &runtime_tool_registry,
                        IpcRequest::CreateSession {
                            agent_id: Some(agent.id),
                            model: None,
                            name: None,
                            skill_id: None,
                        },
                    )
                    .await;

                    match response {
                        IpcResponse::Success(value) => {
                            let session: ChatSession =
                                serde_json::from_value(value).expect("chat session");
                            assert_eq!(session.provider, "openai");
                            assert_eq!(session.model, types::ModelId::Gpt5_5.as_serialized_str());
                        }
                        other => panic!("expected success response, got {other:?}"),
                    }
                }

                #[tokio::test]
                async fn create_session_without_model_prefers_agent_model_before_cli_default() {
                    let (core, _temp) = create_test_core().await;
                    let runtime_tool_registry = OnceLock::new();
                    let mut cli_config =
                        restflow_core::storage::load_global_cli_config().expect("cli config");
                    cli_config.model = Some("openai:gpt-5.5".to_string());
                    restflow_core::storage::write_cli_config(&cli_config)
                        .expect("write cli config");
                    let agent = core
                        .storage
                        .agents
                        .create_agent(
                            "Pinned Agent".to_string(),
                            AgentNode::with_model(types::ModelId::MiniMaxM27),
                        )
                        .unwrap();

                    let response = IpcServer::process(
                        &core,
                        &runtime_tool_registry,
                        IpcRequest::CreateSession {
                            agent_id: Some(agent.id),
                            model: None,
                            name: None,
                            skill_id: None,
                        },
                    )
                    .await;

                    match response {
                        IpcResponse::Success(value) => {
                            let session: ChatSession =
                                serde_json::from_value(value).expect("chat session");
                            assert_eq!(session.provider, "minimax");
                            assert_eq!(
                                session.model,
                                types::ModelId::MiniMaxM27.as_serialized_str()
                            );
                        }
                        other => panic!("expected success response, got {other:?}"),
                    }
                }

                #[tokio::test]
                async fn search_sessions_applies_agent_filter_and_limit() {
                    let (core, _temp) = create_test_core().await;
                    let runtime_tool_registry = OnceLock::new();

                    for index in 0..3 {
                        let mut session =
                            ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                        session.rename(format!("match agent one {index}"));
                        session.add_message(ChatMessage::user("needle"));
                        save_chat_session(&core, &session);
                    }
                    let mut other_agent =
                        ChatSession::new("agent-2".to_string(), "gpt-5".to_string());
                    other_agent.rename("match agent two");
                    other_agent.add_message(ChatMessage::user("needle"));
                    save_chat_session(&core, &other_agent);

                    let response = IpcServer::process(
                        &core,
                        &runtime_tool_registry,
                        IpcRequest::SearchSessions {
                            query: "needle".to_string(),
                            agent_id: Some("agent-1".to_string()),
                            limit: Some(2),
                        },
                    )
                    .await;

                    match response {
                        IpcResponse::Success(value) => {
                            let sessions: Vec<types::ChatSessionSummary> =
                                serde_json::from_value(value).expect("session summaries");
                            assert_eq!(sessions.len(), 2);
                            assert!(sessions.iter().all(|session| session.agent_id == "agent-1"));
                        }
                        other => panic!("expected success response, got {other:?}"),
                    }
                }

                #[tokio::test]
                async fn steer_chat_stream_delivers_message_to_registered_stream() {
                    let (core, _temp) = create_test_core().await;
                    let session_service = SessionService::from_storage(&core.storage);
                    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                    session.add_message(ChatMessage::user("start"));
                    save_chat_session(&core, &session);
                    let session_id = session.id.clone();
                    let stream_id = format!("stream-{}", Uuid::new_v4());
                    let turn_id = stream_id.clone();
                    let (tx, mut rx) = mpsc::channel::<SteerMessage>(1);

                    active_chat_stream_sessions().lock().await.insert(
                        session_id.clone(),
                        ActiveChatStreamBinding::new(stream_id.clone(), turn_id.clone(), None),
                    );
                    active_chat_stream_steers()
                        .lock()
                        .await
                        .insert(stream_id.clone(), tx);

                    let steered =
                        steer_chat_stream(&core, &session_id, "continue with option B", None).await;
                    assert!(steered.unwrap());

                    let message = rx.recv().await.expect("steer message");
                    match message.command {
                        SteerCommand::Message { instruction } => {
                            assert_eq!(instruction, "continue with option B")
                        }
                        _ => panic!("expected message steer command"),
                    }
                    let stored = session_service
                        .get_session_view(&session_id)
                        .unwrap()
                        .expect("stored session");
                    assert!(stored.messages.iter().any(|message| {
                        message.role == ChatRole::User
                            && message.content == "continue with option B"
                    }));
                    let turn = stored
                        .turns
                        .iter()
                        .find(|turn| turn.id == turn_id)
                        .expect("stored turn");
                    assert!(turn.events.iter().any(|event| matches!(
                        &event.kind,
                        ChatTurnEventKind::UserMessage { content } if content == "continue with option B"
                    )));

                    let steered_again =
                        steer_chat_stream(&core, &session_id, "continue with option B", None).await;
                    assert!(steered_again.unwrap());
                    let repeated_message = rx.recv().await.expect("repeated steer message");
                    match repeated_message.command {
                        SteerCommand::Message { instruction } => {
                            assert_eq!(instruction, "continue with option B")
                        }
                        _ => panic!("expected repeated message steer command"),
                    }
                    let stored = session_service
                        .get_session_view(&session_id)
                        .unwrap()
                        .expect("stored session after repeated steer");
                    assert_eq!(
                        stored
                            .messages
                            .iter()
                            .filter(|message| {
                                message.role == ChatRole::User
                                    && message.content == "continue with option B"
                            })
                            .count(),
                        2
                    );
                    let turn = stored
                        .turns
                        .iter()
                        .find(|turn| turn.id == turn_id)
                        .expect("stored turn after repeated steer");
                    assert_eq!(
                        turn.events
                            .iter()
                            .filter(|event| matches!(
                                &event.kind,
                                ChatTurnEventKind::UserMessage { content }
                                    if content == "continue with option B"
                            ))
                            .count(),
                        2
                    );

                    active_chat_stream_sessions()
                        .lock()
                        .await
                        .remove(&session_id);
                    active_chat_stream_steers().lock().await.remove(&stream_id);
                }

                #[tokio::test]
                async fn steer_chat_stream_does_not_deliver_when_persistence_fails() {
                    let (core, _temp) = create_test_core().await;
                    let session_id = format!("session-{}", Uuid::new_v4());
                    let stream_id = format!("stream-{}", Uuid::new_v4());
                    let (tx, mut rx) = mpsc::channel::<SteerMessage>(1);

                    active_chat_stream_sessions().lock().await.insert(
                        session_id.clone(),
                        ActiveChatStreamBinding::new(stream_id.clone(), stream_id.clone(), None),
                    );
                    active_chat_stream_steers()
                        .lock()
                        .await
                        .insert(stream_id.clone(), tx);

                    let error = steer_chat_stream(&core, &session_id, "unpersisted steer", None)
                        .await
                        .expect_err("missing session should make steer persistence fail");

                    assert!(error.to_string().contains(types::SESSION_NOT_FOUND));
                    assert!(rx.try_recv().is_err());

                    active_chat_stream_sessions()
                        .lock()
                        .await
                        .remove(&session_id);
                    active_chat_stream_steers().lock().await.remove(&stream_id);
                }

                #[tokio::test]
                async fn steer_chat_stream_rejects_different_owner_scope() {
                    let (core, _temp) = create_test_core().await;
                    let session_id = format!("session-{}", Uuid::new_v4());
                    let stream_id = format!("stream-{}", Uuid::new_v4());
                    let owner_scope = types::ExecutionScope::foreground("client-a", "terminal-a");
                    let other_scope = types::ExecutionScope::foreground("client-b", "terminal-b");
                    let (tx, _rx) = mpsc::channel::<SteerMessage>(1);

                    active_chat_stream_sessions().lock().await.insert(
                        session_id.clone(),
                        ActiveChatStreamBinding::new(
                            stream_id.clone(),
                            stream_id.clone(),
                            Some(owner_scope.clone()),
                        ),
                    );
                    active_chat_stream_steers()
                        .lock()
                        .await
                        .insert(stream_id.clone(), tx);

                    let steered =
                        steer_chat_stream(&core, &session_id, "continue", Some(&other_scope)).await;
                    assert!(!steered.unwrap());

                    active_chat_stream_sessions()
                        .lock()
                        .await
                        .remove(&session_id);
                    active_chat_stream_steers().lock().await.remove(&stream_id);
                }

                #[tokio::test]
                async fn steer_chat_stream_rejects_unscoped_request_for_scoped_stream() {
                    let (core, _temp) = create_test_core().await;
                    let session_id = format!("session-{}", Uuid::new_v4());
                    let stream_id = format!("stream-{}", Uuid::new_v4());
                    let owner_scope = types::ExecutionScope::foreground("client-a", "terminal-a");
                    let (tx, mut rx) = mpsc::channel::<SteerMessage>(1);

                    active_chat_stream_sessions().lock().await.insert(
                        session_id.clone(),
                        ActiveChatStreamBinding::new(
                            stream_id.clone(),
                            stream_id.clone(),
                            Some(owner_scope),
                        ),
                    );
                    active_chat_stream_steers()
                        .lock()
                        .await
                        .insert(stream_id.clone(), tx);

                    let steered = steer_chat_stream(&core, &session_id, "continue", None).await;
                    assert!(!steered.unwrap());
                    assert!(rx.try_recv().is_err());

                    active_chat_stream_sessions()
                        .lock()
                        .await
                        .remove(&session_id);
                    active_chat_stream_steers().lock().await.remove(&stream_id);
                }

                #[tokio::test]
                async fn steer_chat_stream_marks_persisted_update_when_delivery_fails() {
                    let (core, _temp) = create_test_core().await;
                    let session_service = SessionService::from_storage(&core.storage);
                    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                    session.add_message(ChatMessage::user("start"));
                    save_chat_session(&core, &session);
                    let session_id = session.id.clone();
                    let stream_id = format!("stream-{}", Uuid::new_v4());
                    let turn_id = stream_id.clone();
                    let (tx, rx) = mpsc::channel::<SteerMessage>(1);
                    drop(rx);

                    active_chat_stream_sessions().lock().await.insert(
                        session_id.clone(),
                        ActiveChatStreamBinding::new(stream_id.clone(), turn_id.clone(), None),
                    );
                    active_chat_stream_steers()
                        .lock()
                        .await
                        .insert(stream_id.clone(), tx);

                    let steered = steer_chat_stream(&core, &session_id, "  late steer  ", None)
                        .await
                        .unwrap();
                    assert!(!steered);

                    let stored = session_service
                        .get_session_view(&session_id)
                        .unwrap()
                        .expect("stored session");
                    assert!(stored.messages.iter().any(|message| {
                        message.role == ChatRole::User && message.content == "late steer"
                    }));
                    let turn = stored
                        .turns
                        .iter()
                        .find(|turn| turn.id == turn_id)
                        .expect("stored turn");
                    assert!(turn.events.iter().any(|event| matches!(
                        &event.kind,
                        ChatTurnEventKind::Error { message }
                            if message.contains("not delivered")
                    )));
                    assert!(
                        !active_chat_stream_sessions()
                            .lock()
                            .await
                            .contains_key(&session_id)
                    );
                    assert!(
                        !active_chat_stream_steers()
                            .lock()
                            .await
                            .contains_key(&stream_id)
                    );
                }

                #[tokio::test]
                async fn steer_chat_stream_returns_false_when_no_active_session_stream() {
                    let (core, _temp) = create_test_core().await;
                    let session_id = format!("session-{}", Uuid::new_v4());
                    let steered = steer_chat_stream(&core, &session_id, "test", None).await;
                    assert!(!steered.unwrap());
                }

                #[tokio::test]
                async fn steer_persistence_skips_removed_stream_binding() {
                    let (core, _temp) = create_test_core().await;
                    let session_service = SessionService::from_storage(&core.storage);
                    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                    session.add_message(ChatMessage::user("start"));
                    save_chat_session(&core, &session);
                    let session_id = session.id.clone();
                    let binding =
                        ActiveChatStreamBinding::new("stream-removed", "stream-removed", None);

                    let persisted = persist_steer_if_stream_still_registered(
                        &core,
                        &session_id,
                        &binding,
                        "late steer",
                    )
                    .await;

                    assert!(!persisted.unwrap());
                    let stored = session_service
                        .get_session_view(&session_id)
                        .unwrap()
                        .expect("stored session");
                    assert!(
                        !stored
                            .messages
                            .iter()
                            .any(|message| message.content == "late steer")
                    );
                }

                #[tokio::test]
                async fn unscoped_foreground_cancel_resolves_scoped_stream_binding() {
                    let (core, _temp) = create_test_core().await;
                    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                    let stream_id = format!("stream-{}", Uuid::new_v4());
                    session.record_turn_user_message(&stream_id, "cancel me");
                    save_chat_session(&core, &session);
                    let session_id = session.id.clone();
                    let scope = types::ExecutionScope::foreground("client-a", "terminal-a");
                    let handle = tokio::spawn(async {
                        std::future::pending::<()>().await;
                    });
                    active_chat_streams()
                        .lock()
                        .await
                        .insert(stream_id.clone(), handle);
                    active_chat_stream_sessions().lock().await.insert(
                        session_id.clone(),
                        ActiveChatStreamBinding::new(
                            stream_id.clone(),
                            stream_id.clone(),
                            Some(scope),
                        ),
                    );

                    let canceled =
                        super::super::cancel_foreground_chat_stream(&core, &stream_id).await;

                    assert!(canceled);
                    assert!(!active_chat_streams().lock().await.contains_key(&stream_id));
                    assert!(
                        !active_chat_stream_sessions()
                            .lock()
                            .await
                            .contains_key(&session_id)
                    );
                }

                #[tokio::test]
                async fn foreground_startup_cancels_orphaned_running_turns() {
                    let (core, _temp) = create_test_core().await;
                    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                    session.record_turn_user_message("old-turn", "old input");
                    session.record_turn_user_message("current-turn", "current input");
                    save_chat_session(&core, &session);

                    let canceled = cancel_orphaned_running_turns_in_session_store(
                        &core,
                        &session.id,
                        "current-turn",
                    )
                    .unwrap();

                    assert_eq!(canceled, 1);
                    let stored = load_chat_session(&core, &session.id);
                    let old_turn = stored
                        .turns
                        .iter()
                        .find(|turn| turn.id == "old-turn")
                        .expect("old turn");
                    assert_eq!(old_turn.status, types::ChatTurnStatus::Canceled);
                    let current_turn = stored
                        .turns
                        .iter()
                        .find(|turn| turn.id == "current-turn")
                        .expect("current turn");
                    assert_eq!(current_turn.status, types::ChatTurnStatus::Running);
                }

                #[tokio::test]
                async fn session_reply_sender_buffers_message_and_emits_ack_frame() {
                    let (core, _temp) = create_test_core().await;
                    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                    let turn_id = "reply-turn".to_string();
                    session.record_turn_user_message(&turn_id, "hello");
                    save_chat_session(&core, &session);
                    let buffer = Arc::new(Mutex::new(VecDeque::new()));
                    let (tx, mut rx) = mpsc::unbounded_channel::<StreamFrame>();
                    let sender = SessionReplySender::new(
                        core.clone(),
                        session.id.clone(),
                        turn_id.clone(),
                        buffer.clone(),
                        Some(tx),
                    );
                    ReplySender::send(&sender, "Working on it".to_string())
                        .await
                        .unwrap();

                    let mut guard = buffer.lock().await;
                    assert_eq!(guard.pop_front(), Some("Working on it".to_string()));
                    drop(guard);

                    let frame = rx.recv().await.expect("progress stream frame");
                    match frame {
                        StreamFrame::Progress { content } => assert_eq!(content, "Working on it"),
                        _ => panic!("expected progress frame"),
                    }
                    let stored = load_chat_session(&core, &session.id);
                    assert!(stored.turns[0].events.iter().any(|event| {
                        matches!(
                            &event.kind,
                            ChatTurnEventKind::Progress { message }
                                if message == "Working on it"
                        )
                    }));
                }

                #[tokio::test]
                async fn session_reply_sender_suppresses_ack_when_turn_is_terminal() {
                    let (core, _temp) = create_test_core().await;
                    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                    let turn_id = "reply-canceled-turn".to_string();
                    session.record_turn_user_message(&turn_id, "hello");
                    session.cancel_turn(&turn_id);
                    save_chat_session(&core, &session);
                    let buffer = Arc::new(Mutex::new(VecDeque::new()));
                    let (tx, mut rx) = mpsc::unbounded_channel::<StreamFrame>();
                    let sender = SessionReplySender::new(
                        core.clone(),
                        session.id.clone(),
                        turn_id.clone(),
                        buffer.clone(),
                        Some(tx),
                    );

                    ReplySender::send(&sender, "late answer".to_string())
                        .await
                        .unwrap();

                    assert!(buffer.lock().await.is_empty());
                    assert!(rx.try_recv().is_err());
                    let stored = load_chat_session(&core, &session.id);
                    let turn = stored
                        .turns
                        .iter()
                        .find(|turn| turn.id == turn_id)
                        .expect("turn");
                    assert_eq!(turn.status, ChatTurnStatus::Canceled);
                    assert!(!turn.events.iter().any(|event| {
                        matches!(
                            &event.kind,
                            ChatTurnEventKind::Progress { message }
                                if message == "late answer"
                        )
                    }));
                }

                #[tokio::test]
                async fn session_reply_sender_ignores_blank_messages() {
                    let (core, _temp) = create_test_core().await;
                    let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                    save_chat_session(&core, &session);
                    let buffer = Arc::new(Mutex::new(VecDeque::new()));
                    let (tx, mut rx) = mpsc::unbounded_channel::<StreamFrame>();
                    let sender = SessionReplySender::new(
                        core,
                        session.id,
                        "blank-turn".to_string(),
                        buffer.clone(),
                        Some(tx),
                    );
                    ReplySender::send(&sender, "   ".to_string()).await.unwrap();

                    let guard = buffer.lock().await;
                    assert!(guard.is_empty());
                    drop(guard);

                    assert!(rx.try_recv().is_err());
                }

                #[tokio::test]
                async fn ipc_stream_emitter_persists_assistant_segments_before_tools() {
                    let (core, _temp) = create_test_core().await;
                    let mut session =
                        ChatSession::new("agent-1".to_string(), "deepseek-chat".to_string());
                    let turn_id = "turn-stream-segments".to_string();
                    session.record_turn_user_message(&turn_id, "run tools");
                    save_chat_session(&core, &session);
                    let (tx, _rx) = mpsc::unbounded_channel::<StreamFrame>();
                    let mut emitter = IpcStreamEmitter::new(
                        core.clone(),
                        session.id.clone(),
                        turn_id.clone(),
                        tx,
                        Arc::new(AtomicBool::new(false)),
                    );

                    emitter.emit_text_delta("Planning first.").await;
                    emitter
                        .emit_tool_call_start("call-1", "bash", "{\"command\":\"pwd\"}")
                        .await;
                    emitter.emit_text_delta("Done.").await;
                    emitter.emit_complete().await;

                    let stored = SessionService::from_storage(&core.storage)
                        .get_session_view(&session.id)
                        .unwrap()
                        .expect("stored session");
                    let turn = stored
                        .turns
                        .iter()
                        .find(|turn| turn.id == turn_id)
                        .expect("stored turn");
                    assert!(matches!(
                        &turn.events[1].kind,
                        ChatTurnEventKind::AssistantMessage { content } if content == "Planning first."
                    ));
                    assert!(matches!(
                        &turn.events[2].kind,
                        ChatTurnEventKind::ToolCall { call_id, .. } if call_id == "call-1"
                    ));
                    assert!(matches!(
                        &turn.events[3].kind,
                        ChatTurnEventKind::AssistantMessage { content } if content == "Done."
                    ));
                }

                #[tokio::test]
                async fn ipc_stream_emitter_suppresses_frames_when_turn_is_terminal() {
                    let (core, _temp) = create_test_core().await;
                    let mut session =
                        ChatSession::new("agent-1".to_string(), "deepseek-chat".to_string());
                    let turn_id = "turn-stream-terminal".to_string();
                    session.record_turn_user_message(&turn_id, "run tools");
                    session.cancel_turn(&turn_id);
                    save_chat_session(&core, &session);
                    let (tx, mut rx) = mpsc::unbounded_channel::<StreamFrame>();
                    let has_text_streamed = Arc::new(AtomicBool::new(false));
                    let mut emitter = IpcStreamEmitter::new(
                        core.clone(),
                        session.id.clone(),
                        turn_id.clone(),
                        tx,
                        has_text_streamed.clone(),
                    );

                    emitter.emit_text_delta("late data").await;
                    emitter
                        .emit_tool_call_start("call-late", "bash", "{\"command\":\"pwd\"}")
                        .await;
                    emitter
                        .emit_tool_call_result("call-late", "bash", "pwd output", true)
                        .await;
                    emitter.emit_complete().await;

                    assert!(!has_text_streamed.load(Ordering::Relaxed));
                    assert!(rx.try_recv().is_err());
                    let stored = SessionService::from_storage(&core.storage)
                        .get_session_view(&session.id)
                        .unwrap()
                        .expect("stored session");
                    let turn = stored
                        .turns
                        .iter()
                        .find(|turn| turn.id == turn_id)
                        .expect("turn");
                    assert_eq!(turn.status, ChatTurnStatus::Canceled);
                    assert!(!turn.events.iter().any(|event| {
                        matches!(
                            &event.kind,
                            ChatTurnEventKind::AssistantMessage { content }
                                if content == "late data"
                        ) || matches!(
                            &event.kind,
                            ChatTurnEventKind::ToolCall { call_id, .. }
                                if call_id == "call-late"
                        ) || matches!(
                            &event.kind,
                            ChatTurnEventKind::ToolResult { call_id, .. }
                                if call_id == "call-late"
                        )
                    }));
                }

                #[tokio::test]
                async fn ipc_stream_emitter_only_emits_persisted_text_delta() {
                    let (core, _temp) = create_test_core().await;
                    let mut session =
                        ChatSession::new("agent-1".to_string(), "deepseek-chat".to_string());
                    let turn_id = "turn-stream-exact-delta".to_string();
                    session.record_turn_user_message(&turn_id, "stream exact");
                    save_chat_session(&core, &session);
                    let (tx, mut rx) = mpsc::unbounded_channel::<StreamFrame>();
                    let mut emitter = IpcStreamEmitter::new(
                        core.clone(),
                        session.id.clone(),
                        turn_id.clone(),
                        tx,
                        Arc::new(AtomicBool::new(false)),
                    );

                    emitter.emit_text_delta("   ").await;
                    assert!(rx.try_recv().is_err());

                    emitter.emit_text_delta("hello  ").await;

                    match rx.recv().await.expect("data frame") {
                        StreamFrame::Data { content } => assert_eq!(content, "   hello  "),
                        frame => panic!("expected data frame, got {frame:?}"),
                    }

                    let stored = SessionService::from_storage(&core.storage)
                        .get_session_view(&session.id)
                        .unwrap()
                        .expect("stored session");
                    let turn = stored
                        .turns
                        .iter()
                        .find(|turn| turn.id == turn_id)
                        .expect("turn");
                    assert!(turn.events.iter().any(|event| {
                        matches!(
                            &event.kind,
                            ChatTurnEventKind::AssistantMessage { content }
                                if content == "   hello  "
                        )
                    }));
                    assert!(!turn.events.iter().any(|event| {
                        matches!(
                            &event.kind,
                            ChatTurnEventKind::AssistantMessage { content }
                                if content.trim().is_empty()
                        )
                    }));
                }

                #[tokio::test]
                async fn ipc_stream_emitter_persists_partial_assistant_segment_on_drop() {
                    let (core, _temp) = create_test_core().await;
                    let mut session =
                        ChatSession::new("agent-1".to_string(), "deepseek-chat".to_string());
                    let turn_id = "turn-stream-cancel".to_string();
                    session.record_turn_user_message(&turn_id, "cancel stream");
                    save_chat_session(&core, &session);
                    let (tx, _rx) = mpsc::unbounded_channel::<StreamFrame>();
                    let mut emitter = IpcStreamEmitter::new(
                        core.clone(),
                        session.id.clone(),
                        turn_id.clone(),
                        tx,
                        Arc::new(AtomicBool::new(false)),
                    );

                    emitter.emit_text_delta("partial answer").await;
                    drop(emitter);

                    let stored = SessionService::from_storage(&core.storage)
                        .get_session_view(&session.id)
                        .unwrap()
                        .expect("stored session");
                    let turn = stored
                        .turns
                        .iter()
                        .find(|turn| turn.id == turn_id)
                        .expect("stored turn");
                    assert!(matches!(
                        &turn.events[1].kind,
                        ChatTurnEventKind::AssistantMessage { content } if content == "partial answer"
                    ));
                }

                #[tokio::test]
                async fn cancel_chat_stream_persists_partial_assistant_before_canceled_event() {
                    let (core, _temp) = create_test_core().await;
                    let mut session =
                        ChatSession::new("agent-1".to_string(), "deepseek-chat".to_string());
                    let turn_id = "turn-stream-cancel-order".to_string();
                    session.record_turn_user_message(&turn_id, "cancel stream");
                    save_chat_session(&core, &session);
                    let session_id = session.id.clone();
                    let (tx, _rx) = mpsc::unbounded_channel::<StreamFrame>();
                    let (emitted_tx, emitted_rx) = tokio::sync::oneshot::channel::<()>();
                    let worker_core = core.clone();
                    let worker_session_id = session_id.clone();
                    let worker_turn_id = turn_id.clone();
                    let handle = tokio::spawn(async move {
                        let mut emitter = IpcStreamEmitter::new(
                            worker_core,
                            worker_session_id,
                            worker_turn_id,
                            tx,
                            Arc::new(AtomicBool::new(false)),
                        );
                        emitter.emit_text_delta("partial answer").await;
                        let _ = emitted_tx.send(());
                        std::future::pending::<()>().await;
                    });
                    active_chat_streams()
                        .lock()
                        .await
                        .insert(turn_id.clone(), handle);
                    active_chat_stream_sessions().lock().await.insert(
                        session_id.clone(),
                        ActiveChatStreamBinding::new(turn_id.clone(), turn_id.clone(), None),
                    );
                    emitted_rx.await.expect("worker emitted partial answer");

                    assert!(
                        cancel_chat_stream(&core, &turn_id, None, None)
                            .await
                            .unwrap()
                    );

                    let stored = SessionService::from_storage(&core.storage)
                        .get_session_view(&session_id)
                        .unwrap()
                        .expect("stored session");
                    let turn = stored
                        .turns
                        .iter()
                        .find(|turn| turn.id == turn_id)
                        .expect("stored turn");
                    assert_eq!(turn.status, ChatTurnStatus::Canceled);
                    assert!(matches!(
                        &turn.events[1].kind,
                        ChatTurnEventKind::AssistantMessage { content } if content == "partial answer"
                    ));
                    assert!(matches!(turn.events[2].kind, ChatTurnEventKind::Canceled));
                }

                #[tokio::test]
                async fn cancel_chat_stream_keeps_runtime_when_cancel_persistence_fails() {
                    let (core, _temp) = create_test_core().await;
                    let session_id = format!("missing-session-{}", Uuid::new_v4());
                    let stream_id = format!("stream-{}", Uuid::new_v4());
                    let handle = tokio::spawn(async {
                        std::future::pending::<()>().await;
                    });
                    active_chat_streams()
                        .lock()
                        .await
                        .insert(stream_id.clone(), handle);
                    active_chat_stream_sessions().lock().await.insert(
                        session_id.clone(),
                        ActiveChatStreamBinding::new(stream_id.clone(), stream_id.clone(), None),
                    );

                    let error = cancel_chat_stream(&core, &stream_id, Some(&session_id), None)
                        .await
                        .expect_err("missing session should fail cancel persistence");

                    assert!(error.to_string().contains(types::SESSION_NOT_FOUND));
                    let handle = active_chat_streams()
                        .lock()
                        .await
                        .remove(&stream_id)
                        .expect("runtime should remain active until cancel is persisted");
                    handle.abort();
                    let _ = handle.await;
                    active_chat_stream_sessions()
                        .lock()
                        .await
                        .remove(&session_id);
                }

                #[test]
                fn daemon_stream_open_error_frame_marks_unsupported_stream_as_protocol_error() {
                    let frame = stream_open_error_frame(anyhow::anyhow!(
                        "Foreground chat streaming runs in the TUI process"
                    ));

                    match frame {
                        StreamFrame::Error(error) => {
                            assert_eq!(error.code, -3);
                            assert_eq!(error.kind, types::ErrorKind::Protocol);
                            assert_eq!(
                                error.message,
                                "Foreground chat streaming runs in the TUI process"
                            );
                        }
                        other => panic!("expected stream error frame, got {other:?}"),
                    }
                }

                #[tokio::test]
                async fn foreground_chat_stream_reports_missing_session_without_daemon_ipc() {
                    let (core, _temp) = create_test_core().await;
                    let mut rx = open_foreground_chat_session_stream_for_scope(
                        core.clone(),
                        "missing-session".to_string(),
                        Some("hello".to_string()),
                        "turn-missing".to_string(),
                        None,
                        types::ExecutionScope::foreground("test-client", "test-terminal"),
                    )
                    .await
                    .expect("foreground stream");

                    let first = rx.recv().await.expect("start frame");
                    assert!(matches!(first, StreamFrame::Start { .. }));
                    let second = rx.recv().await.expect("error frame");
                    assert!(matches!(
                        second,
                        StreamFrame::Error(error) if error.message == types::SESSION_NOT_FOUND
                    ));
                }

                #[tokio::test]
                async fn legacy_foreground_helpers_keep_unscoped_signatures() {
                    let (core, _temp) = create_test_core().await;
                    let mut rx = open_foreground_chat_session_stream(
                        core.clone(),
                        "missing-session".to_string(),
                        Some("hello".to_string()),
                        "turn-missing-legacy".to_string(),
                        None,
                    )
                    .await
                    .expect("foreground stream");

                    assert!(matches!(rx.recv().await, Some(StreamFrame::Start { .. })));
                    assert!(matches!(
                        rx.recv().await,
                        Some(StreamFrame::Error(error)) if error.message == types::SESSION_NOT_FOUND
                    ));
                    assert!(
                        !steer_foreground_chat_stream(&core, "missing-session", "continue").await
                    );
                }

                #[tokio::test]
                async fn cancel_before_stream_registration_persists_canceled_user_turn() {
                    let (core, _temp) = create_test_core().await;
                    let mut session =
                        ChatSession::new("agent-1".to_string(), "openai:gpt-5".to_string());
                    save_chat_session(&core, &session);
                    let session_id = session.id.clone();
                    let stream_id = "pending-cancel-stream".to_string();
                    let scope = types::ExecutionScope::foreground("test-client", "test-terminal");

                    assert!(
                        cancel_chat_stream(&core, &stream_id, Some(&session_id), Some(&scope))
                            .await
                            .unwrap()
                    );

                    let mut rx = open_foreground_chat_session_stream_for_scope(
                        core.clone(),
                        session_id.clone(),
                        Some("hello before start".to_string()),
                        stream_id.clone(),
                        None,
                        scope,
                    )
                    .await
                    .expect("foreground stream");

                    assert!(matches!(rx.recv().await, Some(StreamFrame::Start { .. })));
                    assert!(matches!(rx.recv().await, Some(StreamFrame::Done { .. })));
                    assert!(!active_chat_streams().lock().await.contains_key(&stream_id));

                    session = load_chat_session(&core, &session_id);
                    let turn = session
                        .turns
                        .iter()
                        .find(|turn| turn.id == stream_id)
                        .expect("canceled turn");
                    assert_eq!(turn.status, ChatTurnStatus::Canceled);
                    assert!(matches!(
                        &turn.events[0].kind,
                        ChatTurnEventKind::UserMessage { content } if content == "hello before start"
                    ));
                    assert!(matches!(turn.events[1].kind, ChatTurnEventKind::Canceled));
                }

                #[tokio::test]
                async fn cancel_bound_stream_before_handle_registration_is_not_lost() {
                    let (core, _temp) = create_test_core().await;
                    let mut session =
                        ChatSession::new("agent-1".to_string(), "openai:gpt-5".to_string());
                    save_chat_session(&core, &session);
                    let session_id = session.id.clone();
                    let stream_id = "bound-without-handle".to_string();
                    let scope = types::ExecutionScope::foreground("test-client", "test-terminal");
                    active_chat_stream_sessions().lock().await.insert(
                        session_id.clone(),
                        ActiveChatStreamBinding::new(
                            stream_id.clone(),
                            stream_id.clone(),
                            Some(scope.clone()),
                        )
                        .with_user_input(Some("hello while binding".to_string())),
                    );

                    assert!(
                        cancel_chat_stream(&core, &stream_id, Some(&session_id), Some(&scope))
                            .await
                            .unwrap()
                    );

                    assert!(
                        !active_chat_stream_sessions()
                            .lock()
                            .await
                            .contains_key(&session_id)
                    );
                    assert!(
                        pending_canceled_chat_streams()
                            .lock()
                            .await
                            .contains_key(&stream_id)
                    );
                    session = load_chat_session(&core, &session_id);
                    let turn = session
                        .turns
                        .iter()
                        .find(|turn| turn.id == stream_id)
                        .expect("canceled turn");
                    assert_eq!(turn.status, ChatTurnStatus::Canceled);
                    assert!(matches!(
                        &turn.events[0].kind,
                        ChatTurnEventKind::UserMessage { content } if content == "hello while binding"
                    ));
                    assert!(matches!(turn.events[1].kind, ChatTurnEventKind::Canceled));
                    pending_canceled_chat_streams()
                        .lock()
                        .await
                        .remove(&stream_id);
                }

                #[tokio::test]
                async fn late_pending_cancel_does_not_mutate_completed_turn() {
                    let (core, _temp) = create_test_core().await;
                    let mut session =
                        ChatSession::new("agent-1".to_string(), "openai:gpt-5".to_string());
                    let turn_id = "completed-before-cancel".to_string();
                    session.add_message(ChatMessage::user("hello"));
                    session.add_message(ChatMessage::assistant("done"));
                    session.record_turn_user_message(&turn_id, "hello");
                    session.complete_turn_with_assistant_message(&turn_id, "done");
                    save_chat_session(&core, &session);
                    let message_count = session.messages.len();
                    let event_count = session.turns[0].events.len();

                    runtime::cancel_pending_turn_in_session_store(
                        &core,
                        &session.id,
                        &turn_id,
                        Some("hello"),
                    )
                    .unwrap();

                    let stored = load_chat_session(&core, &session.id);
                    let turn = stored
                        .turns
                        .iter()
                        .find(|turn| turn.id == turn_id)
                        .expect("turn");
                    assert_eq!(turn.status, ChatTurnStatus::Completed);
                    assert_eq!(stored.messages.len(), message_count);
                    assert_eq!(turn.events.len(), event_count);
                }

                #[tokio::test]
                async fn cancel_pending_turn_preserves_raw_user_input() {
                    let (core, _temp) = create_test_core().await;
                    let session =
                        ChatSession::new("agent-1".to_string(), "openai:gpt-5".to_string());
                    save_chat_session(&core, &session);
                    let raw_input = "  indented\n";

                    runtime::cancel_pending_turn_in_session_store(
                        &core,
                        &session.id,
                        "turn-with-spaces",
                        Some(raw_input),
                    )
                    .unwrap();

                    let stored = load_chat_session(&core, &session.id);
                    assert_eq!(stored.messages.len(), 1);
                    assert_eq!(stored.messages[0].content, raw_input);
                    let turn = stored
                        .turns
                        .iter()
                        .find(|turn| turn.id == "turn-with-spaces")
                        .expect("turn");
                    assert_eq!(turn.status, ChatTurnStatus::Canceled);
                    assert!(matches!(
                        &turn.events[0].kind,
                        ChatTurnEventKind::UserMessage { content } if content == raw_input
                    ));
                }

                #[test]
                fn completed_streaming_turn_does_not_append_aggregate_assistant_event() {
                    let mut session =
                        ChatSession::new("agent-1".to_string(), "openai:gpt-5".to_string());
                    session.record_turn_user_message("turn-1", "hello");
                    session.record_turn_event(
                        "turn-1",
                        ChatTurnEventKind::AssistantMessage {
                            content: "he".to_string(),
                        },
                    );
                    session.record_turn_event(
                        "turn-1",
                        ChatTurnEventKind::AssistantMessage {
                            content: "llo".to_string(),
                        },
                    );
                    let streamed = std::sync::atomic::AtomicBool::new(true);

                    complete_turn_after_execution(&mut session, "turn-1", "hello", &streamed);

                    let turn = session
                        .turns
                        .iter()
                        .find(|turn| turn.id == "turn-1")
                        .expect("turn");
                    let assistant_events = turn
                        .events
                        .iter()
                        .filter(|event| {
                            matches!(event.kind, ChatTurnEventKind::AssistantMessage { .. })
                        })
                        .count();
                    assert_eq!(turn.status, ChatTurnStatus::Completed);
                    assert_eq!(assistant_events, 2);
                }

                #[test]
                fn reply_ack_does_not_suppress_final_assistant_event() {
                    let mut session =
                        ChatSession::new("agent-1".to_string(), "openai:gpt-5".to_string());
                    session.record_turn_user_message("turn-1", "hello");
                    session.record_turn_event(
                        "turn-1",
                        ChatTurnEventKind::Progress {
                            message: "Working on it.".to_string(),
                        },
                    );
                    let streamed_delta = std::sync::atomic::AtomicBool::new(false);

                    complete_turn_after_execution(
                        &mut session,
                        "turn-1",
                        "Final answer.",
                        &streamed_delta,
                    );

                    let turn = session
                        .turns
                        .iter()
                        .find(|turn| turn.id == "turn-1")
                        .expect("turn");
                    assert_eq!(turn.status, ChatTurnStatus::Completed);
                    assert!(turn.events.iter().any(|event| {
                        matches!(
                            &event.kind,
                            ChatTurnEventKind::Progress { message }
                                if message == "Working on it."
                        )
                    }));
                    assert!(turn.events.iter().any(|event| {
                        matches!(
                            &event.kind,
                            ChatTurnEventKind::AssistantMessage { content }
                                if content == "Final answer."
                        )
                    }));
                }

                #[tokio::test]
                async fn foreground_lease_failure_does_not_leave_session_binding() {
                    let (core, _temp) = create_test_core().await;
                    let session_id = format!("session-{}", Uuid::new_v4());
                    let scope = types::ExecutionScope::foreground("test-client", "test-terminal");
                    let _held_lease =
                        ForegroundSessionLease::acquire(&session_id, &Some(scope.clone()))
                            .expect("lease")
                            .expect("foreground lease");

                    let error = open_foreground_chat_session_stream_for_scope(
                        core,
                        session_id.clone(),
                        Some("hello".to_string()),
                        "turn-held-lease".to_string(),
                        None,
                        scope,
                    )
                    .await
                    .expect_err("duplicate foreground lease should fail");

                    assert!(error.to_string().contains("active foreground stream"));
                    assert!(
                        !active_chat_stream_sessions()
                            .lock()
                            .await
                            .contains_key(&session_id)
                    );
                }

                #[tokio::test]
                async fn same_owner_foreground_stream_replaces_previous_lease() {
                    let (core, _temp) = create_test_core().await;
                    let mut session =
                        ChatSession::new("agent-1".to_string(), "deepseek-chat".to_string());
                    session.record_turn_user_message("old-stream", "old input");
                    save_chat_session(&core, &session);
                    let session_id = session.id.clone();
                    let old_stream_id = "old-stream".to_string();
                    let scope = types::ExecutionScope::foreground("test-client", "test-terminal");
                    let lease = ForegroundSessionLease::acquire(&session_id, &Some(scope.clone()))
                        .expect("lease")
                        .expect("foreground lease");
                    let handle = tokio::spawn(async move {
                        let _lease = lease;
                        std::future::pending::<()>().await;
                    });
                    active_chat_streams()
                        .lock()
                        .await
                        .insert(old_stream_id.clone(), handle);
                    active_chat_stream_sessions().lock().await.insert(
                        session_id.clone(),
                        ActiveChatStreamBinding::new(
                            old_stream_id.clone(),
                            old_stream_id.clone(),
                            Some(scope.clone()),
                        ),
                    );

                    let mut rx = open_foreground_chat_session_stream_for_scope(
                        core.clone(),
                        session_id.clone(),
                        Some("hello".to_string()),
                        "new-stream".to_string(),
                        None,
                        scope,
                    )
                    .await
                    .expect("same owner replacement should acquire foreground lease");

                    assert!(matches!(rx.recv().await, Some(StreamFrame::Start { .. })));
                    assert!(
                        !active_chat_streams()
                            .lock()
                            .await
                            .contains_key(&old_stream_id)
                    );
                    let stored = load_chat_session(&core, &session_id);
                    let old_turn = stored
                        .turns
                        .iter()
                        .find(|turn| turn.id == old_stream_id)
                        .expect("old turn");
                    assert_eq!(old_turn.status, ChatTurnStatus::Canceled);
                }

                #[tokio::test]
                async fn same_owner_replacement_keeps_old_stream_when_cancel_persistence_fails() {
                    let (core, _temp) = create_test_core().await;
                    let session_id = format!("missing-session-{}", Uuid::new_v4());
                    let old_stream_id = "old-stream-missing-session".to_string();
                    let scope = types::ExecutionScope::foreground("test-client", "test-terminal");
                    let handle = tokio::spawn(async {
                        std::future::pending::<()>().await;
                    });
                    active_chat_streams()
                        .lock()
                        .await
                        .insert(old_stream_id.clone(), handle);
                    active_chat_stream_sessions().lock().await.insert(
                        session_id.clone(),
                        ActiveChatStreamBinding::new(
                            old_stream_id.clone(),
                            old_stream_id.clone(),
                            Some(scope.clone()),
                        ),
                    );

                    let error = open_foreground_chat_session_stream_for_scope(
                        core,
                        session_id.clone(),
                        Some("hello".to_string()),
                        "new-stream-after-failed-cancel".to_string(),
                        None,
                        scope,
                    )
                    .await
                    .expect_err("missing session should fail replacement cancel persistence");

                    assert!(error.to_string().contains(types::SESSION_NOT_FOUND));
                    let handle = active_chat_streams()
                        .lock()
                        .await
                        .remove(&old_stream_id)
                        .expect("old runtime should remain active until cancel is persisted");
                    handle.abort();
                    let _ = handle.await;
                    active_chat_stream_sessions()
                        .lock()
                        .await
                        .remove(&session_id);
                }

                #[tokio::test]
                async fn same_stream_id_different_owner_cannot_replace_foreground_stream() {
                    let (core, _temp) = create_test_core().await;
                    let mut session =
                        ChatSession::new("agent-1".to_string(), "deepseek-chat".to_string());
                    let stream_id = "shared-stream".to_string();
                    session.record_turn_user_message(&stream_id, "owner input");
                    save_chat_session(&core, &session);
                    let session_id = session.id.clone();
                    let owner = types::ExecutionScope::foreground("owner", "terminal-a");
                    let intruder = types::ExecutionScope::foreground("intruder", "terminal-b");
                    let lease = ForegroundSessionLease::acquire(&session_id, &Some(owner.clone()))
                        .expect("lease")
                        .expect("foreground lease");
                    let handle = tokio::spawn(async move {
                        let _lease = lease;
                        std::future::pending::<()>().await;
                    });
                    active_chat_streams()
                        .lock()
                        .await
                        .insert(stream_id.clone(), handle);
                    active_chat_stream_sessions().lock().await.insert(
                        session_id.clone(),
                        ActiveChatStreamBinding::new(
                            stream_id.clone(),
                            stream_id.clone(),
                            Some(owner.clone()),
                        ),
                    );

                    let error = open_foreground_chat_session_stream_for_scope(
                        core.clone(),
                        session_id.clone(),
                        Some("hello".to_string()),
                        stream_id.clone(),
                        None,
                        intruder.clone(),
                    )
                    .await
                    .expect_err("different owner should not replace stream");

                    assert!(error.to_string().contains("owned by another client"));
                    assert!(active_chat_streams().lock().await.contains_key(&stream_id));
                    assert!(
                        active_chat_stream_sessions()
                            .lock()
                            .await
                            .contains_key(&session_id)
                    );
                    assert!(
                        !cancel_chat_stream(&core, &stream_id, Some(&session_id), Some(&intruder))
                            .await
                            .unwrap()
                    );
                    assert!(active_chat_streams().lock().await.contains_key(&stream_id));
                    assert!(
                        cancel_chat_stream(&core, &stream_id, Some(&session_id), Some(&owner))
                            .await
                            .unwrap()
                    );
                }

                #[tokio::test]
                async fn append_message_assigns_missing_message_id() {
                    let (core, _temp) = create_test_core().await;
                    let runtime_tool_registry = OnceLock::new();
                    let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                    save_chat_session(&core, &session);

                    let response = IpcServer::process(
                        &core,
                        &runtime_tool_registry,
                        IpcRequest::AppendMessage {
                            session_id: session.id.clone(),
                            message: types::contracts::request::ChatMessage {
                                id: String::new(),
                                role: ChatRole::User,
                                content: "hello".to_string(),
                                timestamp: 1,
                                execution: None,
                                media: None,
                                transcript: None,
                            },
                        },
                    )
                    .await;

                    let session = match response {
                        IpcResponse::Success(value) => {
                            serde_json::from_value::<ChatSession>(value).expect("chat session")
                        }
                        other => panic!("expected success response, got {other:?}"),
                    };
                    assert_eq!(session.messages.len(), 1);
                    assert!(!session.messages[0].id.is_empty());
                    let stored = load_chat_session(&core, &session.id);
                    assert_eq!(stored.messages[0].id, session.messages[0].id);
                }

                #[tokio::test]
                async fn add_message_returns_not_found_for_missing_session() {
                    let (core, _temp) = create_test_core().await;
                    let runtime_tool_registry = OnceLock::new();

                    let response = IpcServer::process(
                        &core,
                        &runtime_tool_registry,
                        IpcRequest::AddMessage {
                            session_id: "missing-session".to_string(),
                            role: ChatRole::User,
                            content: "hello".to_string(),
                        },
                    )
                    .await;

                    match response {
                        IpcResponse::Error(error) => {
                            assert_eq!(error.code, 404);
                            assert_eq!(error.kind, types::ErrorKind::NotFound);
                            assert!(error.message.contains("Session"));
                        }
                        other => panic!("expected error response, got {other:?}"),
                    }
                }
            }
            mod system {
                use super::*;
                use crate::daemon::request_mapper::to_contract;
                use types::request::{AgentNode as ContractAgentNode, WireModelRef};
                #[tokio::test]
                async fn process_get_system_info_returns_pid() {
                    let (core, _temp) = create_test_core().await;
                    let runtime_tool_registry = OnceLock::new();

                    let response = IpcServer::process(
                        &core,
                        &runtime_tool_registry,
                        IpcRequest::GetSystemInfo,
                    )
                    .await;

                    match response {
                        IpcResponse::Success(value) => {
                            let pid = value
                                .get("pid")
                                .and_then(|value| value.as_u64())
                                .expect("pid");
                            assert!(pid > 0);
                        }
                        other => panic!("expected success response, got {other:?}"),
                    }
                }

                #[tokio::test]
                async fn process_build_agent_system_prompt_returns_prompt_payload() {
                    let (core, _temp) = create_test_core().await;
                    let runtime_tool_registry = OnceLock::new();

                    let response = IpcServer::process(
                        &core,
                        &runtime_tool_registry,
                        IpcRequest::BuildAgentSystemPrompt {
                            agent_node: to_contract(AgentNode::new().with_prompt("Base prompt"))
                                .expect("contract agent node"),
                        },
                    )
                    .await;

                    match response {
                        IpcResponse::Success(value) => {
                            let prompt = value
                                .get("prompt")
                                .and_then(|value| value.as_str())
                                .expect("prompt");
                            assert!(prompt.contains("Base prompt"));
                        }
                        other => panic!("expected success response, got {other:?}"),
                    }
                }

                #[tokio::test]
                async fn process_build_agent_system_prompt_rejects_invalid_model_ref() {
                    let (core, _temp) = create_test_core().await;
                    let runtime_tool_registry = OnceLock::new();

                    let response = IpcServer::process(
                        &core,
                        &runtime_tool_registry,
                        IpcRequest::BuildAgentSystemPrompt {
                            agent_node: ContractAgentNode {
                                model_ref: Some(WireModelRef {
                                    provider: "openai".to_string(),
                                    model: "missing-model".to_string(),
                                }),
                                ..ContractAgentNode::default()
                            },
                        },
                    )
                    .await;

                    match response {
                        IpcResponse::Error(error) => {
                            assert_eq!(error.code, 400);
                            assert_eq!(error.kind, types::ErrorKind::Validation);
                            let details = error.details.expect("validation details");
                            assert_eq!(details["type"], "validation_error");
                            assert_eq!(details["errors"][0]["field"], "model_ref.model");
                        }
                        other => panic!("expected validation error, got {other:?}"),
                    }
                }

                #[tokio::test]
                async fn process_get_available_models_returns_openai_catalog_when_secret_exists() {
                    let (core, _temp) = create_test_core().await;
                    core.storage
                        .secrets
                        .set_secret("OPENAI_API_KEY", "test-openai-key", None)
                        .expect("store openai key");
                    let runtime_tool_registry = OnceLock::new();

                    let response = IpcServer::process(
                        &core,
                        &runtime_tool_registry,
                        IpcRequest::GetAvailableModels,
                    )
                    .await;

                    match response {
                        IpcResponse::Success(value) => {
                            let models: Vec<types::ModelMetadataDTO> =
                                serde_json::from_value(value).expect("model catalog");
                            assert!(
                                models
                                    .iter()
                                    .any(|model| model.provider == types::Provider::OpenAI)
                            );
                            assert!(
                                models
                                    .iter()
                                    .any(|model| model.model == types::ModelId::Gpt5_4)
                            );
                            assert!(
                                !models
                                    .iter()
                                    .any(|model| model.model == types::ModelId::Gpt5_1)
                            );
                            assert!(
                                !models
                                    .iter()
                                    .any(|model| model.model == types::ModelId::Gpt5_2)
                            );
                            assert!(
                                !models
                                    .iter()
                                    .any(|model| model.provider == types::Provider::OpenAI
                                        && model.model == types::ModelId::CodexCli)
                            );
                            assert!(
                                !models
                                    .iter()
                                    .any(|model| model.model == types::ModelId::OpenCodeCli)
                            );
                            assert!(
                                models
                                    .iter()
                                    .any(|model| model.provider == types::Provider::Codex
                                        && model.model == types::ModelId::Gpt5_4Codex)
                            );
                        }
                        other => panic!("expected success response, got {other:?}"),
                    }
                }

                #[tokio::test]
                async fn process_get_available_models_returns_minimax_m27_catalog_when_secret_exists()
                 {
                    let (core, _temp) = create_test_core().await;
                    core.storage
                        .secrets
                        .set_secret("MINIMAX_API_KEY", "test-minimax-key", None)
                        .expect("store minimax key");
                    let runtime_tool_registry = OnceLock::new();

                    let response = IpcServer::process(
                        &core,
                        &runtime_tool_registry,
                        IpcRequest::GetAvailableModels,
                    )
                    .await;

                    match response {
                        IpcResponse::Success(value) => {
                            let models: Vec<types::ModelMetadataDTO> =
                                serde_json::from_value(value).expect("model catalog");
                            assert!(
                                models
                                    .iter()
                                    .any(|model| model.provider == types::Provider::MiniMax)
                            );
                            assert!(
                                models
                                    .iter()
                                    .any(|model| model.model == types::ModelId::MiniMaxM27)
                            );
                            assert!(models.iter().any(|model| {
                                model.model == types::ModelId::MiniMaxM27Highspeed
                            }));
                        }
                        other => panic!("expected success response, got {other:?}"),
                    }
                }

                #[tokio::test]
                async fn process_get_available_models_returns_codex_catalog_without_secret() {
                    let (core, _temp) = create_test_core().await;
                    let runtime_tool_registry = OnceLock::new();

                    let response = IpcServer::process(
                        &core,
                        &runtime_tool_registry,
                        IpcRequest::GetAvailableModels,
                    )
                    .await;

                    match response {
                        IpcResponse::Success(value) => {
                            let models: Vec<types::ModelMetadataDTO> =
                                serde_json::from_value(value).expect("model catalog");
                            assert!(
                                models
                                    .iter()
                                    .any(|model| model.provider == types::Provider::Codex
                                        && model.model == types::ModelId::Gpt5_4Codex)
                            );
                        }
                        other => panic!("expected success response, got {other:?}"),
                    }
                }

                #[tokio::test]
                async fn process_get_available_models_returns_all_configured_catalog_groups() {
                    let (core, _temp) = create_test_core().await;
                    core.storage
                        .secrets
                        .set_secret("OPENAI_API_KEY", "test-openai-key", None)
                        .expect("store openai key");
                    core.storage
                        .secrets
                        .set_secret("MINIMAX_CODING_PLAN_API_KEY", "test-minimax-key", None)
                        .expect("store minimax key");
                    core.storage
                        .secrets
                        .set_secret("ZAI_CODING_PLAN_API_KEY", "test-zai-key", None)
                        .expect("store zai key");

                    let runtime_tool_registry = OnceLock::new();
                    let response = IpcServer::process(
                        &core,
                        &runtime_tool_registry,
                        IpcRequest::GetAvailableModels,
                    )
                    .await;

                    match response {
                        IpcResponse::Success(value) => {
                            let models: Vec<types::ModelMetadataDTO> =
                                serde_json::from_value(value).expect("model catalog");
                            let providers: std::collections::HashSet<_> =
                                models.iter().map(|model| model.provider).collect();
                            assert_eq!(
                                providers,
                                std::collections::HashSet::from([
                                    types::Provider::Codex,
                                    types::Provider::OpenAI,
                                    types::Provider::MiniMaxCodingPlan,
                                    types::Provider::ZaiCodingPlan,
                                ])
                            );
                            assert!(models.iter().any(|model| {
                                model.model == types::ModelId::MiniMaxM25CodingPlanHighspeed
                            }));
                        }
                        other => panic!("expected success response, got {other:?}"),
                    }
                }
            }
        }
    }
    mod launcher {
        use crate::daemon::ipc_client;
        use crate::daemon::process::{DaemonConfig, ProcessManager};
        use anyhow::Result;
        use std::time::Duration;
        use tracing::{debug, info, warn};

        #[derive(Debug, Clone, PartialEq)]
        pub enum DaemonStatus {
            Running { pid: u32 },
            NotRunning,
            Stale { pid: u32 },
        }

        pub fn check_daemon_status() -> Result<DaemonStatus> {
            let pid_path = restflow_core::paths::daemon_pid_path()?;
            if !pid_path.exists() {
                return Ok(DaemonStatus::NotRunning);
            }

            let pid_str = std::fs::read_to_string(&pid_path)?;
            let pid: u32 = pid_str.trim().parse()?;

            if is_process_alive(pid) {
                Ok(DaemonStatus::Running { pid })
            } else {
                let _ = std::fs::remove_file(&pid_path);
                Ok(DaemonStatus::Stale { pid })
            }
        }

        pub fn start_daemon() -> Result<u32> {
            start_daemon_with_config(DaemonConfig)
        }

        pub fn start_daemon_with_config(config: DaemonConfig) -> Result<u32> {
            let manager = ProcessManager::new()?;
            let pid = manager.start(config)?;
            info!(pid, "Daemon started in background");
            Ok(pid)
        }

        pub fn stop_daemon() -> Result<bool> {
            let stopped = ProcessManager::new()?.stop()?;
            if stopped {
                info!("Stopped daemon");
            }
            Ok(stopped)
        }

        pub async fn ensure_daemon_running() -> Result<()> {
            ensure_daemon_running_with_config(DaemonConfig).await
        }

        pub async fn ensure_daemon_running_with_config(config: DaemonConfig) -> Result<()> {
            let socket_path = restflow_core::paths::socket_path()?;
            if ipc_client::is_daemon_available(&socket_path).await {
                debug!("Daemon already running");
                return Ok(());
            }

            match check_daemon_status()? {
                DaemonStatus::Running { pid } => {
                    debug!(pid, "Daemon process exists, waiting for socket");
                    for _ in 0..10 {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        if ipc_client::is_daemon_available(&socket_path).await {
                            return Ok(());
                        }
                    }
                    warn!(
                        "Daemon running but socket unavailable or protocol-incompatible; restarting"
                    );
                    let _ = ProcessManager::new()?.stop()?;
                    let report = super::recovery::recover().await?;
                    if !report.is_clean() {
                        info!("Recovered stale daemon state before restart: {}", report);
                    }
                    start_daemon_with_config(config)?;
                    for _ in 0..600 {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        if ipc_client::is_daemon_available(&socket_path).await {
                            info!("Daemon restarted successfully");
                            return Ok(());
                        }
                    }
                    anyhow::bail!("Daemon failed to restart within timeout");
                }
                DaemonStatus::NotRunning | DaemonStatus::Stale { .. } => {
                    // Clean up any stale artifacts before attempting to start.
                    let report = super::recovery::recover().await?;
                    if !report.is_clean() {
                        info!("Recovered stale daemon state before auto-start: {}", report);
                    }
                    info!("Starting daemon automatically");
                    start_daemon_with_config(config)?;
                    for _ in 0..600 {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        if ipc_client::is_daemon_available(&socket_path).await {
                            info!("Daemon started successfully");
                            return Ok(());
                        }
                    }
                    anyhow::bail!("Daemon failed to start within timeout");
                }
            }
        }

        fn is_process_alive(pid: u32) -> bool {
            #[cfg(unix)]
            {
                use nix::sys::signal::kill;
                use nix::unistd::Pid;
                let Ok(pid_i32) = i32::try_from(pid) else {
                    return false;
                };
                kill(Pid::from_raw(pid_i32), None).is_ok()
            }

            #[cfg(not(unix))]
            {
                use std::process::Command;
                Command::new("tasklist")
                    .args(["/FI", &format!("PID eq {}", pid)])
                    .output()
                    .map(|output| {
                        String::from_utf8_lossy(&output.stdout).contains(&pid.to_string())
                    })
                    .unwrap_or(false)
            }
        }
    }
    mod process {
        #[cfg(unix)]
        use anyhow::Context;
        use anyhow::Result;
        use std::ffi::OsString;
        use std::fs::File;
        use std::path::PathBuf;
        use std::process::{Command, Stdio};
        use std::time::Duration;
        use tracing::{debug, warn};

        #[cfg(unix)]
        fn pid_to_unix_pid(pid: u32) -> Result<nix::unistd::Pid> {
            let pid_i32 =
                i32::try_from(pid).with_context(|| format!("PID {} exceeds i32 range", pid))?;
            Ok(nix::unistd::Pid::from_raw(pid_i32))
        }

        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct DaemonConfig;

        impl Default for DaemonConfig {
            fn default() -> Self {
                Self
            }
        }

        pub struct ProcessManager {
            pid_file: PathBuf,
            log_dir: PathBuf,
        }

        impl ProcessManager {
            pub fn new() -> Result<Self> {
                Ok(Self {
                    pid_file: restflow_core::paths::daemon_pid_path()?,
                    log_dir: restflow_core::paths::logs_dir()?,
                })
            }

            pub fn start(&self, config: DaemonConfig) -> Result<u32> {
                if let Some(pid) = self.get_running_pid()? {
                    return Ok(pid);
                }

                let exe = std::env::current_exe()?;
                let mut cmd = Command::new(exe);
                cmd.current_dir(daemon_working_dir()?);
                cmd.args(["daemon", "start", "--foreground"]);
                let _ = config;

                std::fs::create_dir_all(&self.log_dir)?;
                let log_file = self.log_dir.join("daemon.log");
                let log = File::create(&log_file)?;
                cmd.stdout(log.try_clone()?);
                cmd.stderr(log);
                cmd.stdin(Stdio::null());
                self.configure_child_path(&mut cmd);

                #[cfg(unix)]
                {
                    use std::os::unix::process::CommandExt;
                    unsafe {
                        cmd.pre_exec(|| {
                            nix::unistd::setsid()
                                .map(|_| ())
                                .map_err(std::io::Error::other)
                        });
                    }
                }

                let mut child = cmd.spawn()?;
                let bootstrap_pid = child.id();

                // Detect immediate spawn failures before waiting for daemon.pid.
                std::thread::sleep(Duration::from_millis(150));
                if let Some(status) = child.try_wait()? {
                    anyhow::bail!("Daemon process exited early with status {}", status);
                }

                // daemon.pid is written by the daemon process itself after startup
                // succeeds. Wait for it and return the authoritative PID.
                for _ in 0..60 {
                    if let Some(pid) = self.get_running_pid()? {
                        return Ok(pid);
                    }

                    if let Some(status) = child.try_wait()? {
                        anyhow::bail!(
                            "Daemon process exited during startup with status {}",
                            status
                        );
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }

                warn!(
                    bootstrap_pid,
                    "Daemon started but PID file not available yet; returning bootstrap pid"
                );
                Ok(bootstrap_pid)
            }

            pub fn stop(&self) -> Result<bool> {
                if let Some(pid) = self.get_running_pid()? {
                    #[cfg(unix)]
                    {
                        use nix::sys::signal::{Signal, kill};
                        let signal_pid = pid_to_unix_pid(pid)?;
                        kill(signal_pid, Signal::SIGTERM)?;
                    }

                    #[cfg(not(unix))]
                    {
                        Command::new("taskkill")
                            .args(["/PID", &pid.to_string(), "/F"])
                            .output()?;
                    }

                    for _ in 0..50 {
                        if !self.is_process_alive(pid) {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }

                    let _ = std::fs::remove_file(&self.pid_file);
                    return Ok(true);
                }

                Ok(false)
            }

            pub fn get_running_pid(&self) -> Result<Option<u32>> {
                if !self.pid_file.exists() {
                    return Ok(None);
                }

                let pid_str = std::fs::read_to_string(&self.pid_file)?;
                let pid: u32 = match pid_str.trim().parse() {
                    Ok(pid) => pid,
                    Err(err) => {
                        warn!(
                            path = %self.pid_file.display(),
                            error = %err,
                            "Invalid daemon PID file contents; removing stale file"
                        );
                        let _ = std::fs::remove_file(&self.pid_file);
                        return Ok(None);
                    }
                };
                if self.is_process_alive(pid) {
                    Ok(Some(pid))
                } else {
                    let _ = std::fs::remove_file(&self.pid_file);
                    Ok(None)
                }
            }

            fn configure_child_path(&self, cmd: &mut Command) {
                if let Some(path) = build_daemon_child_path() {
                    cmd.env("PATH", &path);
                    debug!(path = %path.to_string_lossy(), "Configured daemon child PATH");
                }
            }

            fn is_process_alive(&self, pid: u32) -> bool {
                #[cfg(unix)]
                {
                    use nix::sys::signal::kill;
                    use nix::unistd::Pid;
                    let Ok(pid_i32) = i32::try_from(pid) else {
                        return false;
                    };
                    kill(Pid::from_raw(pid_i32), None).is_ok()
                }

                #[cfg(not(unix))]
                {
                    Command::new("tasklist")
                        .args(["/FI", &format!("PID eq {}", pid)])
                        .output()
                        .map(|output| {
                            String::from_utf8_lossy(&output.stdout).contains(&pid.to_string())
                        })
                        .unwrap_or(false)
                }
            }
        }

        fn daemon_working_dir() -> Result<PathBuf> {
            restflow_core::paths::ensure_restflow_dir()
        }

        fn build_daemon_child_path() -> Option<OsString> {
            let mut entries: Vec<PathBuf> = std::env::var_os("PATH")
                .map(|value| std::env::split_paths(&value).collect())
                .unwrap_or_default();

            append_default_exec_dirs(&mut entries);

            let unique_entries = unique_paths(entries);
            if unique_entries.is_empty() {
                return None;
            }

            std::env::join_paths(unique_entries).ok()
        }

        fn append_default_exec_dirs(entries: &mut Vec<PathBuf>) {
            let defaults = [
                "/opt/homebrew/bin",
                "/usr/local/bin",
                "/usr/bin",
                "/bin",
                "/usr/sbin",
                "/sbin",
            ];
            entries.extend(defaults.into_iter().map(PathBuf::from));

            if let Some(home) = dirs::home_dir() {
                entries.push(home.join(".local").join("bin"));
                entries.push(home.join(".npm-global").join("bin"));
            }
        }

        fn unique_paths(entries: Vec<PathBuf>) -> Vec<PathBuf> {
            use std::collections::HashSet;

            let mut seen = HashSet::new();
            let mut unique = Vec::new();
            for path in entries {
                if path.as_os_str().is_empty() {
                    continue;
                }
                if seen.insert(path.clone()) {
                    unique.push(path);
                }
            }
            unique
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use std::sync::{Mutex, OnceLock};

            fn env_lock() -> std::sync::MutexGuard<'static, ()> {
                static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
                LOCK.get_or_init(|| Mutex::new(()))
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
            }

            #[test]
            fn unique_paths_removes_duplicates() {
                let input = vec![
                    PathBuf::from("/usr/bin"),
                    PathBuf::from("/usr/bin"),
                    PathBuf::from("/opt/homebrew/bin"),
                ];
                let unique = unique_paths(input);
                assert_eq!(unique.len(), 2);
            }

            #[cfg(unix)]
            #[test]
            fn pid_to_unix_pid_rejects_out_of_range() {
                assert!(pid_to_unix_pid(i32::MAX as u32).is_ok());
                assert!(pid_to_unix_pid(i32::MAX as u32 + 1).is_err());
            }

            #[test]
            fn daemon_working_dir_uses_restflow_dir() {
                let _lock = env_lock();
                let temp = tempfile::tempdir().expect("tempdir");
                let prev = std::env::var_os("RESTFLOW_DIR");
                unsafe { std::env::set_var("RESTFLOW_DIR", temp.path()) };

                let dir = daemon_working_dir().expect("daemon working dir");
                assert_eq!(dir, temp.path());

                match prev {
                    Some(value) => unsafe { std::env::set_var("RESTFLOW_DIR", value) },
                    None => unsafe { std::env::remove_var("RESTFLOW_DIR") },
                }
            }
        }
    }
    pub mod recovery {
        use anyhow::Result;
        use std::fmt;
        use std::path::Path;
        use tracing::{debug, info, warn};

        /// Describes the state of daemon artifacts (PID file and socket).
        #[derive(Debug, Clone, PartialEq)]
        pub enum StaleState {
            /// Everything is healthy — an active daemon owns the artifacts.
            Healthy,
            /// PID file references a dead process.
            StalePid,
            /// Socket file exists but no daemon is listening.
            StaleSocket,
            /// Both PID file and socket are stale.
            Both,
            /// No artifacts present at all.
            Clean,
        }

        /// Evidence of what the recovery routine cleaned up.
        #[derive(Debug, Clone, Default)]
        pub struct RecoveryReport {
            pub removed_pid_file: bool,
            pub removed_socket: bool,
            pub stale_pid: Option<u32>,
        }

        impl RecoveryReport {
            /// Returns `true` when no cleanup was necessary.
            pub fn is_clean(&self) -> bool {
                !self.removed_pid_file && !self.removed_socket
            }
        }

        impl fmt::Display for RecoveryReport {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                if self.is_clean() {
                    return write!(f, "No stale artifacts found");
                }
                let mut parts = Vec::new();
                if self.removed_pid_file {
                    if let Some(pid) = self.stale_pid {
                        parts.push(format!("removed stale PID file (was PID {})", pid));
                    } else {
                        parts.push("removed stale PID file".to_string());
                    }
                }
                if self.removed_socket {
                    parts.push("removed stale socket".to_string());
                }
                write!(f, "Auto-cleaned: {}", parts.join(", "))
            }
        }

        /// Inspect daemon artifacts and determine their staleness.
        pub async fn inspect(pid_path: &Path, socket_path: &Path) -> Result<StaleState> {
            let pid_exists = pid_path.exists();
            let socket_exists = socket_path.exists();

            if !pid_exists && !socket_exists {
                return Ok(StaleState::Clean);
            }

            // Check if the PID file references a live process.
            let pid_alive = if pid_exists {
                match read_pid(pid_path) {
                    Some(pid) => is_process_alive(pid),
                    None => false, // Unparseable PID file is effectively stale.
                }
            } else {
                false
            };

            // Check if the socket is alive by attempting a connection.
            let socket_alive = if socket_exists {
                crate::daemon::ipc_client::is_daemon_available(socket_path).await
            } else {
                false
            };

            match (pid_alive, socket_alive) {
                (true, _) => Ok(StaleState::Healthy),
                (false, true) => {
                    // Socket responds but PID file is stale/missing — unusual but
                    // treat socket as authoritative; only the PID file is stale.
                    if pid_exists {
                        Ok(StaleState::StalePid)
                    } else {
                        Ok(StaleState::Healthy)
                    }
                }
                (false, false) => {
                    if pid_exists && socket_exists {
                        Ok(StaleState::Both)
                    } else if pid_exists {
                        Ok(StaleState::StalePid)
                    } else {
                        Ok(StaleState::StaleSocket)
                    }
                }
            }
        }

        /// Remove stale artifacts. Only deletes files that are verified stale —
        /// never touches files belonging to a healthy daemon.
        pub async fn recover() -> Result<RecoveryReport> {
            let pid_path = restflow_core::paths::daemon_pid_path()?;
            let socket_path = restflow_core::paths::socket_path()?;

            let state = inspect(&pid_path, &socket_path).await?;
            let mut report = RecoveryReport::default();

            match state {
                StaleState::Healthy | StaleState::Clean => {
                    debug!("No stale daemon artifacts to clean up");
                }
                StaleState::StalePid => {
                    report.stale_pid = read_pid(&pid_path);
                    report.removed_pid_file = remove_file_logged(&pid_path, "PID file");
                }
                StaleState::StaleSocket => {
                    report.removed_socket = remove_file_logged(&socket_path, "socket");
                }
                StaleState::Both => {
                    report.stale_pid = read_pid(&pid_path);
                    report.removed_pid_file = remove_file_logged(&pid_path, "PID file");
                    report.removed_socket = remove_file_logged(&socket_path, "socket");
                }
            }

            if !report.is_clean() {
                info!("{}", report);
            }

            Ok(report)
        }

        fn read_pid(path: &Path) -> Option<u32> {
            std::fs::read_to_string(path)
                .ok()
                .and_then(|s| s.trim().parse().ok())
        }

        fn is_process_alive(pid: u32) -> bool {
            #[cfg(unix)]
            {
                use nix::sys::signal::kill;
                use nix::unistd::Pid;
                let Ok(pid_i32) = i32::try_from(pid) else {
                    return false;
                };
                kill(Pid::from_raw(pid_i32), None).is_ok()
            }

            #[cfg(not(unix))]
            {
                use std::process::Command;
                Command::new("tasklist")
                    .args(["/FI", &format!("PID eq {}", pid)])
                    .output()
                    .map(|output| {
                        String::from_utf8_lossy(&output.stdout).contains(&pid.to_string())
                    })
                    .unwrap_or(false)
            }
        }

        fn remove_file_logged(path: &Path, label: &str) -> bool {
            match std::fs::remove_file(path) {
                Ok(()) => {
                    info!("Removed stale {}: {}", label, path.display());
                    return true;
                }
                Err(e) => warn!(
                    "Failed to remove stale {}: {} ({})",
                    label,
                    path.display(),
                    e
                ),
            }
            false
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use std::io::Write;
            use std::path::PathBuf;
            use tempfile::TempDir;

            fn make_paths(dir: &TempDir) -> (PathBuf, PathBuf) {
                let pid = dir.path().join("daemon.pid");
                let sock = dir.path().join("restflow.sock");
                (pid, sock)
            }

            #[tokio::test]
            async fn inspect_clean_returns_clean() {
                let dir = TempDir::new().unwrap();
                let (pid, sock) = make_paths(&dir);
                let state = inspect(&pid, &sock).await.unwrap();
                assert_eq!(state, StaleState::Clean);
            }

            #[tokio::test]
            async fn inspect_stale_pid_detected() {
                let dir = TempDir::new().unwrap();
                let (pid_path, sock_path) = make_paths(&dir);
                let mut f = std::fs::File::create(&pid_path).unwrap();
                write!(f, "999999999").unwrap();

                let state = inspect(&pid_path, &sock_path).await.unwrap();
                assert_eq!(state, StaleState::StalePid);
            }

            #[tokio::test]
            async fn inspect_stale_socket_detected() {
                let dir = TempDir::new().unwrap();
                let (pid_path, sock_path) = make_paths(&dir);
                std::fs::File::create(&sock_path).unwrap();

                let state = inspect(&pid_path, &sock_path).await.unwrap();
                assert_eq!(state, StaleState::StaleSocket);
            }

            #[tokio::test]
            async fn inspect_both_stale_detected() {
                let dir = TempDir::new().unwrap();
                let (pid_path, sock_path) = make_paths(&dir);
                let mut f = std::fs::File::create(&pid_path).unwrap();
                write!(f, "999999999").unwrap();
                std::fs::File::create(&sock_path).unwrap();

                let state = inspect(&pid_path, &sock_path).await.unwrap();
                assert_eq!(state, StaleState::Both);
            }

            #[test]
            fn recovery_report_display_clean() {
                let report = RecoveryReport::default();
                assert!(report.is_clean());
                assert_eq!(format!("{}", report), "No stale artifacts found");
            }

            #[test]
            fn recovery_report_display_removed() {
                let report = RecoveryReport {
                    removed_pid_file: true,
                    removed_socket: true,
                    stale_pid: Some(12345),
                };
                let s = format!("{}", report);
                assert!(s.contains("stale PID file"));
                assert!(s.contains("12345"));
                assert!(s.contains("stale socket"));
            }

            #[test]
            fn read_pid_valid() {
                let dir = TempDir::new().unwrap();
                let path = dir.path().join("test.pid");
                std::fs::write(&path, "42").unwrap();
                assert_eq!(read_pid(&path), Some(42));
            }

            #[test]
            fn read_pid_invalid() {
                let dir = TempDir::new().unwrap();
                let path = dir.path().join("test.pid");
                std::fs::write(&path, "not-a-number").unwrap();
                assert_eq!(read_pid(&path), None);
            }

            #[test]
            fn read_pid_missing() {
                let path = PathBuf::from("/tmp/nonexistent-pid-file-restflow-test");
                assert_eq!(read_pid(&path), None);
            }
        }
    }
    pub mod request_mapper {
        use anyhow::Context;
        use serde::Serialize;
        use serde::de::DeserializeOwned;

        use crate::daemon::IpcResponse;
        use types::{ValidationError, ValidationErrorResponse};

        pub fn to_contract<T, U>(value: T) -> anyhow::Result<U>
        where
            T: Serialize,
            U: DeserializeOwned,
        {
            let encoded =
                serde_json::to_value(value).context("failed to serialize core request payload")?;
            serde_json::from_value(encoded).context("failed to decode contract request payload")
        }

        pub fn from_contract<T, U>(value: T) -> anyhow::Result<U>
        where
            T: Serialize,
            U: DeserializeOwned,
        {
            let encoded = serde_json::to_value(value)
                .context("failed to serialize contract request payload")?;
            serde_json::from_value(encoded).context("failed to decode core request payload")
        }

        pub(crate) fn invalid_request_response(error: anyhow::Error) -> IpcResponse {
            IpcResponse::error(400, format!("Invalid request payload: {error:#}"))
        }

        pub(crate) fn invalid_validation_response(errors: Vec<ValidationError>) -> IpcResponse {
            let details = serde_json::to_value(ValidationErrorResponse::new(errors))
                .expect("validation error response should serialize");
            IpcResponse::error_with_details(400, "Validation failed", Some(details))
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use crate::daemon::IpcResponse;
            use serde::{Deserialize, Serialize};
            use types::{Skill, SkillSource};

            #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
            struct CorePayload {
                id: String,
                enabled: bool,
            }

            #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
            struct ContractPayload {
                id: String,
                enabled: bool,
            }

            #[test]
            fn to_contract_round_trips_same_shape() {
                let core = CorePayload {
                    id: "a".to_string(),
                    enabled: true,
                };

                let contract: ContractPayload = to_contract(core).unwrap();
                assert_eq!(
                    contract,
                    ContractPayload {
                        id: "a".to_string(),
                        enabled: true,
                    }
                );
            }

            #[test]
            fn from_contract_round_trips_same_shape() {
                let contract = ContractPayload {
                    id: "a".to_string(),
                    enabled: false,
                };

                let core: CorePayload = from_contract(contract).unwrap();
                assert_eq!(
                    core,
                    CorePayload {
                        id: "a".to_string(),
                        enabled: false,
                    }
                );
            }

            #[test]
            fn skill_contract_preserves_source_metadata() {
                let mut core_skill = Skill::new(
                    "skill-1".to_string(),
                    "Skill 1".to_string(),
                    Some("External skill".to_string()),
                    Some(vec!["external".to_string()]),
                    "# Skill".to_string(),
                );
                core_skill.source = SkillSource::External;
                core_skill.read_only = false;
                core_skill.source_ref = Some("marketplace:skill-1@1.0.0".to_string());
                core_skill.kind = Some("markdown".to_string());
                core_skill.executable = true;

                let contract: types::request::Skill = to_contract(core_skill.clone()).unwrap();
                assert_eq!(contract.kind.as_deref(), Some("markdown"));
                assert!(contract.executable);
                assert_eq!(
                    serde_json::to_value(contract.source).unwrap(),
                    serde_json::json!("external")
                );
                assert_eq!(
                    contract.source_ref.as_deref(),
                    Some("marketplace:skill-1@1.0.0")
                );

                let round_trip: Skill = from_contract(contract).unwrap();
                assert_eq!(round_trip.kind.as_deref(), Some("markdown"));
                assert!(round_trip.executable);
                assert_eq!(round_trip.source, SkillSource::External);
                assert_eq!(
                    round_trip.source_ref.as_deref(),
                    Some("marketplace:skill-1@1.0.0")
                );
            }

            #[test]
            fn invalid_validation_response_encodes_structured_details() {
                let response = invalid_validation_response(vec![types::ValidationError::new(
                    "model_ref.provider",
                    "unknown provider 'bad'",
                )]);

                match response {
                    IpcResponse::Error(error) => {
                        assert_eq!(error.code, 400);
                        assert_eq!(error.kind, types::ErrorKind::Validation);
                        assert_eq!(error.message, "Validation failed");
                        let details = error.details.expect("validation details");
                        assert_eq!(details["type"], "validation_error");
                        assert_eq!(details["errors"][0]["field"], "model_ref.provider");
                    }
                    other => panic!("expected error response, got {other:?}"),
                }
            }
        }
    }
    pub use core_access::CoreAccess;
    pub use health::{HealthChecker, HealthStatus, check_health};
    pub use ipc_client::{IpcClient, is_daemon_available};
    pub use ipc_protocol::{
        IPC_PROTOCOL_VERSION, IpcDaemonStatus, IpcRequest, IpcResponse, IpcStreamEvent,
        MAX_MESSAGE_SIZE, StreamFrame,
    };
    pub use ipc_server::{
        IpcServer, cancel_foreground_chat_stream, cancel_foreground_chat_stream_for_scope,
        open_foreground_chat_session_stream, open_foreground_chat_session_stream_for_scope,
        steer_foreground_chat_stream, steer_foreground_chat_stream_for_scope,
    };
    pub use launcher::{
        DaemonStatus, check_daemon_status, ensure_daemon_running,
        ensure_daemon_running_with_config, start_daemon, start_daemon_with_config, stop_daemon,
    };
    pub use process::{DaemonConfig, ProcessManager};
}

#[cfg(test)]
mod integration_tests {
    mod ipc_client_stub_parity {
        use std::collections::BTreeSet;
        use std::fs;
        use std::path::Path;

        fn parse_method_names(source: &str, prefix: &str) -> BTreeSet<String> {
            source
                .lines()
                .filter_map(|line| {
                    let trimmed = line.trim();
                    let rest = trimmed.strip_prefix(prefix)?;
                    let (name, _) = rest.split_once('(')?;
                    Some(name.trim().to_string())
                })
                .collect()
        }

        fn load_source(path: &Path) -> String {
            fs::read_to_string(path).unwrap_or_else(|error| {
                panic!("failed to read {}: {error}", path.display());
            })
        }

        #[test]
        fn non_unix_stub_covers_session_client_methods() {
            let ipc_client_source =
                load_source(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"));
            let unix_methods = parse_method_names(&ipc_client_source, "pub async fn ")
                .into_iter()
                .filter(|name| {
                    name.contains("session")
                        || matches!(
                            name.as_str(),
                            "count_sessions"
                                | "add_message"
                                | "append_message"
                                | "subscribe_session_events"
                        )
                })
                .collect::<BTreeSet<_>>();
            let mut unsupported_methods = parse_method_names(&ipc_client_source, "fn ");
            unsupported_methods.extend(parse_method_names(&ipc_client_source, "pub async fn "));
            unsupported_methods.remove("$name");

            let missing: Vec<_> = unix_methods
                .difference(&unsupported_methods)
                .cloned()
                .collect();

            assert!(
                missing.is_empty(),
                "unsupported IPC client is missing session methods: {}",
                missing.join(", ")
            );
        }
    }

    mod tool_agent_integration {
        use ::tools::{BashConfig, FileConfig, ToolRegistryBuilder};

        #[test]
        fn minimal_registry_excludes_external_capabilities() {
            let registry = ToolRegistryBuilder::new()
                .with_bash(BashConfig::default())
                .with_file(FileConfig::default())
                .build();

            assert!(registry.has("bash"));
            assert!(registry.has("file"));

            for tool_name in [
                "http_request",
                "send_email",
                "telegram_send",
                "discord_send",
                "slack_send",
                "browser",
                "web_search",
                "web_fetch",
                "jina_reader",
                "transcribe",
                "vision",
            ] {
                assert!(!registry.has(tool_name), "unexpected {tool_name}");
            }
        }
    }

    mod tool_subagent_lifecycle {
        use std::collections::HashMap;
        use std::sync::Arc;

        use ::agent::agent::{
            SubagentConfig, SubagentDefLookup, SubagentDefSnapshot, SubagentDefSummary,
            SubagentDeps, SubagentManagerImpl, SubagentTracker,
        };
        use ::agent::llm::{MockLlmClient, MockStep};
        use ::tools::{ListSubagentsTool, SpawnSubagentTool, WaitSubagentsTool};
        use serde_json::json;
        use tokio::sync::mpsc;
        use types::SubagentManager;
        use types::tool::Tool;
        use types::toolset::ToolRegistry;

        const TEST_PARENT_RUN_ID: &str = "parent-1";

        struct MockDefLookup {
            defs: HashMap<String, SubagentDefSnapshot>,
            summaries: Vec<SubagentDefSummary>,
        }

        impl MockDefLookup {
            fn with_agents(agents: Vec<(&str, &str)>) -> Self {
                let mut defs = HashMap::new();
                let mut summaries = Vec::new();
                for (id, name) in agents {
                    defs.insert(
                        id.to_string(),
                        SubagentDefSnapshot {
                            name: name.to_string(),
                            system_prompt: format!("You are a {name} agent."),
                            allowed_tools: vec![],
                            max_iterations: Some(1),
                            default_model: None,
                        },
                    );
                    summaries.push(SubagentDefSummary {
                        id: id.to_string(),
                        name: name.to_string(),
                        description: format!("{name} agent"),
                        tags: vec![],
                    });
                }
                Self { defs, summaries }
            }
        }

        impl SubagentDefLookup for MockDefLookup {
            fn lookup(&self, id: &str) -> Option<SubagentDefSnapshot> {
                self.defs.get(id).cloned()
            }

            fn list_callable(&self) -> Vec<SubagentDefSummary> {
                self.summaries.clone()
            }
        }

        fn make_shared_deps(
            agents: Vec<(&str, &str)>,
            mock_steps: Vec<MockStep>,
        ) -> Arc<dyn SubagentManager> {
            let (tx, rx) = mpsc::channel(32);
            let tracker = Arc::new(SubagentTracker::new(tx, rx));
            let definitions: Arc<dyn SubagentDefLookup> =
                Arc::new(MockDefLookup::with_agents(agents));
            let llm_client = Arc::new(MockLlmClient::from_steps("mock", mock_steps));
            let tool_registry = Arc::new(ToolRegistry::new());
            let config = SubagentConfig {
                max_parallel_agents: 10,
                subagent_timeout_secs: 30,
                max_iterations: 5,
                max_depth: 1,
            };
            let deps = Arc::new(SubagentDeps {
                tracker,
                definitions,
                llm_client,
                tool_registry,
                config,
                llm_client_factory: None,
                orchestrator: None,
            });
            Arc::new(SubagentManagerImpl::from_deps(&deps))
        }

        #[tokio::test]
        async fn spawn_then_wait_lifecycle() {
            let deps = make_shared_deps(
                vec![("researcher", "Researcher")],
                vec![MockStep::text("research complete")],
            );

            let spawn_tool = SpawnSubagentTool::new(deps.clone());
            let spawn_result = spawn_tool
                .execute(json!({
                    "agent": "researcher",
                    "task": "Find info",
                    "wait": false,
                    "parent_run_id": TEST_PARENT_RUN_ID
                }))
                .await
                .unwrap();
            assert!(spawn_result.success);
            assert_eq!(spawn_result.result["status"], "spawned");
            let task_id = spawn_result.result["task_id"].as_str().unwrap().to_string();

            let wait_tool = WaitSubagentsTool::new(deps);
            let wait_result = wait_tool
                .execute(json!({
                    "task_ids": [task_id],
                    "parent_run_id": TEST_PARENT_RUN_ID,
                    "timeout_secs": 10
                }))
                .await
                .unwrap();
            assert!(wait_result.success);
            let results = wait_result.result["results"].as_array().unwrap();
            assert_eq!(results.len(), 1);
            assert_eq!(results[0]["status"], "completed");
        }

        #[tokio::test]
        async fn spawn_then_list_shows_running() {
            let deps = make_shared_deps(
                vec![("coder", "Coder")],
                vec![MockStep::text("slow result").with_delay(5000)],
            );

            let spawn_tool = SpawnSubagentTool::new(deps.clone());
            let spawn_result = spawn_tool
                .execute(json!({
                    "agent": "coder",
                    "task": "Write code",
                    "wait": false,
                    "parent_run_id": TEST_PARENT_RUN_ID
                }))
                .await
                .unwrap();
            assert!(spawn_result.success);

            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            let list_tool = ListSubagentsTool::new(deps);
            let list_result = list_tool
                .execute(json!({"parent_run_id": TEST_PARENT_RUN_ID}))
                .await
                .unwrap();
            assert!(list_result.success);
            assert!(list_result.result["running_count"].as_u64().unwrap() >= 1);
        }

        #[tokio::test]
        async fn spawn_multiple_then_wait_all() {
            let deps = make_shared_deps(
                vec![("researcher", "Researcher"), ("coder", "Coder")],
                vec![
                    MockStep::text("result 1"),
                    MockStep::text("result 2"),
                    MockStep::text("result 3"),
                ],
            );

            let spawn_tool = SpawnSubagentTool::new(deps.clone());
            let mut task_ids = Vec::new();
            for (agent, task_desc) in [
                ("researcher", "task 1"),
                ("coder", "task 2"),
                ("researcher", "task 3"),
            ] {
                let result = spawn_tool
                    .execute(json!({
                        "agent": agent,
                        "task": task_desc,
                        "wait": false,
                        "parent_run_id": TEST_PARENT_RUN_ID
                    }))
                    .await
                    .unwrap();
                assert!(result.success);
                task_ids.push(result.result["task_id"].as_str().unwrap().to_string());
            }

            let wait_tool = WaitSubagentsTool::new(deps);
            let wait_result = wait_tool
                .execute(json!({
                    "task_ids": task_ids,
                    "parent_run_id": TEST_PARENT_RUN_ID,
                    "timeout_secs": 10
                }))
                .await
                .unwrap();
            assert!(wait_result.success);
            let results = wait_result.result["results"].as_array().unwrap();
            assert_eq!(results.len(), 3);
            for result in results {
                assert_eq!(result["status"], "completed");
            }
        }

        #[tokio::test]
        async fn spawn_unknown_agent_error() {
            let deps = make_shared_deps(vec![("coder", "Coder")], vec![]);

            let spawn_tool = SpawnSubagentTool::new(deps.clone());
            let result = spawn_tool
                .execute(json!({"agent": "nonexistent", "task": "impossible"}))
                .await;
            assert!(result.is_err());

            let list_tool = ListSubagentsTool::new(deps);
            let list_result = list_tool.execute(json!({})).await.unwrap();
            assert!(list_result.success);
            assert_eq!(list_result.result["running_count"], 0);
        }

        #[tokio::test]
        async fn spawn_wait_timeout_then_list() {
            let deps = make_shared_deps(
                vec![("coder", "Coder")],
                vec![MockStep::text("never").with_delay(60_000)],
            );

            let spawn_tool = SpawnSubagentTool::new(deps.clone());
            let spawn_result = spawn_tool
                .execute(json!({
                    "agent": "coder",
                    "task": "infinite task",
                    "wait": false,
                    "parent_run_id": TEST_PARENT_RUN_ID
                }))
                .await
                .unwrap();
            let task_id = spawn_result.result["task_id"].as_str().unwrap().to_string();

            let wait_tool = WaitSubagentsTool::new(deps.clone());
            let wait_result = wait_tool
                .execute(json!({
                    "task_ids": [task_id],
                    "parent_run_id": TEST_PARENT_RUN_ID,
                    "timeout_secs": 1
                }))
                .await
                .unwrap();
            assert!(wait_result.success);
            let results = wait_result.result["results"].as_array().unwrap();
            assert_eq!(results[0]["status"], "timeout");

            let list_tool = ListSubagentsTool::new(deps);
            let list_result = list_tool
                .execute(json!({"parent_run_id": TEST_PARENT_RUN_ID}))
                .await
                .unwrap();
            assert!(list_result.success);
            assert!(list_result.result["running_count"].as_u64().is_some());
        }
    }
}
