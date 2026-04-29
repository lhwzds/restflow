use super::*;

impl RestFlowMcpServer {
    pub(crate) async fn handle_list_agents(&self) -> Result<String, String> {
        let agents = self
            .backend
            .list_agents()
            .await
            .map_err(|e| format!("Failed to list agents: {}", e))?;

        let summaries: Vec<AgentSummary> = agents
            .into_iter()
            .map(|a| {
                let model_ref = a.agent.resolved_model_ref();
                AgentSummary {
                    id: a.id,
                    name: a.name,
                    model: model_ref
                        .map(|model_ref| model_ref.model.as_serialized_str().to_string())
                        .unwrap_or_else(|| "auto".to_string()),
                    provider: model_ref
                        .map(|model_ref| model_ref.provider.as_canonical_str().to_string())
                        .unwrap_or_else(|| "auto".to_string()),
                }
            })
            .collect();

        serde_json::to_string_pretty(&summaries)
            .map_err(|e| format!("Failed to serialize agents: {}", e))
    }

    pub(crate) async fn handle_get_agent(&self, params: GetAgentParams) -> Result<String, String> {
        let agent = self
            .backend
            .get_agent(&params.id)
            .await
            .map_err(|e| format!("Failed to get agent: {}", e))?;

        serde_json::to_string_pretty(&agent)
            .map_err(|e| format!("Failed to serialize agent: {}", e))
    }
}
