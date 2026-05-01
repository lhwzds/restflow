use super::*;

impl RestFlowMcpServer {
    pub(crate) async fn handle_memory_search(
        &self,
        params: MemorySearchParams,
    ) -> Result<String, String> {
        let defaults = self.load_api_defaults().await?;
        let limit = params.limit.unwrap_or(defaults.memory_search_limit).max(1);
        let query = MemorySearchQuery::new(params.agent_id)
            .with_query(params.query)
            .with_mode(SearchMode::Keyword)
            .paginate(limit, 0);

        let results = self
            .backend
            .search_memory(query)
            .await
            .map_err(|e| format!("Failed to search memory: {}", e))?;

        serde_json::to_string_pretty(&results)
            .map_err(|e| format!("Failed to serialize search results: {}", e))
    }

    pub(crate) async fn handle_memory_store(
        &self,
        params: MemoryStoreParams,
    ) -> Result<String, String> {
        let mut chunk =
            MemoryChunk::new(params.agent_id, params.content).with_source(MemorySource::ManualNote);

        if !params.tags.is_empty() {
            chunk = chunk.with_tags(params.tags);
        }

        let id = self
            .backend
            .store_memory(chunk)
            .await
            .map_err(|e| format!("Failed to store memory: {}", e))?;

        Ok(format!("Stored memory chunk: {}", id))
    }

    pub(crate) async fn handle_memory_stats(
        &self,
        params: MemoryStatsParams,
    ) -> Result<String, String> {
        let stats = self
            .backend
            .get_memory_stats(&params.agent_id)
            .await
            .map_err(|e| format!("Failed to load memory stats: {}", e))?;

        serde_json::to_string_pretty(&stats)
            .map_err(|e| format!("Failed to serialize memory stats: {}", e))
    }
}
