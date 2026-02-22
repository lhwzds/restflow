//! Marketplace tool for searching, installing, and managing skills from remote sources.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::Result;
use crate::{Tool, ToolOutput};
use restflow_traits::store::MarketplaceStore;

pub struct MarketplaceTool {
    store: Arc<dyn MarketplaceStore>,
}

impl MarketplaceTool {
    pub fn new(store: Arc<dyn MarketplaceStore>) -> Self {
        Self { store }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum MarketplaceOperation {
    Search {
        #[serde(default)]
        query: Option<String>,
        #[serde(default)]
        category: Option<String>,
        #[serde(default)]
        tags: Option<Vec<String>>,
        #[serde(default)]
        author: Option<String>,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        offset: Option<usize>,
        #[serde(default)]
        source: Option<String>,
    },
    Info {
        id: String,
        #[serde(default)]
        source: Option<String>,
    },
    Install {
        id: String,
        #[serde(default)]
        source: Option<String>,
        #[serde(default)]
        overwrite: bool,
    },
    Uninstall {
        id: String,
    },
    ListInstalled,
}

#[async_trait]
impl Tool for MarketplaceTool {
    fn name(&self) -> &str {
        "manage_marketplace"
    }

    fn description(&self) -> &str {
        "Search marketplace skills and install/uninstall them into local skill storage."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["search", "info", "install", "uninstall", "list_installed"]
                },
                "id": { "type": "string" },
                "query": { "type": "string" },
                "category": { "type": "string" },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "author": { "type": "string" },
                "limit": { "type": "integer", "minimum": 1 },
                "offset": { "type": "integer", "minimum": 0 },
                "source": { "type": "string", "enum": ["marketplace", "github"] },
                "overwrite": { "type": "boolean", "default": false }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let operation: MarketplaceOperation = serde_json::from_value(input)?;
        match operation {
            MarketplaceOperation::Search {
                query,
                category,
                tags,
                author,
                limit,
                offset,
                source,
            } => {
                let result = self
                    .store
                    .search_skills(
                        query.as_deref(),
                        category.as_deref(),
                        tags,
                        author.as_deref(),
                        limit,
                        offset,
                        source.as_deref(),
                    )
                    .await?;
                Ok(ToolOutput::success(result))
            }
            MarketplaceOperation::Info { id, source } => {
                let result = self.store.skill_info(&id, source.as_deref()).await?;
                Ok(ToolOutput::success(result))
            }
            MarketplaceOperation::Install {
                id,
                source,
                overwrite,
            } => {
                let result = self
                    .store
                    .install_skill(&id, source.as_deref(), overwrite)
                    .await?;
                Ok(ToolOutput::success(result))
            }
            MarketplaceOperation::Uninstall { id } => {
                let result = self.store.uninstall_skill(&id)?;
                Ok(ToolOutput::success(result))
            }
            MarketplaceOperation::ListInstalled => {
                let result = self.store.list_installed()?;
                Ok(ToolOutput::success(result))
            }
        }
    }
}
