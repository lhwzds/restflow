//! Authentication Profile Manager
//!
//! Manages credential profiles with selection, health tracking, and failover.

use super::discoverer::CompositeDiscoverer;
use super::refresh::{AnthropicRefresher, OAuthRefresher};
use super::types::{
    AuthProfile, AuthProvider, Credential, CredentialSource, DiscoverySummary, ProfileHealth,
    ProfileSelection,
};
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Configuration for the auth profile manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthManagerConfig {
    /// Base cooldown duration in seconds (exponential backoff applied)
    #[serde(default = "default_cooldown")]
    pub cooldown_seconds: u64,
    /// Maximum consecutive failures before disabling a profile
    #[serde(default = "default_max_failures")]
    pub max_failures: u32,
    /// Whether to auto-discover credentials on initialization
    #[serde(default = "default_true")]
    pub auto_discover: bool,
    /// Path to store manual profiles
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profiles_path: Option<PathBuf>,
}

fn default_cooldown() -> u64 {
    60
}
fn default_max_failures() -> u32 {
    5
}
fn default_true() -> bool {
    true
}

impl Default for AuthManagerConfig {
    fn default() -> Self {
        Self {
            cooldown_seconds: default_cooldown(),
            max_failures: default_max_failures(),
            auto_discover: true,
            profiles_path: None,
        }
    }
}

/// Authentication Profile Manager
///
/// Manages credential profiles with:
/// - Automatic discovery from multiple sources
/// - Profile selection based on priority and health
/// - Health tracking with exponential backoff
/// - Manual profile management
pub struct AuthProfileManager {
    config: AuthManagerConfig,
    profiles: Arc<RwLock<HashMap<String, AuthProfile>>>,
    discoverer: CompositeDiscoverer,
    refreshers: HashMap<AuthProvider, Arc<dyn OAuthRefresher>>,
}

impl AuthProfileManager {
    /// Create a new auth profile manager with default config
    pub fn new() -> Self {
        Self::with_config(AuthManagerConfig::default())
    }

    /// Create a new auth profile manager with custom config
    pub fn with_config(config: AuthManagerConfig) -> Self {
        let mut refreshers: HashMap<AuthProvider, Arc<dyn OAuthRefresher>> = HashMap::new();
        refreshers.insert(AuthProvider::Anthropic, Arc::new(AnthropicRefresher::default()));

        Self {
            config,
            profiles: Arc::new(RwLock::new(HashMap::new())),
            discoverer: CompositeDiscoverer::with_defaults(),
            refreshers,
        }
    }

    /// Initialize the manager (run discovery if auto_discover is enabled)
    pub async fn initialize(&self) -> Result<DiscoverySummary> {
        if self.config.auto_discover {
            let summary = self.discover().await?;
            
            // Load any saved manual profiles
            if let Some(path) = &self.config.profiles_path
                && let Err(e) = self.load_manual_profiles(path).await
            {
                warn!(error = %e, "Failed to load manual profiles");
            }
            
            Ok(summary)
        } else {
            Ok(DiscoverySummary::default())
        }
    }

    /// Run credential discovery
    pub async fn discover(&self) -> Result<DiscoverySummary> {
        let (discovered_profiles, summary) = self.discoverer.discover_all().await;

        let mut profiles = self.profiles.write().await;

        for profile in discovered_profiles {
            // Don't overwrite existing profiles with same source credentials
            let exists = profiles.values().any(|p| {
                p.source == profile.source
                    && p.provider == profile.provider
                    && p.credential.get_email() == profile.credential.get_email()
            });

            if !exists {
                profiles.insert(profile.id.clone(), profile);
            }
        }

        info!(
            total_profiles = profiles.len(),
            discovered = summary.total,
            "Discovery complete"
        );

        Ok(summary)
    }

    /// Get all profiles
    pub async fn list_profiles(&self) -> Vec<AuthProfile> {
        let profiles = self.profiles.read().await;
        profiles.values().cloned().collect()
    }

    /// Get profiles for a specific provider
    pub async fn get_profiles_for_provider(&self, provider: AuthProvider) -> Vec<AuthProfile> {
        let profiles = self.profiles.read().await;
        profiles
            .values()
            .filter(|p| p.provider == provider)
            .cloned()
            .collect()
    }

    /// Get available profiles (enabled, not expired, not in cooldown)
    pub async fn get_available_profiles(&self) -> Vec<AuthProfile> {
        let profiles = self.profiles.read().await;
        profiles
            .values()
            .filter(|p| p.is_available())
            .cloned()
            .collect()
    }

    /// Get available profiles for a specific provider
    pub async fn get_available_for_provider(&self, provider: AuthProvider) -> Vec<AuthProfile> {
        let profiles = self.profiles.read().await;
        profiles
            .values()
            .filter(|p| p.is_available() && p.provider == provider)
            .cloned()
            .collect()
    }

    /// Get a specific profile by ID
    pub async fn get_profile(&self, id: &str) -> Option<AuthProfile> {
        let profiles = self.profiles.read().await;
        profiles.get(id).cloned()
    }

    /// Select the best available profile for a provider
    pub async fn select_profile(&self, provider: AuthProvider) -> Option<ProfileSelection> {
        if let Err(error) = self.refresh_expired_profiles(provider).await {
            warn!(%error, provider = %provider, "Failed to refresh expired OAuth profiles");
        }

        let profiles = self.profiles.read().await;
        
        let mut available: Vec<_> = profiles
            .values()
            .filter(|p| p.is_available() && p.provider == provider)
            .collect();

        if available.is_empty() {
            return None;
        }

        // Sort by priority (lower = higher priority), then by last used
        available.sort_by(|a, b| {
            a.priority.cmp(&b.priority).then_with(|| {
                // Prefer recently used profiles (they're known working)
                b.last_used_at.cmp(&a.last_used_at)
            })
        });

        let profile = available[0].clone();
        let alternatives = available.len() - 1;
        let reason = format!(
            "Selected {} (priority: {}, health: {:?})",
            profile.name, profile.priority, profile.health
        );

        Some(ProfileSelection {
            profile,
            reason,
            alternatives,
        })
    }

    async fn refresh_expired_profiles(&self, provider: AuthProvider) -> Result<usize> {
        let refresher = match self.refreshers.get(&provider) {
            Some(refresher) => refresher.clone(),
            None => return Ok(0),
        };

        let candidates: Vec<(String, Credential)> = {
            let profiles = self.profiles.read().await;
            profiles
                .values()
                .filter(|profile| {
                    profile.provider == provider
                        && profile.credential.is_expired()
                        && profile.credential.can_refresh()
                })
                .map(|profile| (profile.id.clone(), profile.credential.clone()))
                .collect()
        };

        if candidates.is_empty() {
            return Ok(0);
        }

        let mut refreshed = 0;

        for (profile_id, credential) in candidates {
            let email = credential.get_email().map(|value| value.to_string());
            let updated = refresher.refresh(&credential).await;
            match updated {
                Ok(updated) => {
                    let mut profiles = self.profiles.write().await;
                    if let Some(profile) = profiles.get_mut(&profile_id) {
                        profile.credential = Credential::OAuth {
                            access_token: updated.access_token,
                            refresh_token: updated
                                .refresh_token
                                .or_else(|| credential.refresh_token().map(|value| value.to_string())),
                            expires_at: updated.expires_at,
                            email,
                        };
                        refreshed += 1;
                    }
                }
                Err(error) => {
                    warn!(%error, profile_id, provider = %provider, "Failed to refresh OAuth token");
                }
            }
        }

        Ok(refreshed)
    }

    /// Get the best API key for a provider
    pub async fn get_api_key(&self, provider: AuthProvider) -> Option<String> {
        self.select_profile(provider)
            .await
            .map(|s| s.profile.get_api_key().to_string())
    }

    /// Mark a profile as successfully used
    pub async fn mark_success(&self, profile_id: &str) -> Result<()> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles
            .get_mut(profile_id)
            .ok_or_else(|| anyhow!("Profile not found: {}", profile_id))?;

        profile.mark_success();
        debug!(profile_id, "Profile marked as success");

        Ok(())
    }

    /// Mark a profile as failed
    pub async fn mark_failure(&self, profile_id: &str) -> Result<()> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles
            .get_mut(profile_id)
            .ok_or_else(|| anyhow!("Profile not found: {}", profile_id))?;

        profile.mark_failure(self.config.cooldown_seconds);

        if profile.failure_count >= self.config.max_failures {
            warn!(
                profile_id,
                failure_count = profile.failure_count,
                "Profile reached max failures, disabling"
            );
            profile.disable("Max failures reached");
        }

        debug!(
            profile_id,
            failure_count = profile.failure_count,
            cooldown_until = ?profile.cooldown_until,
            "Profile marked as failed"
        );

        Ok(())
    }

    /// Add a manual profile
    pub async fn add_profile(&self, profile: AuthProfile) -> Result<String> {
        let mut profiles = self.profiles.write().await;

        // Check for duplicates
        let exists = profiles.values().any(|p| {
            p.provider == profile.provider
                && p.credential.get_auth_value() == profile.credential.get_auth_value()
        });

        if exists {
            return Err(anyhow!(
                "A profile with this credential already exists for {}",
                profile.provider
            ));
        }

        let id = profile.id.clone();
        profiles.insert(id.clone(), profile);

        info!(profile_id = %id, "Manual profile added");

        // Save manual profiles if path is configured
        if let Some(path) = &self.config.profiles_path {
            let manual_profiles: Vec<_> = profiles
                .values()
                .filter(|p| p.source == CredentialSource::Manual)
                .cloned()
                .collect();
            
            if let Err(e) = Self::save_profiles_to_file(&manual_profiles, path).await {
                warn!(error = %e, "Failed to save manual profiles");
            }
        }

        Ok(id)
    }

    /// Remove a profile
    pub async fn remove_profile(&self, profile_id: &str) -> Result<AuthProfile> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles
            .remove(profile_id)
            .ok_or_else(|| anyhow!("Profile not found: {}", profile_id))?;

        info!(profile_id, name = %profile.name, "Profile removed");

        // Save manual profiles if path is configured
        if let Some(path) = &self.config.profiles_path {
            let manual_profiles: Vec<_> = profiles
                .values()
                .filter(|p| p.source == CredentialSource::Manual)
                .cloned()
                .collect();
            
            if let Err(e) = Self::save_profiles_to_file(&manual_profiles, path).await {
                warn!(error = %e, "Failed to save manual profiles after removal");
            }
        }

        Ok(profile)
    }

    /// Update a profile
    pub async fn update_profile(&self, profile_id: &str, update: ProfileUpdate) -> Result<AuthProfile> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles
            .get_mut(profile_id)
            .ok_or_else(|| anyhow!("Profile not found: {}", profile_id))?;

        if let Some(name) = update.name {
            profile.name = name;
        }
        if let Some(enabled) = update.enabled {
            profile.enabled = enabled;
        }
        if let Some(priority) = update.priority {
            profile.priority = priority;
        }

        let updated = profile.clone();

        info!(profile_id, name = %updated.name, "Profile updated");

        Ok(updated)
    }

    /// Enable a profile
    pub async fn enable_profile(&self, profile_id: &str) -> Result<()> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles
            .get_mut(profile_id)
            .ok_or_else(|| anyhow!("Profile not found: {}", profile_id))?;

        profile.enabled = true;
        profile.health = ProfileHealth::Unknown;
        profile.failure_count = 0;
        profile.cooldown_until = None;

        info!(profile_id, "Profile enabled");
        Ok(())
    }

    /// Disable a profile
    pub async fn disable_profile(&self, profile_id: &str, reason: &str) -> Result<()> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles
            .get_mut(profile_id)
            .ok_or_else(|| anyhow!("Profile not found: {}", profile_id))?;

        profile.disable(reason);
        Ok(())
    }

    /// Get a summary of all profiles
    pub async fn get_summary(&self) -> ManagerSummary {
        let profiles = self.profiles.read().await;

        let total = profiles.len();
        let enabled = profiles.values().filter(|p| p.enabled).count();
        let available = profiles.values().filter(|p| p.is_available()).count();
        let in_cooldown = profiles
            .values()
            .filter(|p| p.health == ProfileHealth::Cooldown)
            .count();
        let disabled = profiles
            .values()
            .filter(|p| p.health == ProfileHealth::Disabled)
            .count();

        let mut by_provider: HashMap<String, usize> = HashMap::new();
        let mut by_source: HashMap<String, usize> = HashMap::new();

        for profile in profiles.values() {
            *by_provider.entry(profile.provider.to_string()).or_insert(0) += 1;
            *by_source.entry(profile.source.to_string()).or_insert(0) += 1;
        }

        ManagerSummary {
            total,
            enabled,
            available,
            in_cooldown,
            disabled,
            by_provider,
            by_source,
        }
    }

    /// Clear all profiles
    pub async fn clear(&self) {
        let mut profiles = self.profiles.write().await;
        profiles.clear();
        info!("All profiles cleared");
    }

    /// Load manual profiles from a file
    async fn load_manual_profiles(&self, path: &PathBuf) -> Result<()> {
        if !path.exists() {
            return Ok(());
        }

        let content = tokio::fs::read_to_string(path)
            .await
            .context("Failed to read profiles file")?;

        let loaded_profiles: Vec<AuthProfile> =
            serde_json::from_str(&content).context("Failed to parse profiles file")?;

        let mut profiles = self.profiles.write().await;
        for profile in loaded_profiles {
            if profile.source == CredentialSource::Manual {
                profiles.insert(profile.id.clone(), profile);
            }
        }

        Ok(())
    }

    /// Save profiles to a file
    async fn save_profiles_to_file(profiles: &[AuthProfile], path: &PathBuf) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let content = serde_json::to_string_pretty(profiles)?;
        tokio::fs::write(path, content).await?;

        Ok(())
    }
}

impl Default for AuthProfileManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Update request for a profile
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileUpdate {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
}

/// Summary of the auth profile manager state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagerSummary {
    pub total: usize,
    pub enabled: usize,
    pub available: usize,
    pub in_cooldown: usize,
    pub disabled: usize,
    pub by_provider: HashMap<String, usize>,
    pub by_source: HashMap<String, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::Credential;

    fn create_test_profile(name: &str, provider: AuthProvider) -> AuthProfile {
        AuthProfile::new(
            name,
            Credential::ApiKey {
                key: format!("test-key-{}", name),
                email: None,
            },
            CredentialSource::Manual,
            provider,
        )
    }

    #[test]
    fn test_credential_refresh_token_extracts_oauth() {
        let cred = Credential::OAuth {
            access_token: "access".to_string(),
            refresh_token: Some("refresh".to_string()),
            expires_at: None,
            email: None,
        };

        assert_eq!(cred.refresh_token(), Some("refresh"));
    }

    #[test]
    fn test_credential_refresh_token_returns_none_for_api_key() {
        let cred = Credential::ApiKey {
            key: "key".to_string(),
            email: None,
        };

        assert_eq!(cred.refresh_token(), None);
    }

    #[test]
    fn test_credential_refresh_token_returns_none_without_refresh() {
        let cred = Credential::OAuth {
            access_token: "access".to_string(),
            refresh_token: None,
            expires_at: None,
            email: None,
        };

        assert_eq!(cred.refresh_token(), None);
    }

        AuthProfile::new(
            name,
            Credential::ApiKey {
                key: format!("test-key-{}", name),
                email: None,
            },
            CredentialSource::Manual,
            provider,
        )
    }

    #[tokio::test]
    async fn test_manager_new() {
        let manager = AuthProfileManager::new();
        let profiles = manager.list_profiles().await;
        assert!(profiles.is_empty());
    }

    #[tokio::test]
    async fn test_manager_add_profile() {
        let manager = AuthProfileManager::new();
        let profile = create_test_profile("Test", AuthProvider::Anthropic);

        let id = manager.add_profile(profile).await.unwrap();
        assert!(!id.is_empty());

        let profiles = manager.list_profiles().await;
        assert_eq!(profiles.len(), 1);
    }

    #[tokio::test]
    async fn test_manager_add_duplicate_profile() {
        let manager = AuthProfileManager::new();
        
        let profile1 = AuthProfile::new(
            "Test 1",
            Credential::ApiKey {
                key: "same-key".to_string(),
                email: None,
            },
            CredentialSource::Manual,
            AuthProvider::Anthropic,
        );
        
        let profile2 = AuthProfile::new(
            "Test 2",
            Credential::ApiKey {
                key: "same-key".to_string(),
                email: None,
            },
            CredentialSource::Manual,
            AuthProvider::Anthropic,
        );

        manager.add_profile(profile1).await.unwrap();
        let result = manager.add_profile(profile2).await;
        
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_manager_remove_profile() {
        let manager = AuthProfileManager::new();
        let profile = create_test_profile("Test", AuthProvider::Anthropic);
        let id = manager.add_profile(profile).await.unwrap();

        let removed = manager.remove_profile(&id).await.unwrap();
        assert_eq!(removed.name, "Test");

        let profiles = manager.list_profiles().await;
        assert!(profiles.is_empty());
    }

    #[tokio::test]
    async fn test_manager_get_profile() {
        let manager = AuthProfileManager::new();
        let profile = create_test_profile("Test", AuthProvider::Anthropic);
        let id = manager.add_profile(profile).await.unwrap();

        let retrieved = manager.get_profile(&id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Test");

        let nonexistent = manager.get_profile("nonexistent").await;
        assert!(nonexistent.is_none());
    }

    #[tokio::test]
    async fn test_manager_select_profile() {
        let manager = AuthProfileManager::new();

        let profile1 = create_test_profile("Low Priority", AuthProvider::Anthropic);
        let mut profile2 = create_test_profile("High Priority", AuthProvider::Anthropic);
        profile2.priority = -1; // Higher priority (lower number)

        manager.add_profile(profile1).await.unwrap();
        manager.add_profile(profile2).await.unwrap();

        let selection = manager.select_profile(AuthProvider::Anthropic).await;
        assert!(selection.is_some());
        
        let selection = selection.unwrap();
        assert_eq!(selection.profile.name, "High Priority");
        assert_eq!(selection.alternatives, 1);
    }

    #[tokio::test]
    async fn test_manager_select_no_profiles() {
        let manager = AuthProfileManager::new();
        let selection = manager.select_profile(AuthProvider::Anthropic).await;
        assert!(selection.is_none());
    }

    #[tokio::test]
    async fn test_manager_mark_success() {
        let manager = AuthProfileManager::new();
        let profile = create_test_profile("Test", AuthProvider::Anthropic);
        let id = manager.add_profile(profile).await.unwrap();

        manager.mark_success(&id).await.unwrap();

        let profile = manager.get_profile(&id).await.unwrap();
        assert_eq!(profile.health, ProfileHealth::Healthy);
        assert!(profile.last_used_at.is_some());
    }

    #[tokio::test]
    async fn test_manager_mark_failure() {
        let manager = AuthProfileManager::new();
        let profile = create_test_profile("Test", AuthProvider::Anthropic);
        let id = manager.add_profile(profile).await.unwrap();

        manager.mark_failure(&id).await.unwrap();

        let profile = manager.get_profile(&id).await.unwrap();
        assert_eq!(profile.health, ProfileHealth::Cooldown);
        assert_eq!(profile.failure_count, 1);
        assert!(profile.cooldown_until.is_some());
    }

    #[tokio::test]
    async fn test_manager_max_failures() {
        let config = AuthManagerConfig {
            max_failures: 3,
            ..Default::default()
        };
        let manager = AuthProfileManager::with_config(config);
        let profile = create_test_profile("Test", AuthProvider::Anthropic);
        let id = manager.add_profile(profile).await.unwrap();

        // Fail 3 times
        for _ in 0..3 {
            manager.mark_failure(&id).await.unwrap();
        }

        let profile = manager.get_profile(&id).await.unwrap();
        assert_eq!(profile.health, ProfileHealth::Disabled);
        assert!(!profile.enabled);
    }

    #[tokio::test]
    async fn test_manager_enable_disable_profile() {
        let manager = AuthProfileManager::new();
        let profile = create_test_profile("Test", AuthProvider::Anthropic);
        let id = manager.add_profile(profile).await.unwrap();

        manager.disable_profile(&id, "Test reason").await.unwrap();
        let profile = manager.get_profile(&id).await.unwrap();
        assert!(!profile.enabled);
        assert_eq!(profile.health, ProfileHealth::Disabled);

        manager.enable_profile(&id).await.unwrap();
        let profile = manager.get_profile(&id).await.unwrap();
        assert!(profile.enabled);
        assert_eq!(profile.health, ProfileHealth::Unknown);
    }

    #[tokio::test]
    async fn test_manager_update_profile() {
        let manager = AuthProfileManager::new();
        let profile = create_test_profile("Test", AuthProvider::Anthropic);
        let id = manager.add_profile(profile).await.unwrap();

        let update = ProfileUpdate {
            name: Some("Updated Name".to_string()),
            priority: Some(10),
            enabled: None,
        };

        let updated = manager.update_profile(&id, update).await.unwrap();
        assert_eq!(updated.name, "Updated Name");
        assert_eq!(updated.priority, 10);
    }

    #[tokio::test]
    async fn test_manager_get_profiles_for_provider() {
        let manager = AuthProfileManager::new();

        manager
            .add_profile(create_test_profile("Anthropic 1", AuthProvider::Anthropic))
            .await
            .unwrap();
        manager
            .add_profile(create_test_profile("Anthropic 2", AuthProvider::Anthropic))
            .await
            .unwrap();
        manager
            .add_profile(create_test_profile("OpenAI 1", AuthProvider::OpenAI))
            .await
            .unwrap();

        let anthropic_profiles = manager
            .get_profiles_for_provider(AuthProvider::Anthropic)
            .await;
        assert_eq!(anthropic_profiles.len(), 2);

        let openai_profiles = manager
            .get_profiles_for_provider(AuthProvider::OpenAI)
            .await;
        assert_eq!(openai_profiles.len(), 1);
    }

    #[tokio::test]
    async fn test_manager_get_available_profiles() {
        let manager = AuthProfileManager::new();

        let profile1 = create_test_profile("Available", AuthProvider::Anthropic);
        let mut profile2 = create_test_profile("Disabled", AuthProvider::Anthropic);
        profile2.enabled = false;

        let id1 = manager.add_profile(profile1).await.unwrap();
        manager.add_profile(profile2).await.unwrap();

        let available = manager.get_available_profiles().await;
        assert_eq!(available.len(), 1);
        assert_eq!(available[0].id, id1);
    }

    #[tokio::test]
    async fn test_manager_get_api_key() {
        let manager = AuthProfileManager::new();
        let profile = create_test_profile("Test", AuthProvider::Anthropic);
        manager.add_profile(profile).await.unwrap();

        let key = manager.get_api_key(AuthProvider::Anthropic).await;
        assert!(key.is_some());
        assert!(key.unwrap().starts_with("test-key-"));

        let no_key = manager.get_api_key(AuthProvider::OpenAI).await;
        assert!(no_key.is_none());
    }

    #[tokio::test]
    async fn test_manager_get_summary() {
        let manager = AuthProfileManager::new();

        manager
            .add_profile(create_test_profile("P1", AuthProvider::Anthropic))
            .await
            .unwrap();
        manager
            .add_profile(create_test_profile("P2", AuthProvider::Anthropic))
            .await
            .unwrap();
        manager
            .add_profile(create_test_profile("P3", AuthProvider::OpenAI))
            .await
            .unwrap();

        let summary = manager.get_summary().await;
        assert_eq!(summary.total, 3);
        assert_eq!(summary.enabled, 3);
        assert_eq!(summary.available, 3);
        assert_eq!(*summary.by_provider.get("Anthropic").unwrap_or(&0), 2);
        assert_eq!(*summary.by_provider.get("OpenAI").unwrap_or(&0), 1);
    }

    #[tokio::test]
    async fn test_manager_clear() {
        let manager = AuthProfileManager::new();

        manager
            .add_profile(create_test_profile("P1", AuthProvider::Anthropic))
            .await
            .unwrap();
        manager
            .add_profile(create_test_profile("P2", AuthProvider::OpenAI))
            .await
            .unwrap();

        manager.clear().await;

        let profiles = manager.list_profiles().await;
        assert!(profiles.is_empty());
    }

    #[tokio::test]
    async fn test_profile_update_partial() {
        let manager = AuthProfileManager::new();
        let profile = create_test_profile("Test", AuthProvider::Anthropic);
        let id = manager.add_profile(profile).await.unwrap();

        // Only update name
        let update = ProfileUpdate {
            name: Some("New Name".to_string()),
            enabled: None,
            priority: None,
        };

        let updated = manager.update_profile(&id, update).await.unwrap();
        assert_eq!(updated.name, "New Name");
        assert!(updated.enabled); // Should remain unchanged
        assert_eq!(updated.priority, 0); // Should remain unchanged
    }
}
