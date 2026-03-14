#[cfg(unix)]
use super::*;

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
        self.request_typed(IpcRequest::SearchMemoryRanked {
            query,
            min_score,
            scoring_preset,
        })
        .await
    }

    pub async fn get_memory_chunk(&mut self, id: String) -> Result<Option<MemoryChunk>> {
        match self.request(IpcRequest::GetMemoryChunk { id }).await? {
            IpcResponse::Success(value) => Ok(Some(serde_json::from_value(value)?)),
            IpcResponse::Error { code: 404, .. } => Ok(None),
            IpcResponse::Error {
                code,
                message,
                details,
            } => {
                bail!(Self::format_ipc_error(code, &message, details))
            }
            IpcResponse::Pong => bail!("Unexpected Pong response"),
        }
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
        #[derive(serde::Deserialize)]
        struct AddMemoryResponse {
            id: String,
        }
        let resp: AddMemoryResponse = self
            .request_typed(IpcRequest::AddMemory {
                content,
                agent_id,
                tags,
            })
            .await?;
        Ok(resp.id)
    }

    pub async fn create_memory_chunk(&mut self, chunk: MemoryChunk) -> Result<MemoryChunk> {
        self.request_typed(IpcRequest::CreateMemoryChunk { chunk })
            .await
    }

    pub async fn list_memory_by_session(&mut self, session_id: String) -> Result<Vec<MemoryChunk>> {
        self.request_typed(IpcRequest::ListMemoryBySession { session_id })
            .await
    }

    pub async fn delete_memory(&mut self, id: String) -> Result<bool> {
        #[derive(serde::Deserialize)]
        struct DeleteResponse {
            deleted: bool,
        }
        let resp: DeleteResponse = self.request_typed(IpcRequest::DeleteMemory { id }).await?;
        Ok(resp.deleted)
    }

    pub async fn clear_memory(&mut self, agent_id: Option<String>) -> Result<u32> {
        #[derive(serde::Deserialize)]
        struct ClearResponse {
            deleted: u32,
        }
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
        match self
            .request(IpcRequest::GetMemorySession { session_id })
            .await?
        {
            IpcResponse::Success(value) => Ok(Some(serde_json::from_value(value)?)),
            IpcResponse::Error { code: 404, .. } => Ok(None),
            IpcResponse::Error {
                code,
                message,
                details,
            } => {
                bail!(Self::format_ipc_error(code, &message, details))
            }
            IpcResponse::Pong => bail!("Unexpected Pong response"),
        }
    }

    pub async fn list_memory_sessions(&mut self, agent_id: String) -> Result<Vec<MemorySession>> {
        self.request_typed(IpcRequest::ListMemorySessions { agent_id })
            .await
    }

    pub async fn create_memory_session(&mut self, session: MemorySession) -> Result<MemorySession> {
        self.request_typed(IpcRequest::CreateMemorySession { session })
            .await
    }

    pub async fn delete_memory_session(
        &mut self,
        session_id: String,
        delete_chunks: bool,
    ) -> Result<bool> {
        #[derive(serde::Deserialize)]
        struct DeleteResponse {
            deleted: bool,
        }
        let resp: DeleteResponse = self
            .request_typed(IpcRequest::DeleteMemorySession {
                session_id,
                delete_chunks,
            })
            .await?;
        Ok(resp.deleted)
    }
}
