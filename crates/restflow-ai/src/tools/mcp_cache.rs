//! Minimal MCP tool cache with a global OnceCell.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use once_cell::sync::OnceCell;

use crate::tools::traits::ToolSchema;

#[derive(Debug, Clone)]
pub struct McpServerConfig {
    pub command: String,
}

#[derive(Debug, Default)]
struct McpToolCache {
    generation: u64,
    initialized: bool,
    tools: Arc<Vec<ToolSchema>>,
}

static MCP_CACHE: OnceCell<Arc<RwLock<McpToolCache>>> = OnceCell::new();
static MCP_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Get or discover MCP tools and cache them globally.
pub async fn get_mcp_tools(configs: &HashMap<String, McpServerConfig>) -> Arc<Vec<ToolSchema>> {
    let cache = MCP_CACHE
        .get_or_init(|| Arc::new(RwLock::new(McpToolCache::default())))
        .clone();

    let generation = MCP_GENERATION.load(Ordering::SeqCst);
    if let Ok(cache_read) = cache.read()
        && cache_read.initialized
        && cache_read.generation == generation
    {
        return cache_read.tools.clone();
    }

    let tools = Arc::new(discover_mcp_tools(configs).await);

    if let Ok(mut cache_write) = cache.write() {
        *cache_write = McpToolCache {
            generation,
            initialized: true,
            tools: tools.clone(),
        };
    }

    tools
}

/// Force re-discovery (e.g., after config change).
pub fn invalidate_mcp_cache() {
    MCP_GENERATION.fetch_add(1, Ordering::SeqCst);
}

async fn discover_mcp_tools(configs: &HashMap<String, McpServerConfig>) -> Vec<ToolSchema> {
    let mut tools = Vec::new();

    for (name, config) in configs {
        let discover_result =
            tokio::time::timeout(Duration::from_secs(30), discover_server_tools(name, config))
                .await;

        match discover_result {
            Ok(Ok(server_tools)) => {
                tracing::info!(server = %name, count = server_tools.len(), "MCP tools discovered");
                tools.extend(server_tools);
            }
            Ok(Err(error)) => {
                tracing::warn!(server = %name, error = %error, "MCP discovery failed");
            }
            Err(_) => {
                tracing::warn!(server = %name, "MCP discovery timeout (30s)");
            }
        }
    }

    tools
}

async fn discover_server_tools(
    _name: &str,
    _config: &McpServerConfig,
) -> Result<Vec<ToolSchema>, String> {
    Ok(Vec::new())
}
