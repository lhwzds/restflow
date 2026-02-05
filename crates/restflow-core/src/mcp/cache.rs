use futures::future;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::{OnceCell, RwLock};
use tracing::debug;

/// Cached MCP tool information
#[derive(Debug, Clone)]
pub struct CachedMcpTool {
    pub server_name: String,
    pub tool_name: String,
    pub description: String,
    pub parameters_schema: serde_json::Value,
}

/// Global MCP tool cache
#[derive(Debug)]
pub struct McpToolCache {
    /// Discovered tools (server_name -> tools)
    tools: RwLock<HashMap<String, Vec<CachedMcpTool>>>,
    /// Discovery timestamps
    discovered_at: RwLock<HashMap<String, Instant>>,
    /// Cache TTL (default: 1 hour, MCP tools rarely change)
    ttl: Duration,
    /// Whether initial discovery is complete
    initialized: OnceCell<()>,
}

impl McpToolCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
            discovered_at: RwLock::new(HashMap::new()),
            ttl,
            initialized: OnceCell::new(),
        }
    }

    /// Get or discover tools for a server
    pub async fn get_tools(&self, server_name: &str) -> Vec<CachedMcpTool> {
        {
            let tools = self.tools.read().await;
            let timestamps = self.discovered_at.read().await;
            if let (Some(cached), Some(discovered)) =
                (tools.get(server_name), timestamps.get(server_name))
            {
                if discovered.elapsed() < self.ttl {
                    return cached.clone();
                }
            }
        }

        self.discover_server(server_name).await
    }

    /// Discover tools from a specific server
    async fn discover_server(&self, server_name: &str) -> Vec<CachedMcpTool> {
        debug!(server_name, "Refreshing MCP tool cache");

        let discovered = Vec::new();

        {
            let mut tools = self.tools.write().await;
            tools.insert(server_name.to_string(), discovered.clone());
        }

        {
            let mut timestamps = self.discovered_at.write().await;
            timestamps.insert(server_name.to_string(), Instant::now());
        }

        discovered
    }

    /// Initialize all configured MCP servers
    pub async fn initialize<I>(&self, servers: I)
    where
        I: IntoIterator<Item = String>,
    {
        let servers: Vec<String> = servers.into_iter().collect();
        let _ = self
            .initialized
            .get_or_init(async {
                if servers.is_empty() {
                    return;
                }

                let futures: Vec<_> = servers.iter().map(|name| self.discover_server(name)).collect();
                let _ = tokio::time::timeout(
                    Duration::from_secs(30),
                    future::join_all(futures),
                )
                .await;
            })
            .await;
    }

    /// Force refresh a specific server
    pub async fn refresh(&self, server_name: &str) {
        let mut timestamps = self.discovered_at.write().await;
        timestamps.remove(server_name);
        drop(timestamps);
        self.discover_server(server_name).await;
    }

    /// Refresh all servers
    pub async fn refresh_all(&self) {
        let mut timestamps = self.discovered_at.write().await;
        timestamps.clear();
    }

    /// Get all cached tools
    pub async fn all_tools(&self) -> Vec<CachedMcpTool> {
        let tools = self.tools.read().await;
        tools.values().flatten().cloned().collect()
    }
}
