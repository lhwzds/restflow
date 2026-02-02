//! Core types for authentication profile management
//!
//! Defines credentials, profiles, and their metadata.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::Provider;
use super::resolver::CredentialResolver;

/// Secret key naming convention for auth profiles.
pub fn secret_key(profile_id: &str, field: &str) -> String {
    format!("auth:{}:{}", profile_id, field)
}

/// Credential type representing different authentication methods
///
/// Note: Debug is manually implemented to prevent logging sensitive values.
#[derive(Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../web/src/types/generated/")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Credential {
    /// API key authentication (e.g., ANTHROPIC_API_KEY)
    ApiKey {
        /// The API key value
        key: String,
        /// Associated email/account (optional)
        #[serde(skip_serializing_if = "Option::is_none")]
        email: Option<String>,
    },
    /// Session token authentication
    Token {
        /// The session token
        token: String,
        /// Token expiration time
        #[serde(skip_serializing_if = "Option::is_none")]
        expires_at: Option<DateTime<Utc>>,
        /// Associated email/account
        #[serde(skip_serializing_if = "Option::is_none")]
        email: Option<String>,
    },
    /// OAuth authentication with refresh capability
    OAuth {
        /// Access token for API calls
        access_token: String,
        /// Refresh token for obtaining new access tokens
        #[serde(skip_serializing_if = "Option::is_none")]
        refresh_token: Option<String>,
        /// Access token expiration time
        #[serde(skip_serializing_if = "Option::is_none")]
        expires_at: Option<DateTime<Utc>>,
        /// Associated email/account
        #[serde(skip_serializing_if = "Option::is_none")]
        email: Option<String>,
    },
}

// Manual Debug implementation to prevent logging sensitive credential values
impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Credential::ApiKey { email, .. } => f
                .debug_struct("ApiKey")
                .field("key", &"[REDACTED]")
                .field("email", email)
                .finish(),
            Credential::Token {
                expires_at, email, ..
            } => f
                .debug_struct("Token")
                .field("token", &"[REDACTED]")
                .field("expires_at", expires_at)
                .field("email", email)
                .finish(),
            Credential::OAuth {
                refresh_token,
                expires_at,
                email,
                ..
            } => f
                .debug_struct("OAuth")
                .field("access_token", &"[REDACTED]")
                .field(
                    "refresh_token",
                    &refresh_token.as_ref().map(|_| "[REDACTED]"),
                )
                .field("expires_at", expires_at)
                .field("email", email)
                .finish(),
        }
    }
}

impl Credential {
    /// Get the primary authentication value (key/token)
    pub fn get_auth_value(&self) -> &str {
        match self {
            Credential::ApiKey { key, .. } => key,
            Credential::Token { token, .. } => token,
            Credential::OAuth { access_token, .. } => access_token,
        }
    }

    /// Get the associated email if available
    pub fn get_email(&self) -> Option<&str> {
        match self {
            Credential::ApiKey { email, .. } => email.as_deref(),
            Credential::Token { email, .. } => email.as_deref(),
            Credential::OAuth { email, .. } => email.as_deref(),
        }
    }

    /// Check if the credential has expired
    pub fn is_expired(&self) -> bool {
        match self {
            Credential::ApiKey { .. } => false, // API keys don't expire
            Credential::Token { expires_at, .. } | Credential::OAuth { expires_at, .. } => {
                expires_at.map(|exp| exp < Utc::now()).unwrap_or(false)
            }
        }
    }

    /// Check if the credential can be refreshed
    pub fn can_refresh(&self) -> bool {
        matches!(
            self,
            Credential::OAuth {
                refresh_token: Some(_),
                ..
            }
        )
    }

    /// Get the refresh token if this is an OAuth credential with one.
    pub fn refresh_token(&self) -> Option<&str> {
        match self {
            Credential::OAuth {
                refresh_token: Some(token),
                ..
            } => Some(token.as_str()),
            _ => None,
        }
    }

    /// Get a display-safe version of the credential (masked)
    pub fn masked(&self) -> String {
        let value = self.get_auth_value();
        if value.len() <= 8 {
            return "*".repeat(value.len());
        }
        format!("{}...{}", &value[..4], &value[value.len() - 4..])
    }
}

/// Secure credential storing secret references instead of plaintext values.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../web/src/types/generated/")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SecureCredential {
    /// API key stored in SecretStorage.
    ApiKey {
        /// Reference to secret in SecretStorage
        secret_ref: String,
        /// Associated email/account (optional)
        #[serde(skip_serializing_if = "Option::is_none")]
        email: Option<String>,
    },
    /// Session token stored in SecretStorage.
    Token {
        /// Reference to secret in SecretStorage
        secret_ref: String,
        /// Token expiration time
        #[serde(skip_serializing_if = "Option::is_none")]
        expires_at: Option<DateTime<Utc>>,
        /// Associated email/account
        #[serde(skip_serializing_if = "Option::is_none")]
        email: Option<String>,
    },
    /// OAuth with references to access/refresh tokens.
    OAuth {
        /// Reference to access token secret
        access_token_ref: String,
        /// Reference to refresh token secret (optional)
        #[serde(skip_serializing_if = "Option::is_none")]
        refresh_token_ref: Option<String>,
        /// Access token expiration time
        #[serde(skip_serializing_if = "Option::is_none")]
        expires_at: Option<DateTime<Utc>>,
        /// Associated email/account
        #[serde(skip_serializing_if = "Option::is_none")]
        email: Option<String>,
    },
}

impl SecureCredential {
    /// Get all secret references for this credential.
    pub fn secret_refs(&self) -> Vec<&str> {
        match self {
            SecureCredential::ApiKey { secret_ref, .. } => vec![secret_ref],
            SecureCredential::Token { secret_ref, .. } => vec![secret_ref],
            SecureCredential::OAuth {
                access_token_ref,
                refresh_token_ref,
                ..
            } => {
                let mut refs = vec![access_token_ref.as_str()];
                if let Some(refresh_ref) = refresh_token_ref {
                    refs.push(refresh_ref.as_str());
                }
                refs
            }
        }
    }

    /// Get the primary secret reference (for API key retrieval).
    pub fn primary_secret_ref(&self) -> &str {
        match self {
            SecureCredential::ApiKey { secret_ref, .. } => secret_ref,
            SecureCredential::Token { secret_ref, .. } => secret_ref,
            SecureCredential::OAuth { access_token_ref, .. } => access_token_ref,
        }
    }

    /// Get the refresh token reference if available.
    pub fn refresh_token_ref(&self) -> Option<&str> {
        match self {
            SecureCredential::OAuth {
                refresh_token_ref: Some(refresh_ref),
                ..
            } => Some(refresh_ref.as_str()),
            _ => None,
        }
    }

    /// Get the associated email if available.
    pub fn get_email(&self) -> Option<&str> {
        match self {
            SecureCredential::ApiKey { email, .. } => email.as_deref(),
            SecureCredential::Token { email, .. } => email.as_deref(),
            SecureCredential::OAuth { email, .. } => email.as_deref(),
        }
    }

    /// Check if the credential has expired.
    pub fn is_expired(&self) -> bool {
        match self {
            SecureCredential::ApiKey { .. } => false,
            SecureCredential::Token { expires_at, .. }
            | SecureCredential::OAuth { expires_at, .. } => {
                expires_at.map(|exp| exp < Utc::now()).unwrap_or(false)
            }
        }
    }

    /// Check if the credential can be refreshed.
    pub fn can_refresh(&self) -> bool {
        matches!(
            self,
            SecureCredential::OAuth {
                refresh_token_ref: Some(_),
                ..
            }
        )
    }

    /// Update the OAuth access token reference metadata.
    pub fn update_oauth_metadata(
        &mut self,
        refresh_token_ref: Option<String>,
        expires_at: Option<DateTime<Utc>>,
    ) {
        if let SecureCredential::OAuth {
            refresh_token_ref: current_refresh,
            expires_at: current_expiry,
            ..
        } = self
        {
            if refresh_token_ref.is_some() {
                *current_refresh = refresh_token_ref;
            }
            *current_expiry = expires_at;
        }
    }
}

/// Source of the credential discovery
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../web/src/types/generated/")]
#[serde(rename_all = "snake_case")]
pub enum CredentialSource {
    /// Discovered from Claude Code credentials file
    ClaudeCode,
    /// Discovered from Codex CLI credentials file
    CodexCli,
    /// Retrieved from macOS Keychain
    Keychain,
    /// Read from environment variable
    Environment,
    /// Manually configured by user
    Manual,
}

impl std::fmt::Display for CredentialSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CredentialSource::ClaudeCode => write!(f, "Claude Code"),
            CredentialSource::CodexCli => write!(f, "Codex CLI"),
            CredentialSource::Keychain => write!(f, "Keychain"),
            CredentialSource::Environment => write!(f, "Environment"),
            CredentialSource::Manual => write!(f, "Manual"),
        }
    }
}

/// Provider type for the credential
///
/// Distinguishes between direct API access and Claude Code CLI usage:
/// - `Anthropic`: Direct API calls using `sk-ant-api03-...` keys
/// - `ClaudeCode`: Claude Code CLI with OAuth tokens (`sk-ant-oat01-...`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../web/src/types/generated/")]
#[serde(rename_all = "snake_case")]
pub enum AuthProvider {
    /// Anthropic Claude API - direct API calls with API key (`sk-ant-api03-...`)
    Anthropic,
    /// Claude Code CLI - OAuth tokens (`sk-ant-oat01-...`)
    ///
    /// Two sources of ClaudeCode tokens:
    /// - `claude login`: Short-lived OAuth with refresh token (auto-discovered from ~/.claude/.credentials.json)
    /// - `claude setup-token`: Long-lived OAuth token (1 year, manually added, no refresh needed)
    ///
    /// Both use the same token format but differ in expiration and refresh capability.
    ClaudeCode,
    /// OpenAI API
    #[serde(rename = "openai")]
    #[ts(rename = "openai")]
    OpenAI,
    /// OpenAI Codex CLI
    #[serde(rename = "openai_codex")]
    #[ts(rename = "openai_codex")]
    OpenAICodex,
    /// Google Gemini API
    Google,
    /// Other/Custom provider
    Other,
}

impl std::fmt::Display for AuthProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthProvider::Anthropic => write!(f, "Anthropic"),
            AuthProvider::ClaudeCode => write!(f, "ClaudeCode"),
            AuthProvider::OpenAI => write!(f, "OpenAI"),
            AuthProvider::OpenAICodex => write!(f, "OpenAICodex"),
            AuthProvider::Google => write!(f, "Google"),
            AuthProvider::Other => write!(f, "Other"),
        }
    }
}

impl AuthProvider {
    /// Return compatible auth providers for a model provider, ordered by preference.
    pub fn compatible_with(provider: Provider) -> Vec<AuthProvider> {
        match provider {
            Provider::Anthropic => vec![AuthProvider::ClaudeCode, AuthProvider::Anthropic],
            Provider::OpenAI => vec![AuthProvider::OpenAICodex, AuthProvider::OpenAI],
            Provider::DeepSeek => vec![AuthProvider::Other],
        }
    }
}

/// Health status of an auth profile
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../web/src/types/generated/")]
#[serde(rename_all = "snake_case")]
pub enum ProfileHealth {
    /// Profile is healthy and available
    Healthy,
    /// Profile has failed recently, in cooldown
    Cooldown,
    /// Profile is permanently disabled
    Disabled,
    /// Profile status is unknown (not yet tested)
    #[default]
    Unknown,
}

/// Authentication profile combining credential with metadata
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../web/src/types/generated/")]
pub struct AuthProfile {
    /// Unique profile identifier
    pub id: String,
    /// Display name for the profile
    pub name: String,
    /// The credential data (secure references)
    pub credential: SecureCredential,
    /// Where the credential was discovered from
    pub source: CredentialSource,
    /// Which provider this credential is for
    pub provider: AuthProvider,
    /// Current health status
    #[serde(default)]
    pub health: ProfileHealth,
    /// Whether this profile is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Priority for selection (lower = higher priority)
    #[serde(default)]
    pub priority: i32,
    /// When the profile was created
    pub created_at: DateTime<Utc>,
    /// When the profile was last used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
    /// When the profile last failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_failed_at: Option<DateTime<Utc>>,
    /// Number of consecutive failures
    #[serde(default)]
    pub failure_count: u32,
    /// Cooldown end time if in cooldown
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cooldown_until: Option<DateTime<Utc>>,
}

fn default_true() -> bool {
    true
}

impl AuthProfile {
    /// Create a new auth profile with a generated id.
    pub fn new(
        name: impl Into<String>,
        credential: SecureCredential,
        source: CredentialSource,
        provider: AuthProvider,
    ) -> Self {
        Self::new_with_id(Uuid::new_v4().to_string(), name, credential, source, provider)
    }

    /// Create a new auth profile with a specific id.
    pub fn new_with_id(
        id: String,
        name: impl Into<String>,
        credential: SecureCredential,
        source: CredentialSource,
        provider: AuthProvider,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            credential,
            source,
            provider,
            health: ProfileHealth::Unknown,
            enabled: true,
            priority: 0,
            created_at: Utc::now(),
            last_used_at: None,
            last_failed_at: None,
            failure_count: 0,
            cooldown_until: None,
        }
    }

    /// Check if the profile is available for use
    pub fn is_available(&self) -> bool {
        if !self.enabled {
            return false;
        }
        if self.credential.is_expired() {
            return false;
        }
        if let Some(cooldown_until) = self.cooldown_until
            && cooldown_until > Utc::now()
        {
            return false;
        }
        true
    }

    /// Mark the profile as successfully used
    pub fn mark_success(&mut self) {
        self.last_used_at = Some(Utc::now());
        self.health = ProfileHealth::Healthy;
        self.failure_count = 0;
        self.cooldown_until = None;
    }

    /// Mark the profile as failed
    pub fn mark_failure(&mut self, cooldown_seconds: u64) {
        self.last_failed_at = Some(Utc::now());
        self.failure_count += 1;

        // Calculate cooldown duration with exponential backoff
        let base_cooldown = cooldown_seconds as i64;
        let backoff_factor = 2_i64.pow((self.failure_count - 1).min(5));
        let cooldown_duration = chrono::Duration::seconds(base_cooldown * backoff_factor);

        self.cooldown_until = Some(Utc::now() + cooldown_duration);
        self.health = ProfileHealth::Cooldown;
    }

    /// Disable the profile permanently
    pub fn disable(&mut self, reason: &str) {
        self.enabled = false;
        self.health = ProfileHealth::Disabled;
        tracing::warn!(profile_id = %self.id, name = %self.name, reason, "Profile disabled");
    }

    /// Resolve the API key/token for use.
    pub fn get_api_key(&self, resolver: &CredentialResolver) -> anyhow::Result<String> {
        resolver.resolve_auth_value(&self.credential)
    }

    pub fn is_oauth(&self) -> bool {
        matches!(self.credential, SecureCredential::OAuth { .. })
    }
}

/// Summary of discovered profiles
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../web/src/types/generated/")]
pub struct DiscoverySummary {
    /// Total profiles discovered
    pub total: usize,
    /// Profiles by source
    pub by_source: std::collections::HashMap<String, usize>,
    /// Profiles by provider
    pub by_provider: std::collections::HashMap<String, usize>,
    /// Profiles that are available for use
    pub available: usize,
    /// Discovery errors encountered
    pub errors: Vec<String>,
}

/// Result of profile selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSelection {
    /// Selected profile
    pub profile: AuthProfile,
    /// Reason for selection
    pub reason: String,
    /// Alternative profiles available
    pub alternatives: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{SecretStorage, SecretStorageConfig};
    use redb::Database;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn create_test_resolver() -> (Arc<SecretStorage>, CredentialResolver, TempDir) {
        let dir = TempDir::new().unwrap();
        let db = Arc::new(Database::create(dir.path().join("test.db")).unwrap());
        let secrets = Arc::new(
            SecretStorage::with_config(
                db,
                SecretStorageConfig {
                    allow_insecure_fallback: true,
                    ..Default::default()
                },
            )
            .unwrap(),
        );
        let resolver = CredentialResolver::new(secrets.clone());
        (secrets, resolver, dir)
    }

    #[test]
    fn test_compatible_with_provider() {
        assert_eq!(
            AuthProvider::compatible_with(Provider::OpenAI),
            vec![AuthProvider::OpenAICodex, AuthProvider::OpenAI]
        );
        assert_eq!(
            AuthProvider::compatible_with(Provider::Anthropic),
            vec![AuthProvider::ClaudeCode, AuthProvider::Anthropic]
        );
    }

    #[test]
    fn test_credential_api_key() {
        let cred = Credential::ApiKey {
            key: "sk-ant-api03-test".to_string(),
            email: Some("test@example.com".to_string()),
        };

        assert_eq!(cred.get_auth_value(), "sk-ant-api03-test");
        assert_eq!(cred.get_email(), Some("test@example.com"));
        assert!(!cred.is_expired());
        assert!(!cred.can_refresh());
    }

    #[test]
    fn test_credential_token_expired() {
        let cred = Credential::Token {
            token: "token123".to_string(),
            expires_at: Some(Utc::now() - chrono::Duration::hours(1)),
            email: None,
        };

        assert!(cred.is_expired());
    }

    #[test]
    fn test_credential_token_not_expired() {
        let cred = Credential::Token {
            token: "token123".to_string(),
            expires_at: Some(Utc::now() + chrono::Duration::hours(1)),
            email: None,
        };

        assert!(!cred.is_expired());
    }

    #[test]
    fn test_credential_oauth_can_refresh() {
        let cred = Credential::OAuth {
            access_token: "access123".to_string(),
            refresh_token: Some("refresh123".to_string()),
            expires_at: None,
            email: None,
        };

        assert!(cred.can_refresh());
    }

    #[test]
    fn test_credential_oauth_cannot_refresh() {
        let cred = Credential::OAuth {
            access_token: "access123".to_string(),
            refresh_token: None,
            expires_at: None,
            email: None,
        };

        assert!(!cred.can_refresh());
    }

    #[test]
    fn test_credential_masked() {
        let cred = Credential::ApiKey {
            key: "sk-ant-api03-abcdefgh1234".to_string(),
            email: None,
        };

        let masked = cred.masked();
        assert!(masked.starts_with("sk-a"));
        assert!(masked.ends_with("1234"));
        assert!(masked.contains("..."));
    }

    #[test]
    fn test_credential_masked_short() {
        let cred = Credential::ApiKey {
            key: "short".to_string(),
            email: None,
        };

        let masked = cred.masked();
        assert_eq!(masked, "*****");
    }

    #[test]
    fn test_auth_profile_new() {
        let (_, resolver, _dir) = create_test_resolver();
        let credential = SecureCredential::ApiKey {
            secret_ref: "auth:test:api_key".to_string(),
            email: None,
        };
        let profile = AuthProfile::new(
            "Test Profile",
            credential,
            CredentialSource::Manual,
            AuthProvider::Anthropic,
        );

        assert!(!profile.id.is_empty());
        assert_eq!(profile.name, "Test Profile");
        assert!(profile.enabled);
        assert_eq!(profile.health, ProfileHealth::Unknown);
        assert!(profile.is_available());

        let _ = resolver;
    }

    #[test]
    fn test_auth_profile_not_available_disabled() {
        let credential = SecureCredential::ApiKey {
            secret_ref: "auth:test:api_key".to_string(),
            email: None,
        };
        let mut profile = AuthProfile::new(
            "Test",
            credential,
            CredentialSource::Manual,
            AuthProvider::Anthropic,
        );
        profile.enabled = false;

        assert!(!profile.is_available());
    }

    #[test]
    fn test_auth_profile_not_available_cooldown() {
        let credential = SecureCredential::ApiKey {
            secret_ref: "auth:test:api_key".to_string(),
            email: None,
        };
        let mut profile = AuthProfile::new(
            "Test",
            credential,
            CredentialSource::Manual,
            AuthProvider::Anthropic,
        );
        profile.cooldown_until = Some(Utc::now() + chrono::Duration::hours(1));

        assert!(!profile.is_available());
    }

    #[test]
    fn test_auth_profile_mark_success() {
        let credential = SecureCredential::ApiKey {
            secret_ref: "auth:test:api_key".to_string(),
            email: None,
        };
        let mut profile = AuthProfile::new(
            "Test",
            credential,
            CredentialSource::Manual,
            AuthProvider::Anthropic,
        );
        profile.failure_count = 3;
        profile.health = ProfileHealth::Cooldown;

        profile.mark_success();

        assert_eq!(profile.failure_count, 0);
        assert_eq!(profile.health, ProfileHealth::Healthy);
        assert!(profile.cooldown_until.is_none());
        assert!(profile.last_used_at.is_some());
    }

    #[test]
    fn test_auth_profile_mark_failure_exponential_backoff() {
        let credential = SecureCredential::ApiKey {
            secret_ref: "auth:test:api_key".to_string(),
            email: None,
        };
        let mut profile = AuthProfile::new(
            "Test",
            credential,
            CredentialSource::Manual,
            AuthProvider::Anthropic,
        );

        // First failure: 60 seconds cooldown
        profile.mark_failure(60);
        assert_eq!(profile.failure_count, 1);
        assert!(profile.cooldown_until.is_some());

        // Second failure: 120 seconds cooldown
        profile.mark_failure(60);
        assert_eq!(profile.failure_count, 2);

        // Third failure: 240 seconds cooldown
        profile.mark_failure(60);
        assert_eq!(profile.failure_count, 3);

        assert_eq!(profile.health, ProfileHealth::Cooldown);
    }

    #[test]
    fn test_auth_profile_disable() {
        let credential = SecureCredential::ApiKey {
            secret_ref: "auth:test:api_key".to_string(),
            email: None,
        };
        let mut profile = AuthProfile::new(
            "Test",
            credential,
            CredentialSource::Manual,
            AuthProvider::Anthropic,
        );

        profile.disable("Invalid credentials");

        assert!(!profile.enabled);
        assert_eq!(profile.health, ProfileHealth::Disabled);
    }

    #[test]
    fn test_credential_source_display() {
        assert_eq!(format!("{}", CredentialSource::ClaudeCode), "Claude Code");
        assert_eq!(format!("{}", CredentialSource::CodexCli), "Codex CLI");
        assert_eq!(format!("{}", CredentialSource::Keychain), "Keychain");
        assert_eq!(format!("{}", CredentialSource::Environment), "Environment");
        assert_eq!(format!("{}", CredentialSource::Manual), "Manual");
    }

    #[test]
    fn test_auth_provider_display() {
        assert_eq!(format!("{}", AuthProvider::Anthropic), "Anthropic");
        assert_eq!(format!("{}", AuthProvider::OpenAI), "OpenAI");
        assert_eq!(format!("{}", AuthProvider::OpenAICodex), "OpenAICodex");
        assert_eq!(format!("{}", AuthProvider::Google), "Google");
        assert_eq!(format!("{}", AuthProvider::Other), "Other");
    }

    #[test]
    fn test_profile_health_default() {
        assert_eq!(ProfileHealth::default(), ProfileHealth::Unknown);
    }

    #[test]
    fn test_credential_debug_masks_sensitive_values() {
        // Test ApiKey Debug implementation
        let api_key = Credential::ApiKey {
            key: "sk-ant-api03-secret-key-12345".to_string(),
            email: Some("test@example.com".to_string()),
        };
        let debug_str = format!("{:?}", api_key);
        assert!(
            !debug_str.contains("sk-ant-api03-secret-key-12345"),
            "API key should be redacted in Debug output"
        );
        assert!(
            debug_str.contains("[REDACTED]"),
            "Debug output should contain [REDACTED]"
        );

        // Test Token Debug implementation
        let token = Credential::Token {
            token: "super-secret-token".to_string(),
            expires_at: None,
            email: None,
        };
        let debug_str = format!("{:?}", token);
        assert!(
            !debug_str.contains("super-secret-token"),
            "Token should be redacted in Debug output"
        );

        // Test OAuth Debug implementation
        let oauth = Credential::OAuth {
            access_token: "access-token-secret".to_string(),
            refresh_token: Some("refresh-token-secret".to_string()),
            expires_at: None,
            email: Some("user@example.com".to_string()),
        };
        let debug_str = format!("{:?}", oauth);
        assert!(
            !debug_str.contains("access-token-secret"),
            "Access token should be redacted"
        );
        assert!(
            !debug_str.contains("refresh-token-secret"),
            "Refresh token should be redacted"
        );
        assert!(
            debug_str.contains("user@example.com"),
            "Email should still be visible"
        );
    }
}
