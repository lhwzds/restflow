#[cfg(not(unix))]
use super::*;

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

    async fn request_typed<T: DeserializeOwned>(&mut self, _req: IpcRequest) -> Result<T> {
        Self::unsupported()
    }

    unsupported_result_methods! {
        fn search_memory(&mut self, _query: String, _agent_id: Option<String>, _limit: Option<u32>) -> MemorySearchResult;
        fn list_skills(&mut self) -> Vec<Skill>;
        fn get_skill(&mut self, _id: String) -> Option<Skill>;
        fn get_skill_reference(&mut self, _skill_id: String, _ref_id: String) -> Option<String>;
        fn create_skill(&mut self, _skill: Skill) -> ();
        fn update_skill(&mut self, _id: String, _skill: Skill) -> ();
        fn delete_skill(&mut self, _id: String) -> ();
        fn list_agents(&mut self) -> Vec<StoredAgent>;
        fn get_agent(&mut self, _id: String) -> StoredAgent;
        fn search_memory_ranked(&mut self, _query: crate::models::memory::MemorySearchQuery, _min_score: Option<f64>, _scoring_preset: Option<String>) -> crate::memory::RankedSearchResult;
        fn get_memory_chunk(&mut self, _id: String) -> Option<MemoryChunk>;
        fn list_memory(&mut self, _agent_id: Option<String>, _tag: Option<String>) -> Vec<MemoryChunk>;
        fn add_memory(&mut self, _content: String, _agent_id: Option<String>, _tags: Vec<String>) -> String;
        fn create_memory_chunk(&mut self, _chunk: MemoryChunk) -> MemoryChunk;
        fn list_memory_by_session(&mut self, _session_id: String) -> Vec<MemoryChunk>;
        fn delete_memory(&mut self, _id: String) -> bool;
        fn clear_memory(&mut self, _agent_id: Option<String>) -> u32;
        fn get_memory_stats(&mut self, _agent_id: Option<String>) -> MemoryStats;
        fn export_memory(&mut self, _agent_id: Option<String>) -> ExportResult;
        fn export_memory_session(&mut self, _session_id: String) -> ExportResult;
        fn export_memory_advanced(&mut self, _agent_id: String, _session_id: Option<String>, _preset: Option<String>, _include_metadata: Option<bool>, _include_timestamps: Option<bool>, _include_source: Option<bool>, _include_tags: Option<bool>) -> ExportResult;
        fn get_memory_session(&mut self, _session_id: String) -> Option<MemorySession>;
        fn list_memory_sessions(&mut self, _agent_id: String) -> Vec<MemorySession>;
        fn create_memory_session(&mut self, _session: MemorySession) -> MemorySession;
        fn delete_memory_session(&mut self, _session_id: String, _delete_chunks: bool) -> bool;
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
        fn search_sessions(&mut self, _query: String) -> Vec<ChatSessionSummary>;
        fn add_message(&mut self, _session_id: String, _role: ChatRole, _content: String) -> ChatSession;
        fn append_message(&mut self, _session_id: String, _message: ChatMessage) -> ChatSession;
        fn execute_chat_session(&mut self, _session_id: String, _user_input: Option<String>) -> ChatSession;
        fn cancel_chat_session_stream(&mut self, _stream_id: String) -> bool;
        fn steer_chat_session_stream(&mut self, _session_id: String, _instruction: String) -> bool;
        fn get_session_messages(&mut self, _session_id: String, _limit: Option<usize>) -> Vec<ChatMessage>;
        fn list_execution_sessions(&mut self, _query: ExecutionSessionListQuery) -> Vec<ExecutionSessionSummary>;
        fn query_execution_traces(&mut self, _query: ExecutionTraceQuery) -> Vec<ExecutionTraceEvent>;
        fn get_execution_trace_stats(&mut self, _run_id: Option<String>) -> ExecutionTraceStats;
        fn get_execution_run_timeline(&mut self, _run_id: String) -> ExecutionTimeline;
        fn get_execution_run_metrics(&mut self, _run_id: String) -> ExecutionMetricsResponse;
        fn query_execution_run_logs(&mut self, _run_id: String) -> ExecutionLogResponse;
        fn get_execution_trace_by_id(&mut self, _id: String) -> Option<ExecutionTraceEvent>;
        fn list_terminal_sessions(&mut self) -> Vec<TerminalSession>;
        fn get_terminal_session(&mut self, _id: String) -> TerminalSession;
        fn create_terminal_session(&mut self) -> TerminalSession;
        fn rename_terminal_session(&mut self, _id: String, _name: String) -> TerminalSession;
        fn update_terminal_session(&mut self, _id: String, _name: Option<String>, _working_directory: Option<String>, _startup_command: Option<String>) -> TerminalSession;
        fn save_terminal_session(&mut self, _session: TerminalSession) -> TerminalSession;
        fn delete_terminal_session(&mut self, _id: String) -> ();
        fn mark_all_terminal_sessions_stopped(&mut self) -> usize;
        fn list_auth_profiles(&mut self) -> Vec<AuthProfile>;
        fn get_auth_profile(&mut self, _id: String) -> AuthProfile;
        fn add_auth_profile(&mut self, _name: String, _credential: Credential, _source: CredentialSource, _provider: AuthProvider) -> AuthProfile;
        fn remove_auth_profile(&mut self, _id: String) -> AuthProfile;
        fn update_auth_profile(&mut self, _id: String, _updates: ProfileUpdate) -> AuthProfile;
        fn discover_auth(&mut self) -> crate::auth::DiscoverySummary;
        fn enable_auth_profile(&mut self, _id: String) -> ();
        fn disable_auth_profile(&mut self, _id: String, _reason: String) -> ();
        fn get_api_key(&mut self, _provider: AuthProvider) -> String;
        fn get_api_key_for_profile(&mut self, _id: String) -> String;
        fn test_auth_profile(&mut self, _id: String) -> bool;
        fn mark_auth_success(&mut self, _id: String) -> ();
        fn mark_auth_failure(&mut self, _id: String) -> ();
        fn clear_auth_profiles(&mut self) -> ();
        fn list_background_agents(&mut self, _status: Option<String>) -> Vec<BackgroundAgent>;
        fn get_background_agent(&mut self, _id: String) -> Option<BackgroundAgent>;
        fn create_background_agent(&mut self, _spec: BackgroundAgentSpec) -> BackgroundAgent;
        fn convert_session_to_background_agent(&mut self, _request: restflow_contracts::request::BackgroundAgentConvertSessionRequest) -> crate::models::BackgroundAgentConversionResult;
        fn update_background_agent(&mut self, _id: String, _patch: BackgroundAgentPatch) -> BackgroundAgent;
        fn delete_background_agent(&mut self, _id: String) -> restflow_contracts::DeleteWithIdResponse;
        fn control_background_agent(&mut self, _id: String, _action: BackgroundAgentControlAction) -> BackgroundAgent;
        fn get_background_agent_history(&mut self, _id: String) -> Vec<BackgroundAgentEvent>;
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
        _on_frame: F,
    ) -> Result<()>
    where
        F: FnMut(StreamFrame) -> Result<()>,
    {
        Self::unsupported()
    }

    pub async fn subscribe_background_agent_events<F>(
        &mut self,
        _background_agent_id: String,
        _on_event: F,
    ) -> Result<()>
    where
        F: FnMut(TaskStreamEvent) -> Result<()>,
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
