use super::*;

impl RestFlowMcpServer {
    pub(crate) async fn handle_chat_session_list(
        &self,
        params: ChatSessionListParams,
    ) -> Result<String, String> {
        let defaults = self.load_api_defaults().await?;
        let limit = params.limit.unwrap_or(defaults.session_list_limit).max(1) as usize;
        let summaries: Vec<ChatSessionSummary> = if let Some(agent_id) = params.agent_id {
            self.backend
                .list_sessions_by_agent(&agent_id)
                .await
                .map_err(|e| format!("Failed to list sessions: {}", e))?
                .into_iter()
                .take(limit)
                .collect()
        } else {
            self.backend
                .list_sessions()
                .await
                .map_err(|e| format!("Failed to list sessions: {}", e))?
                .into_iter()
                .take(limit)
                .collect()
        };

        serde_json::to_string_pretty(&summaries)
            .map_err(|e| format!("Failed to serialize sessions: {}", e))
    }

    pub(crate) async fn handle_chat_session_get(
        &self,
        params: ChatSessionGetParams,
    ) -> Result<String, String> {
        let session = self
            .backend
            .get_session(&params.session_id)
            .await
            .map_err(|e| Self::wrap_backend_error("Failed to get session", e))?;

        serde_json::to_string_pretty(&session)
            .map_err(|e| format!("Failed to serialize session: {}", e))
    }
}
