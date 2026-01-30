//! Credential Discovery Framework
//!
//! Provides traits and implementations for discovering credentials from various sources.

use super::types::{AuthProfile, AuthProvider, Credential, CredentialSource, DiscoverySummary};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, info};
#[cfg(feature = "keychain")]
use tracing::warn;

/// Result of a credential discovery attempt
#[derive(Debug, Clone)]
pub struct DiscoveryResult {
    /// Discovered profiles
    pub profiles: Vec<AuthProfile>,
    /// Errors encountered during discovery
    pub errors: Vec<String>,
    /// Source that was checked
    pub source: CredentialSource,
}

impl DiscoveryResult {
    /// Create a successful result with profiles
    pub fn success(profiles: Vec<AuthProfile>, source: CredentialSource) -> Self {
        Self {
            profiles,
            errors: Vec::new(),
            source,
        }
    }

    /// Create an empty result (no profiles found, no errors)
    pub fn empty(source: CredentialSource) -> Self {
        Self {
            profiles: Vec::new(),
            errors: Vec::new(),
            source,
        }
    }

    /// Create an error result
    pub fn error(error: impl Into<String>, source: CredentialSource) -> Self {
        Self {
            profiles: Vec::new(),
            errors: vec![error.into()],
            source,
        }
    }
}

/// Trait for credential discoverers
///
/// Implementations should be able to discover credentials from their specific source
/// (e.g., file system, keychain, environment variables).
#[async_trait]
pub trait CredentialDiscoverer: Send + Sync {
    /// Get the source type for this discoverer
    fn source(&self) -> CredentialSource;

    /// Get a human-readable name for this discoverer
    fn name(&self) -> &str;

    /// Check if this discoverer is available on the current system
    async fn is_available(&self) -> bool;

    /// Discover credentials from this source
    async fn discover(&self) -> DiscoveryResult;

    /// Optionally validate a discovered credential
    async fn validate(&self, _credential: &Credential) -> Result<bool> {
        // Default implementation: assume valid
        Ok(true)
    }
}

/// Claude Code credentials discoverer
///
/// Reads credentials from ~/.claude/.credentials.json
pub struct ClaudeCodeDiscoverer {
    /// Path to the credentials file
    credentials_path: PathBuf,
}

impl ClaudeCodeDiscoverer {
    /// Create a new Claude Code discoverer with default path
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self {
            credentials_path: home.join(".claude").join(".credentials.json"),
        }
    }

    /// Create a discoverer with a custom path (for testing)
    pub fn with_path(path: PathBuf) -> Self {
        Self {
            credentials_path: path,
        }
    }
}

impl Default for ClaudeCodeDiscoverer {
    fn default() -> Self {
        Self::new()
    }
}

/// Claude Code credentials file structure
#[derive(Debug, Deserialize)]
struct ClaudeCredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<ClaudeOAuthCredential>,
}

#[derive(Debug, Deserialize)]
struct ClaudeOAuthCredential {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "refreshToken")]
    refresh_token: Option<String>,
    #[serde(rename = "expiresAt")]
    expires_at: Option<i64>,
    #[serde(rename = "email")]
    email: Option<String>,
}

#[async_trait]
impl CredentialDiscoverer for ClaudeCodeDiscoverer {
    fn source(&self) -> CredentialSource {
        CredentialSource::ClaudeCode
    }

    fn name(&self) -> &str {
        "Claude Code"
    }

    async fn is_available(&self) -> bool {
        self.credentials_path.exists()
    }

    async fn discover(&self) -> DiscoveryResult {
        if !self.credentials_path.exists() {
            debug!(
                path = ?self.credentials_path,
                "Claude Code credentials file not found"
            );
            return DiscoveryResult::empty(self.source());
        }

        let content = match tokio::fs::read_to_string(&self.credentials_path).await {
            Ok(c) => c,
            Err(e) => {
                return DiscoveryResult::error(
                    format!("Failed to read credentials file: {}", e),
                    self.source(),
                );
            }
        };

        let creds: ClaudeCredentialsFile = match serde_json::from_str(&content) {
            Ok(c) => c,
            Err(e) => {
                return DiscoveryResult::error(
                    format!("Failed to parse credentials file: {}", e),
                    self.source(),
                );
            }
        };

        let mut profiles = Vec::new();

        if let Some(oauth) = creds.claude_ai_oauth {
            let expires_at = oauth.expires_at.map(|ts| {
                DateTime::from_timestamp_millis(ts).unwrap_or_else(Utc::now)
            });

            let credential = Credential::OAuth {
                access_token: oauth.access_token,
                refresh_token: oauth.refresh_token,
                expires_at,
                email: oauth.email.clone(),
            };

            let name = oauth
                .email
                .as_ref()
                .map(|e| format!("Claude Code ({})", e))
                .unwrap_or_else(|| "Claude Code".to_string());

            let profile = AuthProfile::new(name, credential, self.source(), AuthProvider::Anthropic);

            info!(
                profile_id = %profile.id,
                email = ?oauth.email,
                "Discovered Claude Code credential"
            );

            profiles.push(profile);
        }

        DiscoveryResult::success(profiles, self.source())
    }
}

/// Environment variable discoverer
///
/// Reads API keys from common environment variables
pub struct EnvVarDiscoverer {
    /// Environment variables to check with their providers
    env_vars: Vec<(String, AuthProvider)>,
}

impl EnvVarDiscoverer {
    /// Create a new environment variable discoverer with default variables
    pub fn new() -> Self {
        Self {
            env_vars: vec![
                ("ANTHROPIC_API_KEY".to_string(), AuthProvider::Anthropic),
                ("OPENAI_API_KEY".to_string(), AuthProvider::OpenAI),
                ("GOOGLE_API_KEY".to_string(), AuthProvider::Google),
                ("GEMINI_API_KEY".to_string(), AuthProvider::Google),
            ],
        }
    }

    /// Add a custom environment variable to check
    pub fn add_env_var(&mut self, name: impl Into<String>, provider: AuthProvider) {
        self.env_vars.push((name.into(), provider));
    }
}

impl Default for EnvVarDiscoverer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CredentialDiscoverer for EnvVarDiscoverer {
    fn source(&self) -> CredentialSource {
        CredentialSource::Environment
    }

    fn name(&self) -> &str {
        "Environment Variables"
    }

    async fn is_available(&self) -> bool {
        // Environment is always available
        true
    }

    async fn discover(&self) -> DiscoveryResult {
        let mut profiles = Vec::new();

        for (env_var, provider) in &self.env_vars {
            if let Ok(value) = std::env::var(env_var) {
                if !value.is_empty() {
                    let credential = Credential::ApiKey {
                        key: value,
                        email: None,
                    };

                    let profile = AuthProfile::new(
                        format!("${}", env_var),
                        credential,
                        self.source(),
                        *provider,
                    );

                    info!(
                        profile_id = %profile.id,
                        env_var = env_var,
                        provider = ?provider,
                        "Discovered credential from environment"
                    );

                    profiles.push(profile);
                }
            }
        }

        DiscoveryResult::success(profiles, self.source())
    }
}

/// Keychain discoverer (macOS/Linux/Windows)
///
/// Uses the keyring crate to access system keychain
#[cfg(feature = "keychain")]
pub struct KeychainDiscoverer {
    /// Service names to check
    services: Vec<(String, String, AuthProvider)>, // (service, account, provider)
}

#[cfg(feature = "keychain")]
impl KeychainDiscoverer {
    /// Create a new keychain discoverer with default services
    pub fn new() -> Self {
        Self {
            services: vec![
                (
                    "anthropic-api".to_string(),
                    "api-key".to_string(),
                    AuthProvider::Anthropic,
                ),
                (
                    "openai-api".to_string(),
                    "api-key".to_string(),
                    AuthProvider::OpenAI,
                ),
                (
                    "restflow".to_string(),
                    "anthropic".to_string(),
                    AuthProvider::Anthropic,
                ),
                (
                    "restflow".to_string(),
                    "openai".to_string(),
                    AuthProvider::OpenAI,
                ),
            ],
        }
    }

    /// Add a custom service to check
    pub fn add_service(
        &mut self,
        service: impl Into<String>,
        account: impl Into<String>,
        provider: AuthProvider,
    ) {
        self.services
            .push((service.into(), account.into(), provider));
    }
}

#[cfg(feature = "keychain")]
impl Default for KeychainDiscoverer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "keychain")]
#[async_trait]
impl CredentialDiscoverer for KeychainDiscoverer {
    fn source(&self) -> CredentialSource {
        CredentialSource::Keychain
    }

    fn name(&self) -> &str {
        "System Keychain"
    }

    async fn is_available(&self) -> bool {
        // Check if keychain is accessible
        let entry = keyring::Entry::new("restflow-test", "availability-check");
        entry.is_ok()
    }

    async fn discover(&self) -> DiscoveryResult {
        let mut profiles = Vec::new();
        let mut errors = Vec::new();

        for (service, account, provider) in &self.services {
            match keyring::Entry::new(service, account) {
                Ok(entry) => match entry.get_password() {
                    Ok(password) if !password.is_empty() => {
                        let credential = Credential::ApiKey {
                            key: password,
                            email: None,
                        };

                        let profile = AuthProfile::new(
                            format!("Keychain: {}/{}", service, account),
                            credential,
                            self.source(),
                            *provider,
                        );

                        info!(
                            profile_id = %profile.id,
                            service = service,
                            account = account,
                            "Discovered credential from keychain"
                        );

                        profiles.push(profile);
                    }
                    Ok(_) => {
                        debug!(service = service, account = account, "Empty keychain entry");
                    }
                    Err(keyring::Error::NoEntry) => {
                        debug!(
                            service = service,
                            account = account,
                            "No keychain entry found"
                        );
                    }
                    Err(e) => {
                        warn!(
                            service = service,
                            account = account,
                            error = %e,
                            "Failed to read keychain entry"
                        );
                        errors.push(format!(
                            "Failed to read keychain {}/{}: {}",
                            service, account, e
                        ));
                    }
                },
                Err(e) => {
                    errors.push(format!(
                        "Failed to access keychain {}/{}: {}",
                        service, account, e
                    ));
                }
            }
        }

        DiscoveryResult {
            profiles,
            errors,
            source: self.source(),
        }
    }
}

/// Composite discoverer that runs multiple discoverers
pub struct CompositeDiscoverer {
    discoverers: Vec<Box<dyn CredentialDiscoverer>>,
}

impl CompositeDiscoverer {
    /// Create a new composite discoverer
    pub fn new() -> Self {
        Self {
            discoverers: Vec::new(),
        }
    }

    /// Create with default discoverers
    pub fn with_defaults() -> Self {
        let mut composite = Self::new();
        composite.add(Box::new(ClaudeCodeDiscoverer::new()));
        composite.add(Box::new(EnvVarDiscoverer::new()));
        #[cfg(feature = "keychain")]
        composite.add(Box::new(KeychainDiscoverer::new()));
        composite
    }

    /// Add a discoverer
    pub fn add(&mut self, discoverer: Box<dyn CredentialDiscoverer>) {
        self.discoverers.push(discoverer);
    }

    /// Run all discoverers and collect results
    pub async fn discover_all(&self) -> (Vec<AuthProfile>, DiscoverySummary) {
        let mut all_profiles = Vec::new();
        let mut summary = DiscoverySummary::default();
        let mut by_source: HashMap<String, usize> = HashMap::new();
        let mut by_provider: HashMap<String, usize> = HashMap::new();

        for discoverer in &self.discoverers {
            if !discoverer.is_available().await {
                debug!(
                    name = discoverer.name(),
                    source = ?discoverer.source(),
                    "Discoverer not available, skipping"
                );
                continue;
            }

            let result = discoverer.discover().await;

            for profile in result.profiles {
                let source_key = profile.source.to_string();
                let provider_key = profile.provider.to_string();

                *by_source.entry(source_key).or_insert(0) += 1;
                *by_provider.entry(provider_key).or_insert(0) += 1;

                if profile.is_available() {
                    summary.available += 1;
                }

                all_profiles.push(profile);
            }

            summary.errors.extend(result.errors);
        }

        summary.total = all_profiles.len();
        summary.by_source = by_source;
        summary.by_provider = by_provider;

        info!(
            total = summary.total,
            available = summary.available,
            errors = summary.errors.len(),
            "Discovery complete"
        );

        (all_profiles, summary)
    }
}

impl Default for CompositeDiscoverer {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_discovery_result_success() {
        let profiles = vec![AuthProfile::new(
            "Test",
            Credential::ApiKey {
                key: "test".to_string(),
                email: None,
            },
            CredentialSource::Manual,
            AuthProvider::Anthropic,
        )];

        let result = DiscoveryResult::success(profiles.clone(), CredentialSource::Manual);

        assert_eq!(result.profiles.len(), 1);
        assert!(result.errors.is_empty());
        assert_eq!(result.source, CredentialSource::Manual);
    }

    #[test]
    fn test_discovery_result_empty() {
        let result = DiscoveryResult::empty(CredentialSource::Environment);

        assert!(result.profiles.is_empty());
        assert!(result.errors.is_empty());
        assert_eq!(result.source, CredentialSource::Environment);
    }

    #[test]
    fn test_discovery_result_error() {
        let result = DiscoveryResult::error("Test error", CredentialSource::Keychain);

        assert!(result.profiles.is_empty());
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0], "Test error");
    }

    #[tokio::test]
    async fn test_claude_code_discoverer_not_found() {
        let discoverer = ClaudeCodeDiscoverer::with_path(PathBuf::from("/nonexistent/path"));

        assert!(!discoverer.is_available().await);

        let result = discoverer.discover().await;
        assert!(result.profiles.is_empty());
        assert!(result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_claude_code_discoverer_valid_file() {
        let temp_dir = TempDir::new().unwrap();
        let creds_path = temp_dir.path().join(".credentials.json");

        let creds_content = r#"{
            "claudeAiOauth": {
                "accessToken": "test-access-token",
                "refreshToken": "test-refresh-token",
                "expiresAt": 1735689600000,
                "email": "test@example.com"
            }
        }"#;

        tokio::fs::write(&creds_path, creds_content).await.unwrap();

        let discoverer = ClaudeCodeDiscoverer::with_path(creds_path);

        assert!(discoverer.is_available().await);

        let result = discoverer.discover().await;
        assert_eq!(result.profiles.len(), 1);
        assert!(result.errors.is_empty());

        let profile = &result.profiles[0];
        assert_eq!(profile.source, CredentialSource::ClaudeCode);
        assert_eq!(profile.provider, AuthProvider::Anthropic);
        assert!(profile.name.contains("test@example.com"));
    }

    #[tokio::test]
    async fn test_claude_code_discoverer_invalid_json() {
        let temp_dir = TempDir::new().unwrap();
        let creds_path = temp_dir.path().join(".credentials.json");

        tokio::fs::write(&creds_path, "invalid json")
            .await
            .unwrap();

        let discoverer = ClaudeCodeDiscoverer::with_path(creds_path);
        let result = discoverer.discover().await;

        assert!(result.profiles.is_empty());
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].contains("Failed to parse"));
    }

    #[tokio::test]
    async fn test_env_var_discoverer() {
        // Set a test environment variable
        // SAFETY: Test only, single-threaded test environment
        unsafe {
            std::env::set_var("TEST_ANTHROPIC_API_KEY", "test-key-123");
        }

        let mut discoverer = EnvVarDiscoverer::new();
        discoverer.add_env_var("TEST_ANTHROPIC_API_KEY", AuthProvider::Anthropic);

        let result = discoverer.discover().await;

        // Find the profile we added
        let profile = result
            .profiles
            .iter()
            .find(|p| p.name == "$TEST_ANTHROPIC_API_KEY");

        assert!(profile.is_some());
        let profile = profile.unwrap();
        assert_eq!(profile.get_api_key(), "test-key-123");
        assert_eq!(profile.source, CredentialSource::Environment);

        // Clean up
        // SAFETY: Test only, single-threaded test environment
        unsafe {
            std::env::remove_var("TEST_ANTHROPIC_API_KEY");
        }
    }

    #[tokio::test]
    async fn test_env_var_discoverer_empty_value() {
        // SAFETY: Test only, single-threaded test environment
        unsafe {
            std::env::set_var("TEST_EMPTY_KEY", "");
        }

        let mut discoverer = EnvVarDiscoverer::new();
        discoverer.add_env_var("TEST_EMPTY_KEY", AuthProvider::Anthropic);

        let result = discoverer.discover().await;

        // Empty values should not be discovered
        let profile = result.profiles.iter().find(|p| p.name == "$TEST_EMPTY_KEY");
        assert!(profile.is_none());

        // SAFETY: Test only, single-threaded test environment
        unsafe {
            std::env::remove_var("TEST_EMPTY_KEY");
        }
    }

    #[tokio::test]
    async fn test_composite_discoverer_empty() {
        let discoverer = CompositeDiscoverer::new();
        let (profiles, summary) = discoverer.discover_all().await;

        assert!(profiles.is_empty());
        assert_eq!(summary.total, 0);
        assert_eq!(summary.available, 0);
    }

    #[tokio::test]
    async fn test_composite_discoverer_with_env() {
        // SAFETY: Test only, single-threaded test environment
        unsafe {
            std::env::set_var("TEST_COMPOSITE_KEY", "composite-test-key");
        }

        let mut env_discoverer = EnvVarDiscoverer::new();
        env_discoverer.add_env_var("TEST_COMPOSITE_KEY", AuthProvider::Anthropic);

        let mut composite = CompositeDiscoverer::new();
        composite.add(Box::new(env_discoverer));

        let (profiles, summary) = composite.discover_all().await;

        assert!(!profiles.is_empty());
        assert!(summary.total > 0);

        // SAFETY: Test only, single-threaded test environment
        unsafe {
            std::env::remove_var("TEST_COMPOSITE_KEY");
        }
    }

    #[test]
    fn test_claude_code_discoverer_default() {
        let discoverer = ClaudeCodeDiscoverer::default();
        assert_eq!(discoverer.source(), CredentialSource::ClaudeCode);
        assert_eq!(discoverer.name(), "Claude Code");
    }

    #[test]
    fn test_env_var_discoverer_default() {
        let discoverer = EnvVarDiscoverer::default();
        assert_eq!(discoverer.source(), CredentialSource::Environment);
        assert_eq!(discoverer.name(), "Environment Variables");
    }
}
