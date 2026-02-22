//! MarketplaceStore adapter backed by SkillStorage.

use crate::models::Skill;
use crate::registry::{GitHubProvider, MarketplaceProvider, SkillProvider as _, SkillSearchQuery};
use crate::storage::skill::SkillStorage;
use chrono::Utc;
use restflow_ai::tools::MarketplaceStore;
use restflow_tools::ToolError;
use serde_json::{Value, json};

pub struct MarketplaceStoreAdapter {
    storage: SkillStorage,
}

impl MarketplaceStoreAdapter {
    pub fn new(storage: SkillStorage) -> Self {
        Self { storage }
    }

    fn provider_name(source: Option<&str>) -> &str {
        match source {
            Some("github") => "github",
            _ => "marketplace",
        }
    }

    async fn search_source(
        source: &str,
        query: &SkillSearchQuery,
    ) -> Result<Vec<crate::registry::SkillSearchResult>, ToolError> {
        match source {
            "github" => GitHubProvider::new()
                .search(query)
                .await
                .map_err(|e| ToolError::Tool(e.to_string())),
            _ => MarketplaceProvider::new()
                .search(query)
                .await
                .map_err(|e| ToolError::Tool(e.to_string())),
        }
    }

    async fn get_manifest(
        source: &str,
        id: &str,
    ) -> Result<crate::models::SkillManifest, ToolError> {
        match source {
            "github" => GitHubProvider::new()
                .get_manifest(id)
                .await
                .map_err(|e| ToolError::Tool(e.to_string())),
            _ => MarketplaceProvider::new()
                .get_manifest(id)
                .await
                .map_err(|e| ToolError::Tool(e.to_string())),
        }
    }

    async fn get_content(
        source: &str,
        id: &str,
        version: &crate::models::SkillVersion,
    ) -> Result<String, ToolError> {
        match source {
            "github" => GitHubProvider::new()
                .get_content(id, version)
                .await
                .map_err(|e| ToolError::Tool(e.to_string())),
            _ => MarketplaceProvider::new()
                .get_content(id, version)
                .await
                .map_err(|e| ToolError::Tool(e.to_string())),
        }
    }

    fn manifest_to_skill(manifest: crate::models::SkillManifest, content: String) -> Skill {
        let now = Utc::now().timestamp_millis();
        Skill {
            id: manifest.id,
            name: manifest.name,
            description: manifest.description,
            tags: Some(manifest.keywords),
            triggers: Vec::new(),
            content,
            folder_path: None,
            suggested_tools: Vec::new(),
            scripts: Vec::new(),
            references: Vec::new(),
            gating: None,
            version: Some(manifest.version.to_string()),
            author: manifest.author.map(|a| a.name),
            license: manifest.license,
            content_hash: None,
            status: crate::models::SkillStatus::Active,
            auto_complete: false,
            storage_mode: crate::models::StorageMode::DatabaseOnly,
            is_synced: false,
            created_at: now,
            updated_at: now,
        }
    }
}

#[async_trait::async_trait]
impl MarketplaceStore for MarketplaceStoreAdapter {
    async fn search_skills(
        &self,
        query: Option<&str>,
        category: Option<&str>,
        tags: Option<Vec<String>>,
        author: Option<&str>,
        limit: Option<usize>,
        offset: Option<usize>,
        source: Option<&str>,
    ) -> restflow_tools::Result<Value> {
        let q = SkillSearchQuery {
            query: query.map(|s| s.to_string()),
            category: category.map(|s| s.to_string()),
            tags: tags.unwrap_or_default(),
            author: author.map(|s| s.to_string()),
            limit,
            offset,
            sort: None,
        };
        let source_name = Self::provider_name(source);
        let results = Self::search_source(source_name, &q).await?;
        Ok(serde_json::to_value(results)?)
    }

    async fn skill_info(&self, id: &str, source: Option<&str>) -> restflow_tools::Result<Value> {
        let source_name = Self::provider_name(source);
        let manifest = Self::get_manifest(source_name, id).await?;
        Ok(serde_json::to_value(manifest)?)
    }

    async fn install_skill(
        &self,
        id: &str,
        source: Option<&str>,
        overwrite: bool,
    ) -> restflow_tools::Result<Value> {
        let source_name = Self::provider_name(source);
        let manifest = Self::get_manifest(source_name, id).await?;
        let content = Self::get_content(source_name, id, &manifest.version).await?;
        let skill = Self::manifest_to_skill(manifest, content);

        let exists = self
            .storage
            .exists(id)
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        if exists && !overwrite {
            return Err(ToolError::Tool(
                "Skill already installed. Set overwrite=true to replace.".to_string(),
            ));
        }

        if exists {
            self.storage
                .update(id, &skill)
                .map_err(|e| ToolError::Tool(e.to_string()))?;
        } else {
            self.storage
                .create(&skill)
                .map_err(|e| ToolError::Tool(e.to_string()))?;
        }

        Ok(json!({
            "id": id,
            "name": skill.name,
            "version": skill.version,
            "installed": true,
            "updated": exists
        }))
    }

    fn uninstall_skill(&self, id: &str) -> restflow_tools::Result<Value> {
        let exists = self
            .storage
            .exists(id)
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        if exists {
            self.storage
                .delete(id)
                .map_err(|e| ToolError::Tool(e.to_string()))?;
        }
        Ok(json!({
            "id": id,
            "deleted": exists
        }))
    }

    fn list_installed(&self) -> restflow_tools::Result<Value> {
        let skills = self
            .storage
            .list()
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        Ok(serde_json::to_value(skills)?)
    }
}
