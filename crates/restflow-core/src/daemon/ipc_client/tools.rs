#[cfg(unix)]
use super::*;

#[cfg(unix)]
impl IpcClient {
    pub async fn build_agent_system_prompt(&mut self, agent_node: AgentNode) -> Result<String> {
        #[derive(serde::Deserialize)]
        struct PromptResponse {
            prompt: String,
        }
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
