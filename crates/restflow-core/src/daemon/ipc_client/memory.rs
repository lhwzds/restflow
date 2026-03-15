#[cfg(unix)]
use super::*;
#[cfg(unix)]
use crate::daemon::request_mapper::to_contract;
#[cfg(unix)]
use restflow_contracts::{ClearResponse, DeleteResponse, IdResponse};

#[cfg(unix)]
impl IpcClient {
    pub async fn search_memory(
        &mut self,
        query: String,
        agent_id: Option<String>,
        limit: Option<u32>,
    ) -> Result<MemorySearchResult> {
        self.request_typed(IpcRequest::SearchMemory {
            query,
            agent_id,
            limit,
        })
        .await
    }
    pub async fn search_memory_ranked(
        &mut self,
        query: crate::models::memory::MemorySearchQuery,
        min_score: Option<f64>,
        scoring_preset: Option<String>,
    ) -> Result<crate::memory::RankedSearchResult> {
        let query = to_contract(query)?;
        self.request_typed(IpcRequest::SearchMemoryRanked {
            query,
            min_score,
            scoring_preset,
        })
        .await
    }

    pub async fn get_memory_chunk(&mut self, id: String) -> Result<Option<MemoryChunk>> {
        self.request_optional(IpcRequest::GetMemoryChunk { id })
            .await
    }

    pub async fn list_memory(
        &mut self,
        agent_id: Option<String>,
        tag: Option<String>,
    ) -> Result<Vec<MemoryChunk>> {
        self.request_typed(IpcRequest::ListMemory { agent_id, tag })
            .await
    }

    pub async fn add_memory(
        &mut self,
        content: String,
        agent_id: Option<String>,
        tags: Vec<String>,
    ) -> Result<String> {
        let resp: IdResponse = self
            .request_typed(IpcRequest::AddMemory {
                content,
                agent_id,
                tags,
            })
            .await?;
        Ok(resp.id)
    }

    pub async fn create_memory_chunk(&mut self, chunk: MemoryChunk) -> Result<MemoryChunk> {
        let chunk = to_contract(chunk)?;
        self.request_typed(IpcRequest::CreateMemoryChunk { chunk })
            .await
    }

    pub async fn list_memory_by_session(&mut self, session_id: String) -> Result<Vec<MemoryChunk>> {
        self.request_typed(IpcRequest::ListMemoryBySession { session_id })
            .await
    }

    pub async fn delete_memory(&mut self, id: String) -> Result<bool> {
        let resp: DeleteResponse = self.request_typed(IpcRequest::DeleteMemory { id }).await?;
        Ok(resp.deleted)
    }

    pub async fn clear_memory(&mut self, agent_id: Option<String>) -> Result<u32> {
        let resp: ClearResponse = self
            .request_typed(IpcRequest::ClearMemory { agent_id })
            .await?;
        Ok(resp.deleted)
    }

    pub async fn get_memory_stats(&mut self, agent_id: Option<String>) -> Result<MemoryStats> {
        self.request_typed(IpcRequest::GetMemoryStats { agent_id })
            .await
    }

    pub async fn export_memory(&mut self, agent_id: Option<String>) -> Result<ExportResult> {
        self.request_typed(IpcRequest::ExportMemory { agent_id })
            .await
    }

    pub async fn export_memory_session(&mut self, session_id: String) -> Result<ExportResult> {
        self.request_typed(IpcRequest::ExportMemorySession { session_id })
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn export_memory_advanced(
        &mut self,
        agent_id: String,
        session_id: Option<String>,
        preset: Option<String>,
        include_metadata: Option<bool>,
        include_timestamps: Option<bool>,
        include_source: Option<bool>,
        include_tags: Option<bool>,
    ) -> Result<ExportResult> {
        self.request_typed(IpcRequest::ExportMemoryAdvanced {
            agent_id,
            session_id,
            preset,
            include_metadata,
            include_timestamps,
            include_source,
            include_tags,
        })
        .await
    }

    pub async fn get_memory_session(
        &mut self,
        session_id: String,
    ) -> Result<Option<MemorySession>> {
        self.request_optional(IpcRequest::GetMemorySession { session_id })
            .await
    }

    pub async fn list_memory_sessions(&mut self, agent_id: String) -> Result<Vec<MemorySession>> {
        self.request_typed(IpcRequest::ListMemorySessions { agent_id })
            .await
    }

    pub async fn create_memory_session(&mut self, session: MemorySession) -> Result<MemorySession> {
        let session = to_contract(session)?;
        self.request_typed(IpcRequest::CreateMemorySession { session })
            .await
    }

    pub async fn delete_memory_session(
        &mut self,
        session_id: String,
        delete_chunks: bool,
    ) -> Result<bool> {
        let resp: DeleteResponse = self
            .request_typed(IpcRequest::DeleteMemorySession {
                session_id,
                delete_chunks,
            })
            .await?;
        Ok(resp.deleted)
    }
}
