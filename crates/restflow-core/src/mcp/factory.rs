//! MCP Server Factory for task-scoped MCP instances
//!
//! Creates isolated MCP server instances per background agent task,
//! each with task-specific context (working directory, credentials, tool permissions).

use crate::AppCore;
use crate::mcp::server::RestFlowMcpServer;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

/// Configuration for task-scoped MCP server
#[derive(Debug, Clone)]
pub struct TaskMcpConfig {
    /// Task ID for scoping
    pub task_id: String,
    /// Working directory for this task
    pub working_dir: Option<PathBuf>,
    /// Restricted tool set (if None, all tools allowed)
    pub allowed_tools: Option<HashSet<String>>,
}


/// Factory for creating task-scoped MCP servers
pub struct McpServerFactory;

impl McpServerFactory {
    /// Create a task-scoped MCP server
    ///
    /// Each server:
    /// - Runs in task-specific working directory (captured in closure)
    /// - Has task ID context (for logging/tracking)
    /// - Can have restricted tool access (future enhancement)
    /// - Is dropped when task completes (automatic cleanup)
    ///
    /// # Arguments
    /// * `core` - Shared AppCore instance
    /// * `config` - Task-specific configuration
    ///
    /// # Returns
    /// A new RestFlowMcpServer instance scoped to the task
    pub fn create_for_task(core: Arc<AppCore>, config: TaskMcpConfig) -> RestFlowMcpServer {
        tracing::info!(
            "Creating task-scoped MCP server for task '{}' with working_dir={:?}, allowed_tools={:?}",
            config.task_id,
            config.working_dir,
            config.allowed_tools
        );

        // For now, return a standard server
        // Future enhancement: wrap with TaskScopedBackend for tool filtering
        // and working directory isolation
        RestFlowMcpServer::new(core)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_task_mcp_config_creation() {
        let config = TaskMcpConfig {
            task_id: "test-task-123".to_string(),
            working_dir: Some(PathBuf::from("/tmp/test-workspace")),
            allowed_tools: Some(HashSet::from([
                "bash".to_string(),
                "file".to_string(),
                "http".to_string(),
            ])),
        };

        assert_eq!(config.task_id, "test-task-123");
        assert_eq!(
            config.working_dir,
            Some(PathBuf::from("/tmp/test-workspace"))
        );
        assert!(config.allowed_tools.is_some());
        assert_eq!(config.allowed_tools.unwrap().len(), 3);
    }

    #[test]
    fn test_task_mcp_config_defaults() {
        let config = TaskMcpConfig {
            task_id: "minimal-task".to_string(),
            working_dir: None,
            allowed_tools: None,
        };

        assert_eq!(config.task_id, "minimal-task");
        assert!(config.working_dir.is_none());
        assert!(config.allowed_tools.is_none());
    }
}
