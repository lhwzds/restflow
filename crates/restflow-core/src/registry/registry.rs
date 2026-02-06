//! Skill Registry - centralized skill management.
//!
//! The registry aggregates multiple skill providers and provides:
//! - Unified search across all sources
//! - Installation with dependency resolution
//! - Gating requirement checks
//! - Update management

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::models::{
    GatingCheckResult, InstallStatus, InstalledSkill, SkillManifest, SkillVersion,
};

use super::gating::GatingChecker;
use super::provider::{
    BuiltinSkillProvider, LocalSkillProvider, SkillProvider, SkillProviderError, SkillSearchQuery,
    SkillSearchResult,
};
use super::resolver::{DependencyError, DependencyResolver, InstallPlan};

/// Registry configuration
#[derive(Debug, Clone)]
pub struct SkillRegistryConfig {
    /// Directory for installed skills
    pub skills_dir: PathBuf,
    /// Directory for skill cache
    pub cache_dir: PathBuf,
    /// Enable marketplace provider
    pub enable_marketplace: bool,
    /// Marketplace URL
    pub marketplace_url: Option<String>,
}

impl Default for SkillRegistryConfig {
    fn default() -> Self {
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("restflow");

        Self {
            skills_dir: data_dir.join("skills"),
            cache_dir: data_dir.join("cache").join("skills"),
            enable_marketplace: true,
            marketplace_url: None,
        }
    }
}

/// Skill Registry - manages skill discovery, installation, and updates
pub struct SkillRegistry {
    /// Configuration
    config: SkillRegistryConfig,
    /// Registered providers (sorted by priority)
    providers: Vec<Arc<dyn SkillProvider>>,
    /// Installed skills cache
    installed: RwLock<HashMap<String, InstalledSkill>>,
    /// Gating checker
    gating_checker: GatingChecker,
}

impl SkillRegistry {
    /// Create a new skill registry with the given configuration
    pub fn new(config: SkillRegistryConfig) -> Self {
        let mut providers: Vec<Arc<dyn SkillProvider>> = vec![
            // Add local provider
            Arc::new(LocalSkillProvider::new(config.skills_dir.clone())),
            // Add builtin provider
            Arc::new(BuiltinSkillProvider::new()),
        ];

        // Sort by priority (descending)
        providers.sort_by_key(|p| std::cmp::Reverse(p.priority()));

        Self {
            config,
            providers,
            installed: RwLock::new(HashMap::new()),
            gating_checker: GatingChecker::default(),
        }
    }

    /// Create a registry with default configuration
    pub fn with_defaults() -> Self {
        Self::new(SkillRegistryConfig::default())
    }

    /// Add a custom provider
    pub fn add_provider(&mut self, provider: Arc<dyn SkillProvider>) {
        self.providers.push(provider);
        self.providers
            .sort_by_key(|p| std::cmp::Reverse(p.priority()));
    }

    /// Search for skills across all providers
    pub async fn search(&self, query: &SkillSearchQuery) -> Vec<SkillSearchResult> {
        let mut all_results = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        for provider in &self.providers {
            match provider.search(query).await {
                Ok(results) => {
                    for result in results {
                        // Deduplicate by skill ID (higher priority providers win)
                        if !seen_ids.contains(&result.manifest.id) {
                            seen_ids.insert(result.manifest.id.clone());
                            all_results.push(result);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Search failed for provider {}: {}", provider.name(), e);
                }
            }
        }

        // Sort by score
        all_results.sort_by(|a, b| b.score.cmp(&a.score));

        // Apply global limit
        if let Some(limit) = query.limit {
            all_results.truncate(limit);
        }

        all_results
    }

    /// Get a skill manifest from any provider
    pub async fn get_manifest(&self, id: &str) -> Result<SkillManifest, SkillProviderError> {
        for provider in &self.providers {
            match provider.get_manifest(id).await {
                Ok(manifest) => return Ok(manifest),
                Err(SkillProviderError::NotFound(_)) => continue,
                Err(e) => {
                    tracing::warn!("Error getting manifest from {}: {}", provider.name(), e);
                    continue;
                }
            }
        }

        Err(SkillProviderError::NotFound(id.to_string()))
    }

    /// Get skill content from any provider
    pub async fn get_content(
        &self,
        id: &str,
        version: &SkillVersion,
    ) -> Result<String, SkillProviderError> {
        for provider in &self.providers {
            match provider.get_content(id, version).await {
                Ok(content) => return Ok(content),
                Err(SkillProviderError::NotFound(_)) => continue,
                Err(e) => {
                    tracing::warn!("Error getting content from {}: {}", provider.name(), e);
                    continue;
                }
            }
        }

        Err(SkillProviderError::NotFound(id.to_string()))
    }

    /// List all installed skills
    pub async fn list_installed(&self) -> Vec<InstalledSkill> {
        let installed = self.installed.read().await;
        installed.values().cloned().collect()
    }

    /// Get an installed skill
    pub async fn get_installed(&self, id: &str) -> Option<InstalledSkill> {
        let installed = self.installed.read().await;
        installed.get(id).cloned()
    }

    /// Check if a skill is installed
    pub async fn is_installed(&self, id: &str) -> bool {
        let installed = self.installed.read().await;
        installed.contains_key(id)
    }

    /// Plan installation of skills (with dependency resolution)
    pub async fn plan_install(&self, skill_ids: &[String]) -> Result<InstallPlan, DependencyError> {
        let mut resolver = DependencyResolver::new();

        // Set currently installed skills
        let installed = self.installed.read().await;
        let installed_versions: HashMap<String, SkillVersion> = installed
            .iter()
            .map(|(id, skill)| (id.clone(), skill.manifest.version.clone()))
            .collect();
        drop(installed);

        resolver.set_installed(installed_versions);

        // Recursively resolve dependencies
        let mut to_resolve = skill_ids.to_vec();
        let mut resolved = std::collections::HashSet::new();

        while let Some(skill_id) = to_resolve.pop() {
            if resolved.contains(&skill_id) {
                continue;
            }

            let manifest = self
                .get_manifest(&skill_id)
                .await
                .map_err(|e| DependencyError::SkillNotFound(e.to_string()))?;

            // Add dependencies to resolve queue
            for dep in &manifest.dependencies {
                if !dep.optional && !resolved.contains(&dep.skill_id) {
                    to_resolve.push(dep.skill_id.clone());
                }
            }

            resolver.add_skill(manifest)?;
            resolved.insert(skill_id);
        }

        resolver.resolve(skill_ids)
    }

    /// Install a skill (and its dependencies)
    pub async fn install(&self, skill_id: &str) -> Result<InstalledSkill, SkillProviderError> {
        let manifest = self.get_manifest(skill_id).await?;
        let content = self.get_content(skill_id, &manifest.version).await?;

        // Check gating requirements
        let gating_result = self.gating_checker.check(&manifest.gating);

        let status = if gating_result.passed {
            InstallStatus::Installed
        } else {
            InstallStatus::RequirementsNotMet
        };

        let now = chrono::Utc::now().timestamp_millis();
        let installed_skill = InstalledSkill {
            manifest,
            content,
            status,
            installed_at: now,
            updated_at: now,
            update_available: None,
            gating_result: Some(gating_result),
        };

        // Save to installed cache
        {
            let mut installed = self.installed.write().await;
            installed.insert(skill_id.to_string(), installed_skill.clone());
        }

        // Persist to disk
        self.save_installed_skill(&installed_skill).await?;

        Ok(installed_skill)
    }

    /// Uninstall a skill
    pub async fn uninstall(&self, skill_id: &str) -> Result<(), SkillProviderError> {
        // Remove from cache
        {
            let mut installed = self.installed.write().await;
            installed.remove(skill_id);
        }

        // Remove from disk
        let skill_dir = self.config.skills_dir.join(skill_id);
        if skill_dir.exists() {
            tokio::fs::remove_dir_all(&skill_dir)
                .await
                .map_err(SkillProviderError::Io)?;
        }

        Ok(())
    }

    /// Check for available updates
    pub async fn check_updates(&self) -> Vec<(String, SkillVersion, SkillVersion)> {
        let installed = self.installed.read().await;
        let mut updates = Vec::new();

        for (id, skill) in installed.iter() {
            match self.get_manifest(id).await {
                Ok(latest) => {
                    if latest.version != skill.manifest.version {
                        // Compare versions
                        if latest.version.compare(&skill.manifest.version) > 0 {
                            updates.push((
                                id.clone(),
                                skill.manifest.version.clone(),
                                latest.version.clone(),
                            ));
                        }
                    }
                }
                Err(e) => {
                    tracing::debug!("Could not check updates for {}: {}", id, e);
                }
            }
        }

        updates
    }

    /// Update a skill to the latest version
    pub async fn update(&self, skill_id: &str) -> Result<InstalledSkill, SkillProviderError> {
        // Just reinstall - the install method will get the latest version
        self.install(skill_id).await
    }

    /// Recheck gating requirements for installed skills
    pub async fn recheck_gating(
        &self,
        skill_id: &str,
    ) -> Result<GatingCheckResult, SkillProviderError> {
        let mut installed = self.installed.write().await;

        let skill = installed
            .get_mut(skill_id)
            .ok_or_else(|| SkillProviderError::NotFound(skill_id.to_string()))?;

        let result = self.gating_checker.check(&skill.manifest.gating);

        skill.gating_result = Some(result.clone());
        skill.status = if result.passed {
            InstallStatus::Installed
        } else {
            InstallStatus::RequirementsNotMet
        };

        Ok(result)
    }

    /// Load installed skills from disk
    pub async fn load_installed(&self) -> Result<(), SkillProviderError> {
        let skills_dir = &self.config.skills_dir;

        if !skills_dir.exists() {
            return Ok(());
        }

        let mut entries = tokio::fs::read_dir(skills_dir).await?;
        let mut installed = self.installed.write().await;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let manifest_path = path.join("skill.json");
            if !manifest_path.exists() {
                continue;
            }

            match self.load_installed_skill(&path).await {
                Ok(skill) => {
                    installed.insert(skill.manifest.id.clone(), skill);
                }
                Err(e) => {
                    tracing::warn!("Failed to load skill from {:?}: {}", path, e);
                }
            }
        }

        Ok(())
    }

    /// Save an installed skill to disk
    async fn save_installed_skill(&self, skill: &InstalledSkill) -> Result<(), SkillProviderError> {
        let skill_dir = self.config.skills_dir.join(&skill.manifest.id);
        tokio::fs::create_dir_all(&skill_dir).await?;

        // Save manifest
        let manifest_json = serde_json::to_string_pretty(&skill.manifest)
            .map_err(|e| SkillProviderError::Parse(e.to_string()))?;
        tokio::fs::write(skill_dir.join("skill.json"), manifest_json).await?;

        // Save content
        tokio::fs::write(skill_dir.join("skill.md"), &skill.content).await?;

        // Save installation metadata
        let metadata = serde_json::json!({
            "installed_at": skill.installed_at,
            "updated_at": skill.updated_at,
            "status": skill.status,
        });
        tokio::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_string_pretty(&metadata).unwrap(),
        )
        .await?;

        Ok(())
    }

    /// Load an installed skill from disk
    async fn load_installed_skill(
        &self,
        path: &std::path::Path,
    ) -> Result<InstalledSkill, SkillProviderError> {
        let manifest_path = path.join("skill.json");
        let content_path = path.join("skill.md");
        let metadata_path = path.join("metadata.json");

        let manifest: SkillManifest = {
            let content = tokio::fs::read_to_string(&manifest_path).await?;
            serde_json::from_str(&content).map_err(|e| SkillProviderError::Parse(e.to_string()))?
        };

        let content = tokio::fs::read_to_string(&content_path).await?;

        let (installed_at, updated_at, status) = if metadata_path.exists() {
            let metadata_content = tokio::fs::read_to_string(&metadata_path).await?;
            let metadata: serde_json::Value = serde_json::from_str(&metadata_content)
                .map_err(|e| SkillProviderError::Parse(e.to_string()))?;

            (
                metadata["installed_at"].as_i64().unwrap_or(0),
                metadata["updated_at"].as_i64().unwrap_or(0),
                serde_json::from_value(metadata["status"].clone())
                    .unwrap_or(InstallStatus::Installed),
            )
        } else {
            let now = chrono::Utc::now().timestamp_millis();
            (now, now, InstallStatus::Installed)
        };

        // Check gating requirements
        let gating_result = self.gating_checker.check(&manifest.gating);

        Ok(InstalledSkill {
            manifest,
            content,
            status,
            installed_at,
            updated_at,
            update_available: None,
            gating_result: Some(gating_result),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_registry_search() {
        let temp_dir = tempdir().unwrap();
        let config = SkillRegistryConfig {
            skills_dir: temp_dir.path().to_path_buf(),
            cache_dir: temp_dir.path().join("cache"),
            enable_marketplace: false,
            marketplace_url: None,
        };

        let registry = SkillRegistry::new(config);

        // Search should return empty for now (no skills)
        let results = registry.search(&SkillSearchQuery::default()).await;
        assert!(results.is_empty());
    }
}
