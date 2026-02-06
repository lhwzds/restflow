//! Skill provider traits and implementations.
//!
//! Providers are responsible for discovering and fetching skills from various sources.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use thiserror::Error;

use crate::models::{SkillManifest, SkillSource, SkillVersion, VersionRequirement};

/// Errors that can occur when working with skill providers
#[derive(Debug, Error)]
pub enum SkillProviderError {
    #[error("Skill not found: {0}")]
    NotFound(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid manifest: {0}")]
    InvalidManifest(String),

    #[error("Version not found: {0}")]
    VersionNotFound(String),

    #[error("Provider error: {0}")]
    Other(String),
}

/// Search query for finding skills
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillSearchQuery {
    /// Search text (matches name, description, keywords)
    pub query: Option<String>,
    /// Filter by category
    pub category: Option<String>,
    /// Filter by keyword/tag
    pub tags: Vec<String>,
    /// Filter by author
    pub author: Option<String>,
    /// Maximum number of results
    pub limit: Option<usize>,
    /// Offset for pagination
    pub offset: Option<usize>,
    /// Sort order
    pub sort: Option<SkillSortOrder>,
}

/// Sort order for skill search results
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SkillSortOrder {
    /// Most relevant (based on search query)
    #[default]
    Relevance,
    /// Most recently updated
    RecentlyUpdated,
    /// Most downloads/installs
    Popular,
    /// Alphabetical by name
    Name,
}

/// Search result containing skill information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSearchResult {
    /// Skill manifest
    pub manifest: SkillManifest,
    /// Relevance score (0-100)
    pub score: u32,
    /// Number of downloads/installs (if available)
    pub downloads: Option<u64>,
    /// Average rating (if available)
    pub rating: Option<f32>,
}

/// Trait for skill providers (sources of skills)
#[async_trait]
pub trait SkillProvider: Send + Sync {
    /// Get the provider name
    fn name(&self) -> &str;

    /// Get the provider priority (higher = checked first)
    fn priority(&self) -> u32 {
        50
    }

    /// Search for skills matching the query
    async fn search(
        &self,
        query: &SkillSearchQuery,
    ) -> Result<Vec<SkillSearchResult>, SkillProviderError>;

    /// Get a skill manifest by ID
    async fn get_manifest(&self, id: &str) -> Result<SkillManifest, SkillProviderError>;

    /// Get a specific version of a skill manifest
    async fn get_manifest_version(
        &self,
        id: &str,
        version: &SkillVersion,
    ) -> Result<SkillManifest, SkillProviderError> {
        let manifest = self.get_manifest(id).await?;
        if &manifest.version == version {
            Ok(manifest)
        } else {
            Err(SkillProviderError::VersionNotFound(format!(
                "{}@{}",
                id, version
            )))
        }
    }

    /// Get the content of a skill
    async fn get_content(
        &self,
        id: &str,
        version: &SkillVersion,
    ) -> Result<String, SkillProviderError>;

    /// List all available versions of a skill
    async fn list_versions(&self, id: &str) -> Result<Vec<SkillVersion>, SkillProviderError>;

    /// Check if a skill exists
    async fn exists(&self, id: &str) -> bool {
        self.get_manifest(id).await.is_ok()
    }

    /// Get the latest version of a skill that satisfies the requirement
    async fn resolve_version(
        &self,
        id: &str,
        requirement: &VersionRequirement,
    ) -> Result<SkillVersion, SkillProviderError> {
        let versions = self.list_versions(id).await?;
        versions
            .into_iter()
            .filter(|v| v.satisfies(requirement))
            .max_by(|a, b| {
                // Compare versions
                if a.major != b.major {
                    a.major.cmp(&b.major)
                } else if a.minor != b.minor {
                    a.minor.cmp(&b.minor)
                } else {
                    a.patch.cmp(&b.patch)
                }
            })
            .ok_or_else(|| {
                SkillProviderError::VersionNotFound(format!(
                    "No version of {} satisfies requirement",
                    id
                ))
            })
    }
}

/// Local skill provider - reads skills from the local filesystem
pub struct LocalSkillProvider {
    /// Base directory for skills
    base_dir: PathBuf,
    /// Cached manifests
    cache: tokio::sync::RwLock<HashMap<String, SkillManifest>>,
}

impl LocalSkillProvider {
    /// Create a new local provider
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            base_dir,
            cache: tokio::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Scan the directory for skills
    async fn scan_skills(&self) -> Result<Vec<SkillManifest>, SkillProviderError> {
        let mut manifests = Vec::new();

        if !self.base_dir.exists() {
            return Ok(manifests);
        }

        let mut entries = tokio::fs::read_dir(&self.base_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            // Check for skill.toml or skill.json
            let manifest_path = if path.is_dir() {
                let toml_path = path.join("skill.toml");
                let json_path = path.join("skill.json");
                if toml_path.exists() {
                    Some(toml_path)
                } else if json_path.exists() {
                    Some(json_path)
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(manifest_path) = manifest_path {
                match self.parse_manifest(&manifest_path).await {
                    Ok(mut manifest) => {
                        manifest.source = SkillSource::Local;
                        manifests.push(manifest);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse manifest at {:?}: {}", manifest_path, e);
                    }
                }
            }
        }

        // Update cache
        let mut cache = self.cache.write().await;
        for manifest in &manifests {
            cache.insert(manifest.id.clone(), manifest.clone());
        }

        Ok(manifests)
    }

    /// Parse a manifest file
    async fn parse_manifest(&self, path: &PathBuf) -> Result<SkillManifest, SkillProviderError> {
        let content = tokio::fs::read_to_string(path).await?;

        if path.extension().map(|e| e == "toml").unwrap_or(false) {
            toml::from_str(&content).map_err(|e| SkillProviderError::Parse(e.to_string()))
        } else {
            serde_json::from_str(&content).map_err(|e| SkillProviderError::Parse(e.to_string()))
        }
    }
}

#[async_trait]
impl SkillProvider for LocalSkillProvider {
    fn name(&self) -> &str {
        "local"
    }

    fn priority(&self) -> u32 {
        100 // Local has highest priority
    }

    async fn search(
        &self,
        query: &SkillSearchQuery,
    ) -> Result<Vec<SkillSearchResult>, SkillProviderError> {
        let manifests = self.scan_skills().await?;

        let results: Vec<SkillSearchResult> = manifests
            .into_iter()
            .filter(|m| {
                // Filter by query text
                if let Some(ref q) = query.query {
                    let q_lower = q.to_lowercase();
                    let matches = m.name.to_lowercase().contains(&q_lower)
                        || m.description
                            .as_ref()
                            .map(|d| d.to_lowercase().contains(&q_lower))
                            .unwrap_or(false)
                        || m.keywords
                            .iter()
                            .any(|k| k.to_lowercase().contains(&q_lower));
                    if !matches {
                        return false;
                    }
                }

                // Filter by category
                if let Some(ref cat) = query.category
                    && !m.categories.iter().any(|c| c == cat)
                {
                    return false;
                }

                // Filter by tags
                if !query.tags.is_empty() {
                    let has_all_tags = query.tags.iter().all(|t| m.keywords.contains(t));
                    if !has_all_tags {
                        return false;
                    }
                }

                true
            })
            .map(|m| SkillSearchResult {
                manifest: m,
                score: 100, // Local skills get full relevance
                downloads: None,
                rating: None,
            })
            .collect();

        // Apply pagination
        let offset = query.offset.unwrap_or(0);
        let limit = query.limit.unwrap_or(100);

        Ok(results.into_iter().skip(offset).take(limit).collect())
    }

    async fn get_manifest(&self, id: &str) -> Result<SkillManifest, SkillProviderError> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(manifest) = cache.get(id) {
                return Ok(manifest.clone());
            }
        }

        // Scan and try again
        self.scan_skills().await?;

        let cache = self.cache.read().await;
        cache
            .get(id)
            .cloned()
            .ok_or_else(|| SkillProviderError::NotFound(id.to_string()))
    }

    async fn get_content(
        &self,
        id: &str,
        _version: &SkillVersion,
    ) -> Result<String, SkillProviderError> {
        let skill_dir = self.base_dir.join(id);
        let content_path = skill_dir.join("skill.md");

        if content_path.exists() {
            tokio::fs::read_to_string(&content_path)
                .await
                .map_err(SkillProviderError::Io)
        } else {
            // Try reading from manifest
            let manifest = self.get_manifest(id).await?;
            manifest.readme.ok_or_else(|| {
                SkillProviderError::NotFound(format!("Content not found for skill {}", id))
            })
        }
    }

    async fn list_versions(&self, id: &str) -> Result<Vec<SkillVersion>, SkillProviderError> {
        // Local skills only have one version
        let manifest = self.get_manifest(id).await?;
        Ok(vec![manifest.version])
    }
}

/// Built-in skill provider - provides skills bundled with RestFlow
pub struct BuiltinSkillProvider {
    /// Built-in skills (loaded at compile time or from embedded resources)
    skills: HashMap<String, (SkillManifest, String)>,
}

impl BuiltinSkillProvider {
    /// Create a new builtin provider with the default skills
    pub fn new() -> Self {
        let skills = HashMap::new();

        // Add built-in skills here
        // These would typically be loaded from embedded resources

        Self { skills }
    }

    /// Add a built-in skill
    pub fn add_skill(&mut self, manifest: SkillManifest, content: String) {
        self.skills.insert(manifest.id.clone(), (manifest, content));
    }
}

impl Default for BuiltinSkillProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SkillProvider for BuiltinSkillProvider {
    fn name(&self) -> &str {
        "builtin"
    }

    fn priority(&self) -> u32 {
        90 // Second highest priority after local
    }

    async fn search(
        &self,
        query: &SkillSearchQuery,
    ) -> Result<Vec<SkillSearchResult>, SkillProviderError> {
        let results: Vec<SkillSearchResult> = self
            .skills
            .values()
            .filter(|(m, _)| {
                if let Some(ref q) = query.query {
                    let q_lower = q.to_lowercase();
                    m.name.to_lowercase().contains(&q_lower)
                        || m.description
                            .as_ref()
                            .map(|d| d.to_lowercase().contains(&q_lower))
                            .unwrap_or(false)
                } else {
                    true
                }
            })
            .map(|(m, _)| SkillSearchResult {
                manifest: m.clone(),
                score: 90,
                downloads: None,
                rating: None,
            })
            .collect();

        Ok(results)
    }

    async fn get_manifest(&self, id: &str) -> Result<SkillManifest, SkillProviderError> {
        self.skills
            .get(id)
            .map(|(m, _)| m.clone())
            .ok_or_else(|| SkillProviderError::NotFound(id.to_string()))
    }

    async fn get_content(
        &self,
        id: &str,
        _version: &SkillVersion,
    ) -> Result<String, SkillProviderError> {
        self.skills
            .get(id)
            .map(|(_, c)| c.clone())
            .ok_or_else(|| SkillProviderError::NotFound(id.to_string()))
    }

    async fn list_versions(&self, id: &str) -> Result<Vec<SkillVersion>, SkillProviderError> {
        self.skills
            .get(id)
            .map(|(m, _)| vec![m.version.clone()])
            .ok_or_else(|| SkillProviderError::NotFound(id.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_builtin_provider() {
        let mut provider = BuiltinSkillProvider::new();

        let manifest = SkillManifest {
            id: "test-skill".to_string(),
            name: "Test Skill".to_string(),
            version: SkillVersion::new(1, 0, 0),
            description: Some("A test skill".to_string()),
            ..Default::default()
        };

        provider.add_skill(manifest.clone(), "# Test Content".to_string());

        let result = provider.get_manifest("test-skill").await.unwrap();
        assert_eq!(result.id, "test-skill");

        let content = provider
            .get_content("test-skill", &SkillVersion::new(1, 0, 0))
            .await
            .unwrap();
        assert_eq!(content, "# Test Content");
    }
}
