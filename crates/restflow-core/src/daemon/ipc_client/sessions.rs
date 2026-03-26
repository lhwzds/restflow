#[cfg(unix)]
use super::*;
#[cfg(unix)]
use crate::daemon::request_mapper::to_contract;
#[cfg(unix)]
use crate::{
    ExecutionLogResponse, ExecutionMetricsResponse, ExecutionTimeline, ProviderHealthQuery,
    ProviderHealthResponse,
};
#[cfg(unix)]
use restflow_contracts::{ArchiveResponse, DeleteResponse};

#[cfg(unix)]
impl IpcClient {
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

    pub async fn rebuild_external_session(&mut self, id: String) -> Result<ChatSession> {
        self.request_typed(IpcRequest::RebuildExternalSession { id })
            .await
    }

    pub async fn search_sessions(&mut self, query: String) -> Result<Vec<ChatSessionSummary>> {
        self.request_typed(IpcRequest::SearchSessions { query })
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
    ) -> Result<ChatSession> {
        self.request_typed(IpcRequest::ExecuteChatSession {
            session_id,
            user_input,
        })
        .await
    }
    pub async fn get_session_messages(
        &mut self,
        session_id: String,
        limit: Option<usize>,
    ) -> Result<Vec<ChatMessage>> {
        self.request_typed(IpcRequest::GetSessionMessages { session_id, limit })
            .await
    }

    pub async fn list_execution_sessions(
        &mut self,
        query: ExecutionSessionListQuery,
    ) -> Result<Vec<ExecutionSessionSummary>> {
        let query = to_contract(query)?;
        self.request_typed(IpcRequest::ListExecutionSessions { query })
            .await
    }

    pub async fn query_execution_traces(
        &mut self,
        query: ExecutionTraceQuery,
    ) -> Result<Vec<ExecutionTraceEvent>> {
        let query = to_contract(query)?;
        self.request_typed(IpcRequest::QueryExecutionTraces { query })
            .await
    }

    pub async fn get_execution_run_timeline(
        &mut self,
        run_id: String,
    ) -> Result<ExecutionTimeline> {
        self.request_typed(IpcRequest::GetExecutionRunTimeline { run_id })
            .await
    }

    pub async fn get_execution_run_metrics(
        &mut self,
        run_id: String,
    ) -> Result<ExecutionMetricsResponse> {
        self.request_typed(IpcRequest::GetExecutionRunMetrics { run_id })
            .await
    }

    pub async fn get_provider_health(
        &mut self,
        query: ProviderHealthQuery,
    ) -> Result<ProviderHealthResponse> {
        let query = to_contract(query)?;
        self.request_typed(IpcRequest::GetProviderHealth { query })
            .await
    }

    pub async fn query_execution_run_logs(
        &mut self,
        run_id: String,
    ) -> Result<ExecutionLogResponse> {
        self.request_typed(IpcRequest::QueryExecutionRunLogs { run_id })
            .await
    }

    pub async fn get_execution_trace_stats(
        &mut self,
        run_id: Option<String>,
    ) -> Result<ExecutionTraceStats> {
        self.request_typed(IpcRequest::GetExecutionTraceStats { run_id })
            .await
    }

    pub async fn get_execution_trace_by_id(
        &mut self,
        id: String,
    ) -> Result<Option<ExecutionTraceEvent>> {
        self.request_optional(IpcRequest::GetExecutionTraceById { id })
            .await
    }
}
