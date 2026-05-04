//! MarketplaceStore adapter for registry search and read-only metadata.

use crate::registry::{GitHubProvider, MarketplaceProvider, SkillProvider as _, SkillSearchQuery};
use restflow_storage::{RegistryDefaults, RegistrySettings};
use restflow_tools::ToolError;
use restflow_traits::store::MarketplaceStore;
use serde_json::{Value, json};

use super::SkrunSkillProvider;

pub struct MarketplaceStoreAdapter {
    github_provider: GitHubProvider,
    marketplace_provider: MarketplaceProvider,
}

impl MarketplaceStoreAdapter {
    pub fn new() -> Self {
        Self::new_with_settings(RegistrySettings::default())
    }

    pub fn new_with_settings(registry: RegistrySettings) -> Self {
        let github_provider =
            GitHubProvider::new().with_cache_ttl_secs(registry.github_cache_ttl_secs);
        let marketplace_provider =
            MarketplaceProvider::new().with_cache_ttl_secs(registry.marketplace_cache_ttl_secs);
        Self {
            github_provider,
            marketplace_provider,
        }
    }

    pub fn new_with_defaults(registry_defaults: RegistryDefaults) -> Self {
        Self::new_with_settings(registry_defaults)
    }

    fn provider_name(source: Option<&str>) -> &str {
        match source {
            Some("github") => "github",
            _ => "marketplace",
        }
    }

    fn is_reserved_skrun_skill_id(id: &str) -> Result<bool, String> {
        Self::is_reserved_skrun_skill_id_with_provider(id, &SkrunSkillProvider::default())
    }

    fn is_reserved_skrun_skill_id_with_provider(
        id: &str,
        provider: &SkrunSkillProvider,
    ) -> Result<bool, String> {
        provider
            .try_get_skill_model(id)
            .map(|skill| skill.is_some())
    }

    async fn search_source(
        &self,
        source: &str,
        query: &SkillSearchQuery,
    ) -> Result<Vec<crate::registry::SkillSearchResult>, ToolError> {
        match source {
            "github" => self
                .github_provider
                .search(query)
                .await
                .map_err(|e| ToolError::Tool(e.to_string())),
            _ => self
                .marketplace_provider
                .search(query)
                .await
                .map_err(|e| ToolError::Tool(e.to_string())),
        }
    }

    async fn get_manifest(
        &self,
        source: &str,
        id: &str,
    ) -> Result<crate::models::SkillManifest, ToolError> {
        match source {
            "github" => self
                .github_provider
                .get_manifest(id)
                .await
                .map_err(|e| ToolError::Tool(e.to_string())),
            _ => self
                .marketplace_provider
                .get_manifest(id)
                .await
                .map_err(|e| ToolError::Tool(e.to_string())),
        }
    }

    fn storage_removed_error(id: &str) -> ToolError {
        ToolError::Tool(format!(
            "RestFlow no longer installs or stores skills; install '{id}' through skrun"
        ))
    }
}

impl Default for MarketplaceStoreAdapter {
    fn default() -> Self {
        Self::new()
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
        let results = self.search_source(source_name, &q).await?;
        Ok(serde_json::to_value(results)?)
    }

    async fn skill_info(&self, id: &str, source: Option<&str>) -> restflow_tools::Result<Value> {
        let source_name = Self::provider_name(source);
        let manifest = self.get_manifest(source_name, id).await?;
        Ok(serde_json::to_value(manifest)?)
    }

    async fn install_skill(
        &self,
        id: &str,
        _source: Option<&str>,
        _overwrite: bool,
    ) -> restflow_tools::Result<Value> {
        if Self::is_reserved_skrun_skill_id(id).map_err(ToolError::Tool)? {
            return Err(ToolError::Tool(format!(
                "Cannot install marketplace skill over read-only skrun skill: {id}"
            )));
        }
        Err(Self::storage_removed_error(id))
    }

    fn uninstall_skill(&self, id: &str) -> restflow_tools::Result<Value> {
        Err(ToolError::Tool(format!(
            "RestFlow no longer stores installed skills; remove '{id}' through skrun"
        )))
    }

    fn list_installed(&self) -> restflow_tools::Result<Value> {
        Ok(json!([]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_traits::store::MarketplaceStore;

    fn setup() -> MarketplaceStoreAdapter {
        MarketplaceStoreAdapter::new()
    }

    #[test]
    fn test_list_installed_empty() {
        let adapter = setup();
        let result = adapter.list_installed().unwrap();
        let skills = result.as_array().unwrap();
        assert!(skills.is_empty());
    }

    #[test]
    fn test_uninstall_returns_skrun_guidance() {
        let adapter = setup();
        let err = adapter.uninstall_skill("demo").unwrap_err();
        assert!(err.to_string().contains("through skrun"));
    }

    #[test]
    fn test_provider_name() {
        assert_eq!(
            MarketplaceStoreAdapter::provider_name(Some("github")),
            "github"
        );
        assert_eq!(MarketplaceStoreAdapter::provider_name(None), "marketplace");
        assert_eq!(
            MarketplaceStoreAdapter::provider_name(Some("other")),
            "marketplace"
        );
    }

    #[test]
    fn test_reserved_skrun_skill_detection_with_provider() {
        let dir = tempfile::tempdir().unwrap();
        let artifact = skrun::SkillArtifact::markdown("team", "Team", "0.1.0", "# Team");
        skrun::save_artifact(dir.path().join("team"), &artifact).unwrap();
        let provider = SkrunSkillProvider::new(dir.path());

        assert!(
            MarketplaceStoreAdapter::is_reserved_skrun_skill_id_with_provider("team", &provider)
                .unwrap()
        );
        assert!(
            !MarketplaceStoreAdapter::is_reserved_skrun_skill_id_with_provider(
                "demo-skill",
                &provider
            )
            .unwrap()
        );
    }
}
