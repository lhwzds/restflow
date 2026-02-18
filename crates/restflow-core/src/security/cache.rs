//! Approval cache for storing and retrieving cached approval grants.
//!
//! This module provides the `ApprovalCache` for caching approval decisions
//! to reduce approval fatigue. Cached approvals can have different scopes:
//! - `ThisCall`: Only valid for the current call
//! - `Session`: Valid for the duration of the session
//! - `Persistent`: Persists across sessions

use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Scope of an approval grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalScope {
    /// Valid only for the current call
    #[default]
    ThisCall,
    /// Valid for the duration of the session
    Session,
    /// Persists across sessions (requires persistent storage)
    Persistent,
}

/// A cached approval grant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalGrant {
    /// When the grant was created (as Unix timestamp in seconds)
    pub granted_at_secs: u64,
    /// Scope of the grant
    pub scope: ApprovalScope,
    /// Optional description of what was approved
    pub description: Option<String>,
}

impl ApprovalGrant {
    /// Create a new approval grant.
    pub fn new(scope: ApprovalScope, description: Option<String>) -> Self {
        let granted_at_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            granted_at_secs,
            scope,
            description,
        }
    }

    /// Check if the grant has expired based on max_age.
    pub fn is_expired(&self, max_age: Duration) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        now.saturating_sub(self.granted_at_secs) > max_age.as_secs()
    }
}

/// A unique key for identifying approval requests.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ApprovalKey {
    /// Tool name (e.g., "bash", "file")
    pub tool_name: String,
    /// Action being performed (e.g., "execute", "read", "write")
    pub action: String,
    /// Optional path or target for the action
    pub target: Option<String>,
}

impl ApprovalKey {
    /// Create a new approval key.
    pub fn new(tool_name: impl Into<String>, action: impl Into<String>, target: Option<String>) -> Self {
        Self {
            tool_name: tool_name.into(),
            action: action.into(),
            target,
        }
    }
}

/// Approval cache for storing cached approval grants.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ApprovalCache {
    /// Map of approval key to cached grant
    #[serde(skip)]
    grants: HashMap<ApprovalKey, ApprovalGrant>,
}

impl ApprovalCache {
    /// Create a new empty approval cache.
    pub fn new() -> Self {
        Self {
            grants: HashMap::new(),
        }
    }

    /// Check if there's a valid cached grant for the given key.
    pub fn get(&self, key: &ApprovalKey) -> Option<&ApprovalGrant> {
        self.grants.get(key)
    }

    /// Store a grant in the cache.
    pub fn insert(&mut self, key: ApprovalKey, grant: ApprovalGrant) {
        self.grants.insert(key, grant);
    }

    /// Clear all session-scoped grants (called when session ends).
    pub fn clear_session(&mut self) {
        self.grants.retain(|_, grant| grant.scope == ApprovalScope::Persistent);
    }

    /// Clear all grants (called when clearing all cached data).
    pub fn clear(&mut self) {
        self.grants.clear();
    }

    /// Remove expired grants (those older than max_age).
    pub fn prune(&mut self, max_age: Duration) {
        self.grants.retain(|_, grant| !grant.is_expired(max_age));
    }

    /// Get the number of cached grants.
    pub fn len(&self) -> usize {
        self.grants.len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.grants.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_approval_key_equality() {
        let key1 = ApprovalKey::new("bash", "execute", Some("/tmp/test".to_string()));
        let key2 = ApprovalKey::new("bash", "execute", Some("/tmp/test".to_string()));
        let key3 = ApprovalKey::new("bash", "execute", Some("/tmp/other".to_string()));

        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_approval_cache() {
        let mut cache = ApprovalCache::new();

        let key = ApprovalKey::new("bash", "execute", None);
        let grant = ApprovalGrant::new(
            ApprovalScope::Session,
            Some("Allowed bash execution".to_string()),
        );

        assert!(cache.get(&key).is_none());
        cache.insert(key.clone(), grant);
        assert!(cache.get(&key).is_some());

        cache.clear_session();
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_persistent_grant_survives_session_clear() {
        let mut cache = ApprovalCache::new();

        let key = ApprovalKey::new("bash", "execute", None);
        let grant = ApprovalGrant::new(ApprovalScope::Persistent, None);

        cache.insert(key.clone(), grant);
        cache.clear_session();

        // Persistent grants should survive session clear
        assert!(cache.get(&key).is_some());
    }

    #[test]
    fn test_grant_expiry() {
        let mut grant = ApprovalGrant::new(ApprovalScope::Session, None);
        
        // Fresh grant should not be expired
        assert!(!grant.is_expired(Duration::from_secs(3600)));

        // Old grant should be expired
        grant.granted_at_secs = 0;
        assert!(grant.is_expired(Duration::from_secs(1)));
    }
}
