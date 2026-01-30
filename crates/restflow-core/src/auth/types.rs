//! Core types for authentication profile management
//!
//! Defines credentials, profiles, and their metadata.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// Credential type representing different authentication methods
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
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

    /// Get a display-safe version of the credential (masked)
    pub fn masked(&self) -> String {
        let value = self.get_auth_value();
        if value.len() <= 8 {
            return "*".repeat(value.len());
        }
        format!("{}...{}", &value[..4], &value[value.len() - 4..])
    }
}

/// Source of the credential discovery
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../web/src/types/generated/")]
#[serde(rename_all = "snake_case")]
pub enum CredentialSource {
    /// Discovered from Claude Code credentials file
    ClaudeCode,
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
            CredentialSource::Keychain => write!(f, "Keychain"),
            CredentialSource::Environment => write!(f, "Environment"),
            CredentialSource::Manual => write!(f, "Manual"),
        }
    }
}

/// Provider type for the credential
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../web/src/types/generated/")]
#[serde(rename_all = "snake_case")]
pub enum AuthProvider {
    /// Anthropic Claude API
    Anthropic,
    /// OpenAI API
    OpenAI,
    /// Google Gemini API
    Google,
    /// Other/Custom provider
    Other,
}

impl std::fmt::Display for AuthProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthProvider::Anthropic => write!(f, "Anthropic"),
            AuthProvider::OpenAI => write!(f, "OpenAI"),
            AuthProvider::Google => write!(f, "Google"),
            AuthProvider::Other => write!(f, "Other"),
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
    /// The credential data
    pub credential: Credential,
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
    /// Create a new auth profile
    pub fn new(
        name: impl Into<String>,
        credential: Credential,
        source: CredentialSource,
        provider: AuthProvider,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
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

    /// Get the API key/token for use
    pub fn get_api_key(&self) -> &str {
        self.credential.get_auth_value()
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
        let profile = AuthProfile::new(
            "Test Profile",
            Credential::ApiKey {
                key: "test-key".to_string(),
                email: None,
            },
            CredentialSource::Manual,
            AuthProvider::Anthropic,
        );

        assert!(!profile.id.is_empty());
        assert_eq!(profile.name, "Test Profile");
        assert!(profile.enabled);
        assert_eq!(profile.health, ProfileHealth::Unknown);
        assert!(profile.is_available());
    }

    #[test]
    fn test_auth_profile_not_available_disabled() {
        let mut profile = AuthProfile::new(
            "Test",
            Credential::ApiKey {
                key: "key".to_string(),
                email: None,
            },
            CredentialSource::Manual,
            AuthProvider::Anthropic,
        );
        profile.enabled = false;

        assert!(!profile.is_available());
    }

    #[test]
    fn test_auth_profile_not_available_cooldown() {
        let mut profile = AuthProfile::new(
            "Test",
            Credential::ApiKey {
                key: "key".to_string(),
                email: None,
            },
            CredentialSource::Manual,
            AuthProvider::Anthropic,
        );
        profile.cooldown_until = Some(Utc::now() + chrono::Duration::hours(1));

        assert!(!profile.is_available());
    }

    #[test]
    fn test_auth_profile_mark_success() {
        let mut profile = AuthProfile::new(
            "Test",
            Credential::ApiKey {
                key: "key".to_string(),
                email: None,
            },
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
        let mut profile = AuthProfile::new(
            "Test",
            Credential::ApiKey {
                key: "key".to_string(),
                email: None,
            },
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
        let mut profile = AuthProfile::new(
            "Test",
            Credential::ApiKey {
                key: "key".to_string(),
                email: None,
            },
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
        assert_eq!(format!("{}", CredentialSource::Keychain), "Keychain");
        assert_eq!(format!("{}", CredentialSource::Environment), "Environment");
        assert_eq!(format!("{}", CredentialSource::Manual), "Manual");
    }

    #[test]
    fn test_auth_provider_display() {
        assert_eq!(format!("{}", AuthProvider::Anthropic), "Anthropic");
        assert_eq!(format!("{}", AuthProvider::OpenAI), "OpenAI");
        assert_eq!(format!("{}", AuthProvider::Google), "Google");
        assert_eq!(format!("{}", AuthProvider::Other), "Other");
    }

    #[test]
    fn test_profile_health_default() {
        assert_eq!(ProfileHealth::default(), ProfileHealth::Unknown);
    }
}
