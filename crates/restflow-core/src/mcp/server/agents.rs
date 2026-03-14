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
            .map(|a| AgentSummary {
                id: a.id,
                name: a.name,
                model: serde_json::to_value(a.agent.model)
                    .ok()
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| format!("{:?}", a.agent.model)),
                provider: a
                    .agent
                    .model
                    .map(|model| model.provider().as_canonical_str().to_string())
                    .unwrap_or_else(|| "auto".to_string()),
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
