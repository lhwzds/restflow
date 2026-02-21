//! Authentication Profile Manager
//!
//! Manages credential profiles with selection, health tracking, and failover.

use super::discoverer::{CompositeDiscoverer, DiscoveredProfile};
use super::refresh::{AnthropicRefresher, OAuthRefresher};
use super::resolver::CredentialResolver;
use super::types::{
    AuthProfile, AuthProvider, Credential, CredentialSource, DiscoverySummary, ProfileHealth,
    ProfileSelection, SecureCredential,
};
use super::writer::CredentialWriter;
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::Provider;
use crate::storage::SecretStorage;
use restflow_storage::{AuthProfileStorage, SimpleStorage};

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
/// - Secure credential storage via SecretStorage
pub struct AuthProfileManager {
    config: AuthManagerConfig,
    profiles: Arc<RwLock<HashMap<String, AuthProfile>>>,
    discoverer: CompositeDiscoverer,
    refreshers: HashMap<AuthProvider, Arc<dyn OAuthRefresher>>,
    /// Resolver for reading secrets
    resolver: CredentialResolver,
    /// Writer for storing secrets
    writer: CredentialWriter,
    /// Optional database storage for manual profiles
    storage: Option<AuthProfileStorage>,
}

impl AuthProfileManager {
    /// Create a new auth profile manager with secret storage
    pub fn new(secrets: Arc<SecretStorage>) -> Self {
        Self::with_config(AuthManagerConfig::default(), secrets)
    }

    /// Create a new auth profile manager with custom config and secret storage
    pub fn with_config(config: AuthManagerConfig, secrets: Arc<SecretStorage>) -> Self {
        Self::with_storage(config, secrets, None)
    }

    /// Create a new auth profile manager with custom config, secret storage, and profile storage
    pub fn with_storage(
        config: AuthManagerConfig,
        secrets: Arc<SecretStorage>,
        storage: Option<AuthProfileStorage>,
    ) -> Self {
        let mut refreshers: HashMap<AuthProvider, Arc<dyn OAuthRefresher>> = HashMap::new();
        // ClaudeCode OAuth tokens can be refreshed using Anthropic's OAuth endpoint
        refreshers.insert(
            AuthProvider::ClaudeCode,
            Arc::new(AnthropicRefresher::default()),
        );

        let resolver = CredentialResolver::new(secrets.clone());
        let writer = CredentialWriter::new(secrets.clone());

        Self {
            config,
            profiles: Arc::new(RwLock::new(HashMap::new())),
            discoverer: CompositeDiscoverer::with_defaults(),
            refreshers,
            resolver,
            writer,
            storage,
        }
    }

    /// Initialize the manager (run discovery if auto_discover is enabled)
    pub async fn initialize(&self) -> Result<DiscoverySummary> {
        if let Err(e) = self.load_profiles_from_storage().await {
            warn!(error = %e, "Failed to load manual profiles from storage");
        }

        if self.config.auto_discover {
            return self.discover().await;
        }

        Ok(DiscoverySummary::default())
    }

    /// Run credential discovery
    pub async fn discover(&self) -> Result<DiscoverySummary> {
        let (discovered_profiles, summary) = self.discoverer.discover_all().await;

        let mut profiles = self.profiles.write().await;

        for discovered in discovered_profiles {
            // Use deterministic IDs for discovered profiles so repeated discovery does not
            // create unbounded auth:* secret keys across restarts.
            let profile_id = Self::discovered_profile_id(&discovered);
            if profiles.contains_key(&profile_id) {
                continue;
            }

            match self
                .writer
                .store_credential(&profile_id, &discovered.credential)
            {
                Ok(secure_credential) => {
                    let profile = AuthProfile::new_with_id(
                        profile_id.clone(),
                        discovered.name,
                        secure_credential,
                        discovered.source,
                        discovered.provider,
                    );
                    profiles.insert(profile_id, profile);
                }
                Err(e) => {
                    warn!(error = %e, "Failed to store discovered credential securely");
                }
            }
        }

        info!(
            total_profiles = profiles.len(),
            discovered = summary.total,
            "Discovery complete"
        );

        Ok(summary)
    }

    fn discovered_profile_id(discovered: &DiscoveredProfile) -> String {
        let identity = discovered
            .credential
            .get_email()
            .map(|email| email.trim().to_ascii_lowercase())
            .filter(|email| !email.is_empty())
            .unwrap_or_else(|| discovered.name.trim().to_ascii_lowercase());

        let seed = format!(
            "auth-discovered:{}:{}:{}",
            discovered.source, discovered.provider, identity
        );

        let mut hasher = Sha256::new();
        hasher.update(seed.as_bytes());
        let digest = hasher.finalize();

        let mut bytes = [0_u8; 16];
        bytes.copy_from_slice(&digest[..16]);
        // Set RFC4122 variant and a deterministic v5-like version marker.
        bytes[6] = (bytes[6] & 0x0f) | 0x50;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;

        Uuid::from_bytes(bytes).to_string()
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

    /// Get available profiles compatible with a model provider.
    ///
    /// Returned profiles are sorted by:
    /// 1. Priority (lower value first)
    /// 2. Least recently used first
    pub async fn get_compatible_profiles_for_model_provider(
        &self,
        provider: Provider,
    ) -> Vec<AuthProfile> {
        let compatible = AuthProvider::compatible_with(provider);
        let profiles = self.profiles.read().await;
        let mut candidates: Vec<AuthProfile> = profiles
            .values()
            .filter(|profile| profile.is_available() && compatible.contains(&profile.provider))
            .cloned()
            .collect();

        candidates.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then_with(|| a.last_used_at.cmp(&b.last_used_at))
        });

        candidates
    }

    /// Get the best available profile for a specific provider.
    pub async fn get_available_profile(&self, provider: AuthProvider) -> Option<AuthProfile> {
        self.select_profile(provider)
            .await
            .map(|selection| selection.profile)
    }

    /// Get the best available profile compatible with a model provider.
    pub async fn get_credential_for_model(&self, provider: Provider) -> Option<AuthProfile> {
        let compatible = AuthProvider::compatible_with(provider);
        for auth_provider in compatible {
            if let Some(profile) = self.get_available_profile(auth_provider).await {
                return Some(profile);
            }
        }
        None
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

        // Collect candidates that need refresh
        let candidates: Vec<(String, SecureCredential)> = {
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

        for (profile_id, secure_credential) in candidates {
            // Resolve current tokens
            let access_token = match self.resolver.resolve_auth_value(&secure_credential) {
                Ok(token) => token,
                Err(e) => {
                    warn!(%e, profile_id, "Failed to resolve access token for refresh");
                    continue;
                }
            };

            let refresh_token = match self.resolver.resolve_refresh_token(&secure_credential) {
                Ok(Some(token)) => token,
                Ok(None) => {
                    warn!(profile_id, "No refresh token available");
                    continue;
                }
                Err(e) => {
                    warn!(%e, profile_id, "Failed to resolve refresh token");
                    continue;
                }
            };

            // Create temporary Credential for refresher
            let temp_credential = Credential::OAuth {
                access_token,
                refresh_token: Some(refresh_token),
                expires_at: None,
                email: secure_credential.get_email().map(|s| s.to_string()),
            };

            match refresher.refresh(&temp_credential).await {
                Ok(updated) => {
                    // Update secrets
                    if let Err(e) = self.writer.update_secret(
                        secure_credential.primary_secret_ref(),
                        &updated.access_token,
                    ) {
                        warn!(%e, profile_id, "Failed to update access token secret");
                        continue;
                    }

                    // Update refresh token if provided
                    if let Some(new_refresh) = &updated.refresh_token
                        && let Some(refresh_ref) = secure_credential.refresh_token_ref()
                        && let Err(e) = self.writer.update_secret(refresh_ref, new_refresh)
                    {
                        warn!(%e, profile_id, "Failed to update refresh token secret");
                    }

                    // Update profile metadata
                    let mut profiles = self.profiles.write().await;
                    if let Some(profile) = profiles.get_mut(&profile_id) {
                        profile.credential.update_oauth_metadata(
                            updated.refresh_token.and_then(|_| {
                                secure_credential.refresh_token_ref().map(|s| s.to_string())
                            }),
                            updated.expires_at,
                        );
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
        let selected = self.select_profile(provider).await?;
        match selected.profile.get_api_key(&self.resolver) {
            Ok(key) => Some(key),
            Err(error) => {
                warn!(%error, "Failed to retrieve API key");
                None
            }
        }
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

    /// Add a profile from a plaintext credential (stores securely)
    pub async fn add_profile_from_credential(
        &self,
        name: impl Into<String>,
        credential: Credential,
        source: CredentialSource,
        provider: AuthProvider,
    ) -> Result<String> {
        let profile_id = Uuid::new_v4().to_string();

        // Check for duplicates
        {
            let profiles = self.profiles.read().await;
            let exists = profiles.values().any(|p| {
                p.provider == provider
                    && self.resolver.resolve_auth_value(&p.credential).ok()
                        == Some(credential.get_auth_value().to_string())
            });

            if exists {
                return Err(anyhow!(
                    "A profile with this credential already exists for {}",
                    provider
                ));
            }
        }

        // Store credential securely
        let secure_credential = self.writer.store_credential(&profile_id, &credential)?;

        let profile = AuthProfile::new_with_id(
            profile_id.clone(),
            name,
            secure_credential,
            source,
            provider,
        );

        let mut profiles = self.profiles.write().await;
        profiles.insert(profile_id.clone(), profile.clone());

        info!(profile_id = %profile_id, "Manual profile added");

        if source == CredentialSource::Manual
            && let Err(e) = self.save_profile_to_storage(&profile)
        {
            warn!(error = %e, "Failed to save manual profile to storage");
        }

        Ok(profile_id)
    }

    /// Add a profile (for internal use with already-secure credentials)
    pub async fn add_profile(&self, profile: AuthProfile) -> Result<String> {
        let mut profiles = self.profiles.write().await;

        // Check for duplicates by resolving values
        let new_value = self.resolver.resolve_auth_value(&profile.credential).ok();
        let exists = profiles.values().any(|p| {
            p.provider == profile.provider
                && self.resolver.resolve_auth_value(&p.credential).ok() == new_value
        });

        if exists {
            return Err(anyhow!(
                "A profile with this credential already exists for {}",
                profile.provider
            ));
        }

        let id = profile.id.clone();
        profiles.insert(id.clone(), profile);

        info!(profile_id = %id, "Profile added");

        if let Some(stored) = profiles.get(&id)
            && stored.source == CredentialSource::Manual
            && let Err(e) = self.save_profile_to_storage(stored)
        {
            warn!(error = %e, "Failed to save manual profile to storage");
        }

        Ok(id)
    }

    /// Remove a profile
    pub async fn remove_profile(&self, profile_id: &str) -> Result<AuthProfile> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles
            .remove(profile_id)
            .ok_or_else(|| anyhow!("Profile not found: {}", profile_id))?;

        // Delete associated secrets
        if let Err(e) = self.writer.delete_credential(&profile.credential) {
            warn!(error = %e, profile_id, "Failed to delete credential secrets");
        }

        info!(profile_id, name = %profile.name, "Profile removed");

        if profile.source == CredentialSource::Manual
            && let Err(e) = self.delete_profile_from_storage(profile_id)
        {
            warn!(error = %e, "Failed to delete manual profile from storage");
        }

        Ok(profile)
    }

    /// Update a profile
    pub async fn update_profile(
        &self,
        profile_id: &str,
        update: ProfileUpdate,
    ) -> Result<AuthProfile> {
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

        if updated.source == CredentialSource::Manual
            && let Err(e) = self.save_profile_to_storage(&updated)
        {
            warn!(error = %e, "Failed to persist manual profile update");
        }

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

        let updated = profile.clone();
        info!(profile_id, "Profile enabled");

        if updated.source == CredentialSource::Manual
            && let Err(e) = self.save_profile_to_storage(&updated)
        {
            warn!(error = %e, "Failed to persist manual profile enable");
        }

        Ok(())
    }

    /// Disable a profile
    pub async fn disable_profile(&self, profile_id: &str, reason: &str) -> Result<()> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles
            .get_mut(profile_id)
            .ok_or_else(|| anyhow!("Profile not found: {}", profile_id))?;

        profile.disable(reason);

        let updated = profile.clone();
        if updated.source == CredentialSource::Manual
            && let Err(e) = self.save_profile_to_storage(&updated)
        {
            warn!(error = %e, "Failed to persist manual profile disable");
        }

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

        // Delete all secrets
        for profile in profiles.values() {
            if let Err(e) = self.writer.delete_credential(&profile.credential) {
                warn!(error = %e, profile_id = %profile.id, "Failed to delete credential secrets on clear");
            }
        }

        if let Some(storage) = &self.storage {
            for profile in profiles.values() {
                if profile.source == CredentialSource::Manual
                    && let Err(e) = storage.delete(profile.id.as_str())
                {
                    warn!(error = %e, profile_id = %profile.id, "Failed to delete manual profile from storage");
                }
            }
        }

        profiles.clear();
        info!("All profiles cleared");
    }

    /// Get the credential resolver for external use
    pub fn resolver(&self) -> &CredentialResolver {
        &self.resolver
    }

    fn save_profile_to_storage(&self, profile: &AuthProfile) -> Result<()> {
        let Some(storage) = &self.storage else {
            return Ok(());
        };
        let data = serde_json::to_vec(profile)?;
        storage.put_raw(profile.id.as_str(), &data)?;
        Ok(())
    }

    fn delete_profile_from_storage(&self, profile_id: &str) -> Result<()> {
        let Some(storage) = &self.storage else {
            return Ok(());
        };
        storage.delete(profile_id)?;
        Ok(())
    }

    async fn load_profiles_from_storage(&self) -> Result<()> {
        let Some(storage) = &self.storage else {
            return Ok(());
        };
        let entries = storage.list_raw()?;
        let mut profiles = self.profiles.write().await;
        for (_, bytes) in entries {
            let profile: AuthProfile = match serde_json::from_slice(&bytes) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(error = %e, "Skipping corrupt auth profile entry");
                    continue;
                }
            };
            if profile.source == CredentialSource::Manual {
                profiles.insert(profile.id.clone(), profile);
            }
        }
        Ok(())
    }

    /// Migrate profiles from legacy JSON file into the database.
    pub async fn migrate_from_json(&self, json_path: &Path) -> Result<usize> {
        if !json_path.exists() {
            return Ok(0);
        }

        let content = tokio::fs::read_to_string(json_path)
            .await
            .context("Failed to read profiles file")?;
        let profiles: Vec<AuthProfile> =
            serde_json::from_str(&content).context("Failed to parse profiles file")?;

        let Some(storage) = &self.storage else {
            return Err(anyhow!("No storage configured"));
        };

        let mut count = 0;
        for profile in profiles {
            let data = serde_json::to_vec(&profile)?;
            storage.put_raw(profile.id.as_str(), &data)?;
            count += 1;
        }

        info!(count, "Migrated auth profiles from JSON to database");
        Ok(count)
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
    use redb::Database;
    use tempfile::TempDir;

    fn create_test_secrets() -> (Arc<SecretStorage>, TempDir) {
        let dir = TempDir::new().unwrap();
        let db = Arc::new(Database::create(dir.path().join("test.db")).unwrap());
        let secrets = Arc::new(SecretStorage::new(db).unwrap());
        (secrets, dir)
    }

    fn create_test_profile(
        secrets: &Arc<SecretStorage>,
        name: &str,
        provider: AuthProvider,
    ) -> AuthProfile {
        let writer = CredentialWriter::new(secrets.clone());
        let profile_id = Uuid::new_v4().to_string();
        let credential = Credential::ApiKey {
            key: format!("test-key-{}", name),
            email: None,
        };
        let secure = writer.store_credential(&profile_id, &credential).unwrap();
        AuthProfile::new_with_id(profile_id, name, secure, CredentialSource::Manual, provider)
    }

    fn create_discovered_profile(
        name: &str,
        source: CredentialSource,
        provider: AuthProvider,
        credential: Credential,
    ) -> DiscoveredProfile {
        DiscoveredProfile {
            name: name.to_string(),
            credential,
            source,
            provider,
        }
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

    #[tokio::test]
    async fn test_manager_new() {
        let (secrets, _dir) = create_test_secrets();
        let manager = AuthProfileManager::new(secrets);
        let profiles = manager.list_profiles().await;
        assert!(profiles.is_empty());
    }

    #[tokio::test]
    async fn test_manager_add_profile_from_credential() {
        let (secrets, _dir) = create_test_secrets();
        let manager = AuthProfileManager::new(secrets);

        let credential = Credential::ApiKey {
            key: "test-key".to_string(),
            email: None,
        };

        let id = manager
            .add_profile_from_credential(
                "Test",
                credential,
                CredentialSource::Manual,
                AuthProvider::Anthropic,
            )
            .await
            .unwrap();
        assert!(!id.is_empty());

        let profiles = manager.list_profiles().await;
        assert_eq!(profiles.len(), 1);
    }

    #[tokio::test]
    async fn test_manager_add_duplicate_profile() {
        let (secrets, _dir) = create_test_secrets();
        let manager = AuthProfileManager::new(secrets);

        let credential1 = Credential::ApiKey {
            key: "same-key".to_string(),
            email: None,
        };

        let credential2 = Credential::ApiKey {
            key: "same-key".to_string(),
            email: None,
        };

        manager
            .add_profile_from_credential(
                "Test 1",
                credential1,
                CredentialSource::Manual,
                AuthProvider::Anthropic,
            )
            .await
            .unwrap();
        let result = manager
            .add_profile_from_credential(
                "Test 2",
                credential2,
                CredentialSource::Manual,
                AuthProvider::Anthropic,
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_manager_remove_profile() {
        let (secrets, _dir) = create_test_secrets();
        let manager = AuthProfileManager::new(secrets.clone());
        let profile = create_test_profile(&secrets, "Test", AuthProvider::Anthropic);
        let id = manager.add_profile(profile).await.unwrap();

        let removed = manager.remove_profile(&id).await.unwrap();
        assert_eq!(removed.name, "Test");

        let profiles = manager.list_profiles().await;
        assert!(profiles.is_empty());
    }

    #[tokio::test]
    async fn test_manager_get_profile() {
        let (secrets, _dir) = create_test_secrets();
        let manager = AuthProfileManager::new(secrets.clone());
        let profile = create_test_profile(&secrets, "Test", AuthProvider::Anthropic);
        let id = manager.add_profile(profile).await.unwrap();

        let retrieved = manager.get_profile(&id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Test");

        let nonexistent = manager.get_profile("nonexistent").await;
        assert!(nonexistent.is_none());
    }

    #[tokio::test]
    async fn test_manager_select_profile() {
        let (secrets, _dir) = create_test_secrets();
        let manager = AuthProfileManager::new(secrets.clone());

        let profile1 = create_test_profile(&secrets, "Low Priority", AuthProvider::Anthropic);
        let mut profile2 = create_test_profile(&secrets, "High Priority", AuthProvider::Anthropic);
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
        let (secrets, _dir) = create_test_secrets();
        let manager = AuthProfileManager::new(secrets);
        let selection = manager.select_profile(AuthProvider::Anthropic).await;
        assert!(selection.is_none());
    }

    #[tokio::test]
    async fn test_manager_mark_success() {
        let (secrets, _dir) = create_test_secrets();
        let manager = AuthProfileManager::new(secrets.clone());
        let profile = create_test_profile(&secrets, "Test", AuthProvider::Anthropic);
        let id = manager.add_profile(profile).await.unwrap();

        manager.mark_success(&id).await.unwrap();

        let profile = manager.get_profile(&id).await.unwrap();
        assert_eq!(profile.health, ProfileHealth::Healthy);
        assert!(profile.last_used_at.is_some());
    }

    #[tokio::test]
    async fn test_manager_mark_failure() {
        let (secrets, _dir) = create_test_secrets();
        let manager = AuthProfileManager::new(secrets.clone());
        let profile = create_test_profile(&secrets, "Test", AuthProvider::Anthropic);
        let id = manager.add_profile(profile).await.unwrap();

        manager.mark_failure(&id).await.unwrap();

        let profile = manager.get_profile(&id).await.unwrap();
        assert_eq!(profile.health, ProfileHealth::Cooldown);
        assert_eq!(profile.failure_count, 1);
        assert!(profile.cooldown_until.is_some());
    }

    #[tokio::test]
    async fn test_manager_max_failures() {
        let (secrets, _dir) = create_test_secrets();
        let config = AuthManagerConfig {
            max_failures: 3,
            ..Default::default()
        };
        let manager = AuthProfileManager::with_config(config, secrets.clone());
        let profile = create_test_profile(&secrets, "Test", AuthProvider::Anthropic);
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
        let (secrets, _dir) = create_test_secrets();
        let manager = AuthProfileManager::new(secrets.clone());
        let profile = create_test_profile(&secrets, "Test", AuthProvider::Anthropic);
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
    async fn test_manager_enable_disable_persists_to_storage() {
        use redb::Database;

        let dir = TempDir::new().unwrap();
        let db = Arc::new(Database::create(dir.path().join("test.db")).unwrap());
        let secrets = Arc::new(SecretStorage::new(db.clone()).unwrap());
        let storage = AuthProfileStorage::new(db).unwrap();

        // Create manager with storage
        let manager = AuthProfileManager::with_storage(
            AuthManagerConfig::default(),
            secrets.clone(),
            Some(storage.clone()),
        );
        manager.initialize().await.unwrap();

        // Add a manual profile
        let credential = Credential::ApiKey {
            key: "test-key-persist".to_string(),
            email: None,
        };
        let id = manager
            .add_profile_from_credential(
                "Test Persist",
                credential,
                CredentialSource::Manual,
                AuthProvider::Anthropic,
            )
            .await
            .unwrap();

        // Disable the profile
        manager
            .disable_profile(&id, "Testing disable persistence")
            .await
            .unwrap();
        let profile = manager.get_profile(&id).await.unwrap();
        assert!(!profile.enabled);

        // Create a new manager instance (simulates IPC request creating fresh manager)
        let manager2 = AuthProfileManager::with_storage(
            AuthManagerConfig::default(),
            secrets.clone(),
            Some(storage.clone()),
        );
        manager2.initialize().await.unwrap();

        // Verify disabled state persisted
        let profile_reloaded = manager2.get_profile(&id).await.unwrap();
        assert!(
            !profile_reloaded.enabled,
            "Disabled state should persist across manager re-instantiation"
        );
        assert_eq!(profile_reloaded.health, ProfileHealth::Disabled);

        // Enable the profile
        manager2.enable_profile(&id).await.unwrap();
        let profile = manager2.get_profile(&id).await.unwrap();
        assert!(profile.enabled);

        // Create another new manager instance
        let manager3 = AuthProfileManager::with_storage(
            AuthManagerConfig::default(),
            secrets.clone(),
            Some(storage),
        );
        manager3.initialize().await.unwrap();

        // Verify enabled state persisted
        let profile_reloaded2 = manager3.get_profile(&id).await.unwrap();
        assert!(
            profile_reloaded2.enabled,
            "Enabled state should persist across manager re-instantiation"
        );
        assert_eq!(profile_reloaded2.health, ProfileHealth::Unknown);
    }

    #[tokio::test]
    async fn test_manager_update_profile() {
        let (secrets, _dir) = create_test_secrets();
        let manager = AuthProfileManager::new(secrets.clone());
        let profile = create_test_profile(&secrets, "Test", AuthProvider::Anthropic);
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
        let (secrets, _dir) = create_test_secrets();
        let manager = AuthProfileManager::new(secrets.clone());

        manager
            .add_profile(create_test_profile(
                &secrets,
                "Anthropic 1",
                AuthProvider::Anthropic,
            ))
            .await
            .unwrap();
        manager
            .add_profile(create_test_profile(
                &secrets,
                "Anthropic 2",
                AuthProvider::Anthropic,
            ))
            .await
            .unwrap();
        manager
            .add_profile(create_test_profile(
                &secrets,
                "OpenAI 1",
                AuthProvider::OpenAI,
            ))
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
    async fn test_manager_get_compatible_profiles_for_model_provider_sorted() {
        let (secrets, _dir) = create_test_secrets();
        let manager = AuthProfileManager::new(secrets.clone());

        let profile_high = create_test_profile(&secrets, "A", AuthProvider::Anthropic);
        let mut profile_low = create_test_profile(&secrets, "B", AuthProvider::ClaudeCode);
        profile_low.priority = -1;
        profile_low.last_used_at = Some(chrono::Utc::now());

        let id_high = manager.add_profile(profile_high).await.unwrap();
        let id_low = manager.add_profile(profile_low).await.unwrap();

        let profiles = manager
            .get_compatible_profiles_for_model_provider(Provider::Anthropic)
            .await;
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0].id, id_low);
        assert_eq!(profiles[1].id, id_high);
    }

    #[tokio::test]
    async fn test_manager_get_available_profiles() {
        let (secrets, _dir) = create_test_secrets();
        let manager = AuthProfileManager::new(secrets.clone());

        let profile1 = create_test_profile(&secrets, "Available", AuthProvider::Anthropic);
        let mut profile2 = create_test_profile(&secrets, "Disabled", AuthProvider::Anthropic);
        profile2.enabled = false;

        let id1 = manager.add_profile(profile1).await.unwrap();
        manager.add_profile(profile2).await.unwrap();

        let available = manager.get_available_profiles().await;
        assert_eq!(available.len(), 1);
        assert_eq!(available[0].id, id1);
    }

    #[tokio::test]
    async fn test_manager_get_api_key() {
        let (secrets, _dir) = create_test_secrets();
        let manager = AuthProfileManager::new(secrets.clone());
        let profile = create_test_profile(&secrets, "Test", AuthProvider::Anthropic);
        manager.add_profile(profile).await.unwrap();

        let key = manager.get_api_key(AuthProvider::Anthropic).await;
        assert!(key.is_some());
        assert!(key.unwrap().starts_with("test-key-"));

        let no_key = manager.get_api_key(AuthProvider::OpenAI).await;
        assert!(no_key.is_none());
    }

    #[tokio::test]
    async fn test_manager_get_summary() {
        let (secrets, _dir) = create_test_secrets();
        let manager = AuthProfileManager::new(secrets.clone());

        manager
            .add_profile(create_test_profile(&secrets, "P1", AuthProvider::Anthropic))
            .await
            .unwrap();
        manager
            .add_profile(create_test_profile(&secrets, "P2", AuthProvider::Anthropic))
            .await
            .unwrap();
        manager
            .add_profile(create_test_profile(&secrets, "P3", AuthProvider::OpenAI))
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
        let (secrets, _dir) = create_test_secrets();
        let manager = AuthProfileManager::new(secrets.clone());

        manager
            .add_profile(create_test_profile(&secrets, "P1", AuthProvider::Anthropic))
            .await
            .unwrap();
        manager
            .add_profile(create_test_profile(&secrets, "P2", AuthProvider::OpenAI))
            .await
            .unwrap();

        manager.clear().await;

        let profiles = manager.list_profiles().await;
        assert!(profiles.is_empty());
    }

    #[tokio::test]
    async fn test_profile_update_partial() {
        let (secrets, _dir) = create_test_secrets();
        let manager = AuthProfileManager::new(secrets.clone());
        let profile = create_test_profile(&secrets, "Test", AuthProvider::Anthropic);
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

    #[test]
    fn test_discovered_profile_id_stable_for_same_email_identity() {
        let p1 = create_discovered_profile(
            "Claude Code",
            CredentialSource::ClaudeCode,
            AuthProvider::ClaudeCode,
            Credential::OAuth {
                access_token: "token-a".to_string(),
                refresh_token: Some("refresh-a".to_string()),
                expires_at: None,
                email: Some("User@Example.com".to_string()),
            },
        );
        let p2 = create_discovered_profile(
            "Claude Code",
            CredentialSource::ClaudeCode,
            AuthProvider::ClaudeCode,
            Credential::OAuth {
                access_token: "token-b".to_string(),
                refresh_token: Some("refresh-b".to_string()),
                expires_at: None,
                email: Some("user@example.com".to_string()),
            },
        );

        let id1 = AuthProfileManager::discovered_profile_id(&p1);
        let id2 = AuthProfileManager::discovered_profile_id(&p2);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_discovered_profile_id_uses_name_when_email_missing() {
        let p1 = create_discovered_profile(
            "$OPENAI_API_KEY",
            CredentialSource::Environment,
            AuthProvider::OpenAI,
            Credential::ApiKey {
                key: "key-a".to_string(),
                email: None,
            },
        );
        let p2 = create_discovered_profile(
            "$OPENAI_API_KEY",
            CredentialSource::Environment,
            AuthProvider::OpenAI,
            Credential::ApiKey {
                key: "key-b".to_string(),
                email: None,
            },
        );
        let p3 = create_discovered_profile(
            "$ANTHROPIC_API_KEY",
            CredentialSource::Environment,
            AuthProvider::Anthropic,
            Credential::ApiKey {
                key: "key-c".to_string(),
                email: None,
            },
        );

        let id1 = AuthProfileManager::discovered_profile_id(&p1);
        let id2 = AuthProfileManager::discovered_profile_id(&p2);
        let id3 = AuthProfileManager::discovered_profile_id(&p3);

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }
}
