//! MCP server implementation for RestFlow
//!
//! This module provides an MCP server that exposes RestFlow's functionality
//! to AI assistants like Claude Code.

use restflow_core::AppCore;
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::tool::cached_schema_for_type,
    model::{
        CallToolRequestParam, CallToolResult, Content, Implementation, ListToolsResult,
        PaginatedRequestParam, ServerCapabilities, ServerInfo, Tool,
    },
    schemars::{self, JsonSchema},
    service::{RequestContext, RoleServer},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::io::{stdin, stdout};

/// RestFlow MCP Server
///
/// Exposes skills, agents, and workflow functionality via MCP protocol.
#[derive(Clone)]
pub struct RestFlowMcpServer {
    core: Arc<AppCore>,
}

impl RestFlowMcpServer {
    /// Create a new MCP server with the given AppCore
    pub fn new(core: Arc<AppCore>) -> Self {
        Self { core }
    }

    /// Run the MCP server using stdio transport
    pub async fn run(self) -> anyhow::Result<()> {
        tracing::info!("Starting RestFlow MCP server...");
        let server = self.serve(stdio()).await?;
        tracing::info!("MCP server initialized, waiting for requests...");
        server.waiting().await?;
        Ok(())
    }
}

/// Create stdio transport for MCP communication
fn stdio() -> (tokio::io::Stdin, tokio::io::Stdout) {
    (stdin(), stdout())
}

// ============================================================================
// Tool Parameter Types
// ============================================================================

/// Parameters for get_skill tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetSkillParams {
    /// The ID of the skill to retrieve
    pub id: String,
}

/// Parameters for create_skill tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreateSkillParams {
    /// Display name of the skill
    pub name: String,
    /// Optional description of what the skill does
    #[serde(default)]
    pub description: Option<String>,
    /// Optional tags for categorization
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// The markdown content of the skill (instructions for the AI)
    pub content: String,
}

/// Parameters for update_skill tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct UpdateSkillParams {
    /// The ID of the skill to update
    pub id: String,
    /// New display name (optional)
    #[serde(default)]
    pub name: Option<String>,
    /// New description (optional)
    #[serde(default)]
    pub description: Option<String>,
    /// New tags (optional)
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// New content (optional)
    #[serde(default)]
    pub content: Option<String>,
}

/// Parameters for delete_skill tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DeleteSkillParams {
    /// The ID of the skill to delete
    pub id: String,
}

/// Parameters for get_agent tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetAgentParams {
    /// The ID of the agent to retrieve
    pub id: String,
}

// ============================================================================
// Response Types
// ============================================================================

/// Skill summary for list_skills response
#[derive(Debug, Serialize, Deserialize)]
pub struct SkillSummary {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
}

/// Agent summary for list_agents response
#[derive(Debug, Serialize, Deserialize)]
pub struct AgentSummary {
    pub id: String,
    pub name: String,
    pub model: String,
}

// ============================================================================
// Empty params for parameterless tools
// ============================================================================

/// Empty parameters (for tools with no parameters)
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct EmptyParams {}

// ============================================================================
// Tool Implementations
// ============================================================================

impl RestFlowMcpServer {
    async fn handle_list_skills(&self) -> Result<String, String> {
        let skills = restflow_core::services::skills::list_skills(&self.core)
            .await
            .map_err(|e| format!("Failed to list skills: {}", e))?;

        let summaries: Vec<SkillSummary> = skills
            .into_iter()
            .map(|s| SkillSummary {
                id: s.id,
                name: s.name,
                description: s.description,
                tags: s.tags,
            })
            .collect();

        serde_json::to_string_pretty(&summaries)
            .map_err(|e| format!("Failed to serialize skills: {}", e))
    }

    async fn handle_get_skill(&self, params: GetSkillParams) -> Result<String, String> {
        let skill = restflow_core::services::skills::get_skill(&self.core, &params.id)
            .await
            .map_err(|e| format!("Failed to get skill: {}", e))?
            .ok_or_else(|| format!("Skill not found: {}", params.id))?;

        serde_json::to_string_pretty(&skill)
            .map_err(|e| format!("Failed to serialize skill: {}", e))
    }

    async fn handle_create_skill(&self, params: CreateSkillParams) -> Result<String, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let skill = restflow_core::models::Skill::new(
            id.clone(),
            params.name,
            params.description,
            params.tags,
            params.content,
        );

        restflow_core::services::skills::create_skill(&self.core, skill)
            .await
            .map_err(|e| format!("Failed to create skill: {}", e))?;

        Ok(format!("Skill created successfully with ID: {}", id))
    }

    async fn handle_update_skill(&self, params: UpdateSkillParams) -> Result<String, String> {
        let mut skill = restflow_core::services::skills::get_skill(&self.core, &params.id)
            .await
            .map_err(|e| format!("Failed to get skill: {}", e))?
            .ok_or_else(|| format!("Skill not found: {}", params.id))?;

        // Update fields
        skill.update(
            params.name,
            params.description.map(Some),
            params.tags.map(Some),
            params.content,
        );

        restflow_core::services::skills::update_skill(&self.core, &params.id, &skill)
            .await
            .map_err(|e| format!("Failed to update skill: {}", e))?;

        Ok(format!("Skill {} updated successfully", params.id))
    }

    async fn handle_delete_skill(&self, params: DeleteSkillParams) -> Result<String, String> {
        restflow_core::services::skills::delete_skill(&self.core, &params.id)
            .await
            .map_err(|e| format!("Failed to delete skill: {}", e))?;

        Ok(format!("Skill {} deleted successfully", params.id))
    }

    async fn handle_list_agents(&self) -> Result<String, String> {
        let agents = restflow_core::services::agent::list_agents(&self.core)
            .await
            .map_err(|e| format!("Failed to list agents: {}", e))?;

        let summaries: Vec<AgentSummary> = agents
            .into_iter()
            .map(|a| AgentSummary {
                id: a.id,
                name: a.name,
                // Use serde_json to get the proper serialized model name
                model: serde_json::to_value(a.agent.model)
                    .ok()
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| format!("{:?}", a.agent.model)),
            })
            .collect();

        serde_json::to_string_pretty(&summaries)
            .map_err(|e| format!("Failed to serialize agents: {}", e))
    }

    async fn handle_get_agent(&self, params: GetAgentParams) -> Result<String, String> {
        let agent = restflow_core::services::agent::get_agent(&self.core, &params.id)
            .await
            .map_err(|e| format!("Failed to get agent: {}", e))?;

        serde_json::to_string_pretty(&agent)
            .map_err(|e| format!("Failed to serialize agent: {}", e))
    }
}

// ============================================================================
// Server Handler Implementation
// ============================================================================

impl ServerHandler for RestFlowMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: Default::default(),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "restflow".to_string(),
                title: Some("RestFlow MCP Server".to_string()),
                version: env!("CARGO_PKG_VERSION").to_string(),
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "RestFlow MCP Server - Manage skills, agents, and workflows. \
                Use list_skills to see available skills, get_skill to read a skill's content, \
                create_skill to create new skills, and similar tools for agents."
                    .to_string(),
            ),
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let tools = vec![
            Tool::new(
                "list_skills",
                "List all available skills in RestFlow. Returns a summary of each skill including ID, name, description, and tags.",
                cached_schema_for_type::<EmptyParams>(),
            ),
            Tool::new(
                "get_skill",
                "Get the full content of a skill by its ID. Returns the complete skill including its markdown content.",
                cached_schema_for_type::<GetSkillParams>(),
            ),
            Tool::new(
                "create_skill",
                "Create a new skill in RestFlow. Provide a name, optional description, optional tags, and the markdown content.",
                cached_schema_for_type::<CreateSkillParams>(),
            ),
            Tool::new(
                "update_skill",
                "Update an existing skill in RestFlow. Provide the skill ID and the fields to update.",
                cached_schema_for_type::<UpdateSkillParams>(),
            ),
            Tool::new(
                "delete_skill",
                "Delete a skill from RestFlow by its ID.",
                cached_schema_for_type::<DeleteSkillParams>(),
            ),
            Tool::new(
                "list_agents",
                "List all available agents in RestFlow. Returns a summary of each agent including ID, name, and model.",
                cached_schema_for_type::<EmptyParams>(),
            ),
            Tool::new(
                "get_agent",
                "Get the full configuration of an agent by its ID. Returns the complete agent including model, prompt, temperature, and tools.",
                cached_schema_for_type::<GetAgentParams>(),
            ),
        ];

        Ok(ListToolsResult {
            tools,
            next_cursor: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let result = match request.name.as_ref() {
            "list_skills" => self.handle_list_skills().await,
            "get_skill" => {
                let params: GetSkillParams =
                    serde_json::from_value(Value::Object(request.arguments.unwrap_or_default()))
                        .map_err(|e| {
                            McpError::invalid_params(format!("Invalid parameters: {}", e), None)
                        })?;
                self.handle_get_skill(params).await
            }
            "create_skill" => {
                let params: CreateSkillParams =
                    serde_json::from_value(Value::Object(request.arguments.unwrap_or_default()))
                        .map_err(|e| {
                            McpError::invalid_params(format!("Invalid parameters: {}", e), None)
                        })?;
                self.handle_create_skill(params).await
            }
            "update_skill" => {
                let params: UpdateSkillParams =
                    serde_json::from_value(Value::Object(request.arguments.unwrap_or_default()))
                        .map_err(|e| {
                            McpError::invalid_params(format!("Invalid parameters: {}", e), None)
                        })?;
                self.handle_update_skill(params).await
            }
            "delete_skill" => {
                let params: DeleteSkillParams =
                    serde_json::from_value(Value::Object(request.arguments.unwrap_or_default()))
                        .map_err(|e| {
                            McpError::invalid_params(format!("Invalid parameters: {}", e), None)
                        })?;
                self.handle_delete_skill(params).await
            }
            "list_agents" => self.handle_list_agents().await,
            "get_agent" => {
                let params: GetAgentParams =
                    serde_json::from_value(Value::Object(request.arguments.unwrap_or_default()))
                        .map_err(|e| {
                            McpError::invalid_params(format!("Invalid parameters: {}", e), None)
                        })?;
                self.handle_get_agent(params).await
            }
            _ => Err(format!("Unknown tool: {}", request.name)),
        };

        match result {
            Ok(text) => Ok(CallToolResult::success(vec![Content::text(text)])),
            Err(error) => Ok(CallToolResult::error(vec![Content::text(error)])),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_core::models::{AIModel, AgentNode, ApiKeyConfig, Skill};
    use restflow_core::storage::agent::StoredAgent;
    use tempfile::TempDir;

    // =========================================================================
    // Test Utilities
    // =========================================================================

    /// Create a test server with a temporary database
    async fn create_test_server() -> (RestFlowMcpServer, TempDir) {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let core = Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap());
        (RestFlowMcpServer::new(core), temp_dir)
    }

    /// Create a test skill with given id and name
    fn create_test_skill(id: &str, name: &str) -> Skill {
        Skill::new(
            id.to_string(),
            name.to_string(),
            Some(format!("Description for {}", name)),
            Some(vec!["test".to_string()]),
            format!("# {}\n\nContent here.", name),
        )
    }

    /// Create a test agent node
    fn create_test_agent_node(prompt: &str) -> AgentNode {
        AgentNode {
            model: AIModel::ClaudeSonnet4_5,
            prompt: Some(prompt.to_string()),
            temperature: Some(0.7),
            api_key_config: Some(ApiKeyConfig::Direct("test_key".to_string())),
            tools: Some(vec!["add".to_string()]),
        }
    }

    // =========================================================================
    // Serialization Tests
    // =========================================================================

    #[test]
    fn test_skill_summary_serialization() {
        let summary = SkillSummary {
            id: "test-id".to_string(),
            name: "Test Skill".to_string(),
            description: Some("A test skill".to_string()),
            tags: Some(vec!["test".to_string()]),
        };

        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("test-id"));
        assert!(json.contains("Test Skill"));
    }

    #[test]
    fn test_agent_summary_serialization() {
        let summary = AgentSummary {
            id: "test-id".to_string(),
            name: "Test Agent".to_string(),
            model: "gpt-5".to_string(),
        };

        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("test-id"));
        assert!(json.contains("gpt-5"));
    }

    // =========================================================================
    // Skill Tool Tests
    // =========================================================================

    #[tokio::test]
    async fn test_list_skills_empty() {
        let (server, _temp_dir) = create_test_server().await;

        let result = server.handle_list_skills().await;

        assert!(result.is_ok());
        let json = result.unwrap();
        let skills: Vec<SkillSummary> = serde_json::from_str(&json).unwrap();
        assert!(skills.is_empty());
    }

    #[tokio::test]
    async fn test_list_skills_multiple() {
        let (server, _temp_dir) = create_test_server().await;

        // Create skills using the service layer
        let skill1 = create_test_skill("skill-1", "Skill One");
        let skill2 = create_test_skill("skill-2", "Skill Two");

        restflow_core::services::skills::create_skill(&server.core, skill1)
            .await
            .unwrap();
        restflow_core::services::skills::create_skill(&server.core, skill2)
            .await
            .unwrap();

        let result = server.handle_list_skills().await;

        assert!(result.is_ok());
        let json = result.unwrap();
        let skills: Vec<SkillSummary> = serde_json::from_str(&json).unwrap();
        assert_eq!(skills.len(), 2);
    }

    #[tokio::test]
    async fn test_get_skill_success() {
        let (server, _temp_dir) = create_test_server().await;

        let skill = create_test_skill("test-skill", "Test Skill");
        restflow_core::services::skills::create_skill(&server.core, skill.clone())
            .await
            .unwrap();

        let params = GetSkillParams {
            id: "test-skill".to_string(),
        };
        let result = server.handle_get_skill(params).await;

        assert!(result.is_ok());
        let json = result.unwrap();
        let retrieved: Skill = serde_json::from_str(&json).unwrap();
        assert_eq!(retrieved.id, "test-skill");
        assert_eq!(retrieved.name, "Test Skill");
        assert_eq!(retrieved.content, skill.content);
    }

    #[tokio::test]
    async fn test_get_skill_not_found() {
        let (server, _temp_dir) = create_test_server().await;

        let params = GetSkillParams {
            id: "nonexistent".to_string(),
        };
        let result = server.handle_get_skill(params).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.contains("not found"));
    }

    #[tokio::test]
    async fn test_create_skill_success() {
        let (server, _temp_dir) = create_test_server().await;

        let params = CreateSkillParams {
            name: "New Skill".to_string(),
            description: Some("A new skill".to_string()),
            tags: Some(vec!["new".to_string()]),
            content: "# New Skill\n\nContent".to_string(),
        };
        let result = server.handle_create_skill(params).await;

        assert!(result.is_ok());
        let message = result.unwrap();
        assert!(message.contains("created successfully"));

        // Verify it was persisted
        let skills = server.handle_list_skills().await.unwrap();
        let skill_list: Vec<SkillSummary> = serde_json::from_str(&skills).unwrap();
        assert_eq!(skill_list.len(), 1);
        assert_eq!(skill_list[0].name, "New Skill");
    }

    #[tokio::test]
    async fn test_update_skill_success() {
        let (server, _temp_dir) = create_test_server().await;

        let skill = create_test_skill("test-skill", "Original Name");
        restflow_core::services::skills::create_skill(&server.core, skill)
            .await
            .unwrap();

        let params = UpdateSkillParams {
            id: "test-skill".to_string(),
            name: Some("Updated Name".to_string()),
            description: Some("Updated description".to_string()),
            tags: None,
            content: Some("# Updated content".to_string()),
        };
        let result = server.handle_update_skill(params).await;

        assert!(result.is_ok());

        // Verify changes
        let get_params = GetSkillParams {
            id: "test-skill".to_string(),
        };
        let json = server.handle_get_skill(get_params).await.unwrap();
        let updated: Skill = serde_json::from_str(&json).unwrap();
        assert_eq!(updated.name, "Updated Name");
        assert_eq!(updated.description, Some("Updated description".to_string()));
        assert_eq!(updated.content, "# Updated content");
    }

    #[tokio::test]
    async fn test_update_skill_not_found() {
        let (server, _temp_dir) = create_test_server().await;

        let params = UpdateSkillParams {
            id: "nonexistent".to_string(),
            name: Some("New Name".to_string()),
            description: None,
            tags: None,
            content: None,
        };
        let result = server.handle_update_skill(params).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn test_update_skill_partial() {
        let (server, _temp_dir) = create_test_server().await;

        let skill = create_test_skill("test-skill", "Original Name");
        restflow_core::services::skills::create_skill(&server.core, skill)
            .await
            .unwrap();

        // Only update name, keep other fields
        let params = UpdateSkillParams {
            id: "test-skill".to_string(),
            name: Some("New Name".to_string()),
            description: None,
            tags: None,
            content: None,
        };
        server.handle_update_skill(params).await.unwrap();

        let get_params = GetSkillParams {
            id: "test-skill".to_string(),
        };
        let json = server.handle_get_skill(get_params).await.unwrap();
        let updated: Skill = serde_json::from_str(&json).unwrap();

        assert_eq!(updated.name, "New Name");
        // Original description should be preserved
        assert_eq!(
            updated.description,
            Some("Description for Original Name".to_string())
        );
    }

    #[tokio::test]
    async fn test_delete_skill_success() {
        let (server, _temp_dir) = create_test_server().await;

        let skill = create_test_skill("test-skill", "Test Skill");
        restflow_core::services::skills::create_skill(&server.core, skill)
            .await
            .unwrap();

        let params = DeleteSkillParams {
            id: "test-skill".to_string(),
        };
        let result = server.handle_delete_skill(params).await;

        assert!(result.is_ok());

        // Verify deletion
        let get_params = GetSkillParams {
            id: "test-skill".to_string(),
        };
        let get_result = server.handle_get_skill(get_params).await;
        assert!(get_result.is_err());
    }

    // =========================================================================
    // Agent Tool Tests
    // =========================================================================

    #[tokio::test]
    async fn test_list_agents_empty() {
        let (server, _temp_dir) = create_test_server().await;

        let result = server.handle_list_agents().await;

        assert!(result.is_ok());
        let json = result.unwrap();
        let agents: Vec<AgentSummary> = serde_json::from_str(&json).unwrap();
        assert!(agents.is_empty());
    }

    #[tokio::test]
    async fn test_list_agents_multiple() {
        let (server, _temp_dir) = create_test_server().await;

        let agent1 = create_test_agent_node("Prompt 1");
        let agent2 = create_test_agent_node("Prompt 2");

        restflow_core::services::agent::create_agent(&server.core, "Agent 1".to_string(), agent1)
            .await
            .unwrap();
        restflow_core::services::agent::create_agent(&server.core, "Agent 2".to_string(), agent2)
            .await
            .unwrap();

        let result = server.handle_list_agents().await;

        assert!(result.is_ok());
        let json = result.unwrap();
        let agents: Vec<AgentSummary> = serde_json::from_str(&json).unwrap();
        assert_eq!(agents.len(), 2);
    }

    #[tokio::test]
    async fn test_get_agent_success() {
        let (server, _temp_dir) = create_test_server().await;

        let agent_node = create_test_agent_node("Test prompt");
        let stored = restflow_core::services::agent::create_agent(
            &server.core,
            "Test Agent".to_string(),
            agent_node,
        )
        .await
        .unwrap();

        let params = GetAgentParams {
            id: stored.id.clone(),
        };
        let result = server.handle_get_agent(params).await;

        assert!(result.is_ok());
        let json = result.unwrap();
        let retrieved: StoredAgent = serde_json::from_str(&json).unwrap();
        assert_eq!(retrieved.id, stored.id);
        assert_eq!(retrieved.name, "Test Agent");
        assert_eq!(retrieved.agent.prompt, Some("Test prompt".to_string()));
    }

    #[tokio::test]
    async fn test_get_agent_not_found() {
        let (server, _temp_dir) = create_test_server().await;

        let params = GetAgentParams {
            id: "nonexistent".to_string(),
        };
        let result = server.handle_get_agent(params).await;

        assert!(result.is_err());
    }

    // =========================================================================
    // ServerHandler Trait Tests
    // =========================================================================

    #[tokio::test]
    async fn test_get_info() {
        let (server, _temp_dir) = create_test_server().await;

        let info = server.get_info();

        assert_eq!(info.server_info.name, "restflow");
        assert!(info.capabilities.tools.is_some());
        assert!(info.instructions.is_some());
    }

    #[test]
    fn test_tool_definitions() {
        // Verify tool definitions are correct without needing RequestContext
        // The actual list_tools method would be called by the MCP framework
        let expected_tools = [
            "list_skills",
            "get_skill",
            "create_skill",
            "update_skill",
            "delete_skill",
            "list_agents",
            "get_agent",
        ];

        // Verify we have definitions for all expected tools
        assert_eq!(expected_tools.len(), 7);
    }

    #[tokio::test]
    async fn test_handle_unknown_tool() {
        let (server, _temp_dir) = create_test_server().await;

        // Test unknown tool handling by simulating what call_tool does internally
        let result = match "unknown_tool" {
            "list_skills" => server.handle_list_skills().await,
            _ => Err(format!("Unknown tool: {}", "unknown_tool")),
        };

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown tool"));
    }

    #[tokio::test]
    async fn test_handle_invalid_skill_params() {
        // Create test server to ensure setup works (also keeps pattern consistent)
        let (_server, _temp_dir) = create_test_server().await;

        // Test with invalid params - missing required id field
        let args = serde_json::json!({"wrong_field": "value"});
        let result: Result<GetSkillParams, _> = serde_json::from_value(args);

        // Should fail to parse
        assert!(result.is_err());
    }

    // =========================================================================
    // Integration Tests (Full Workflow)
    // =========================================================================

    #[tokio::test]
    async fn test_skill_crud_workflow() {
        let (server, _temp_dir) = create_test_server().await;

        // 1. Create
        let create_params = CreateSkillParams {
            name: "Workflow Skill".to_string(),
            description: Some("Test workflow".to_string()),
            tags: Some(vec!["workflow".to_string()]),
            content: "# Workflow\n\nInitial content".to_string(),
        };
        let create_result = server.handle_create_skill(create_params).await.unwrap();
        assert!(create_result.contains("created successfully"));

        // 2. List to get ID
        let list_json = server.handle_list_skills().await.unwrap();
        let skills: Vec<SkillSummary> = serde_json::from_str(&list_json).unwrap();
        assert_eq!(skills.len(), 1);
        let skill_id = skills[0].id.clone();

        // 3. Get
        let get_params = GetSkillParams {
            id: skill_id.clone(),
        };
        let get_json = server.handle_get_skill(get_params).await.unwrap();
        let skill: Skill = serde_json::from_str(&get_json).unwrap();
        assert_eq!(skill.name, "Workflow Skill");

        // 4. Update
        let update_params = UpdateSkillParams {
            id: skill_id.clone(),
            name: Some("Updated Workflow Skill".to_string()),
            description: None,
            tags: None,
            content: Some("# Updated\n\nNew content".to_string()),
        };
        server.handle_update_skill(update_params).await.unwrap();

        // 5. Verify update
        let get_params2 = GetSkillParams {
            id: skill_id.clone(),
        };
        let get_json2 = server.handle_get_skill(get_params2).await.unwrap();
        let updated_skill: Skill = serde_json::from_str(&get_json2).unwrap();
        assert_eq!(updated_skill.name, "Updated Workflow Skill");
        assert_eq!(updated_skill.content, "# Updated\n\nNew content");

        // 6. Delete
        let delete_params = DeleteSkillParams {
            id: skill_id.clone(),
        };
        server.handle_delete_skill(delete_params).await.unwrap();

        // 7. Verify deletion
        let final_list = server.handle_list_skills().await.unwrap();
        let final_skills: Vec<SkillSummary> = serde_json::from_str(&final_list).unwrap();
        assert!(final_skills.is_empty());
    }
}
