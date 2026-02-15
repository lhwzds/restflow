//! Approval cache for reducing approval fatigue.
//!
//! This module provides a three-stage escalation pattern:
//! 1. Try sandboxed execution first
//! 2. If fails due to permissions, propose specific policy amendment to user
//! 3. If approved, execute and cache the approval (scope: ThisCall | Session | Persistent)
//!
//! Future identical requests skip approval by checking the cache first.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Scope of an approval grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalScope {
    /// Valid only for this specific call (single use).
    ThisCall,
    /// Valid for the current session (cleared on restart).
    Session,
    /// Persisted across sessions (stored in SecurityAmendmentStore).
    Persistent,
}

impl Default for ApprovalScope {
    fn default() -> Self {
        Self::Session
    }
}

/// A cached approval grant.
///
/// Note: This struct intentionally does not implement Serialize/Deserialize
/// because `Instant` is not serializable. The cache is in-memory only and
/// does not need persistence (persistent grants use SecurityAmendmentStore).
#[derive(Debug, Clone)]
pub struct ApprovalGrant {
    /// Hash key for the approval.
    pub key: String,
    /// Tool name that was approved.
    pub tool_name: String,
    /// Action/command pattern that was approved.
    pub action: String,
    /// Optional path constraint.
    pub path: Option<String>,
    /// Scope of the grant.
    pub scope: ApprovalScope,
    /// When the grant was created.
    pub granted_at: Instant,
    /// Optional expiration (for session grants).
    pub expires_after: Option<Duration>,
    /// Whether this grant has been used (for ThisCall scope).
    pub used: bool,
}

impl ApprovalGrant {
    /// Create a new approval grant.
    pub fn new(
        tool_name: impl Into<String>,
        action: impl Into<String>,
        path: Option<String>,
        scope: ApprovalScope,
    ) -> Self {
        let tool_name = tool_name.into();
        let action = action.into();
        let key = compute_cache_key(&tool_name, &action, path.as_deref());

        Self {
            key,
            tool_name,
            action,
            path,
            scope,
            granted_at: Instant::now(),
            expires_after: if scope == ApprovalScope::Session {
                Some(Duration::from_secs(3600)) // 1 hour default for session
            } else {
                None
            },
            used: false,
        }
    }

    /// Check if this grant is still valid.
    pub fn is_valid(&self) -> bool {
        if self.used && self.scope == ApprovalScope::ThisCall {
            return false;
        }

        if let Some(expires_after) = self.expires_after {
            return self.granted_at.elapsed() < expires_after;
        }

        true
    }

    /// Mark this grant as used.
    pub fn mark_used(&mut self) {
        self.used = true;
    }

    /// Check if this grant matches the given request.
    pub fn matches(&self, tool_name: &str, action: &str, path: Option<&str>) -> bool {
        let request_key = compute_cache_key(tool_name, action, path);
        self.key == request_key
    }
}

/// Compute a cache key from tool name, action, and optional path.
fn compute_cache_key(tool_name: &str, action: &str, path: Option<&str>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(tool_name.as_bytes());
    hasher.update(b"\0");
    hasher.update(action.as_bytes());
    if let Some(p) = path {
        hasher.update(b"\0");
        hasher.update(p.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

/// In-memory cache for approval grants.
///
/// This cache stores session-level and this-call grants.
/// Persistent grants are stored in `SecurityAmendmentStore`.
#[derive(Debug, Default)]
pub struct ApprovalCache {
    /// Map of cache key to grant.
    grants: HashMap<String, ApprovalGrant>,
}

impl ApprovalCache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            grants: HashMap::new(),
        }
    }

    /// Check if there's a valid cached grant for the given request.
    pub fn check(&self, tool_name: &str, action: &str, path: Option<&str>) -> Option<&ApprovalGrant> {
        let key = compute_cache_key(tool_name, action, path);
        self.grants.get(&key).filter(|g| g.is_valid())
    }

    /// Add a new grant to the cache.
    ///
    /// Returns the cache key for the grant.
    pub fn grant(
        &mut self,
        tool_name: impl Into<String>,
        action: impl Into<String>,
        path: Option<String>,
        scope: ApprovalScope,
    ) -> String {
        let grant = ApprovalGrant::new(tool_name, action, path, scope);
        let key = grant.key.clone();
        self.grants.insert(key.clone(), grant);
        key
    }

    /// Add a pre-constructed grant to the cache.
    pub fn add_grant(&mut self, grant: ApprovalGrant) {
        self.grants.insert(grant.key.clone(), grant);
    }

    /// Mark a grant as used (for ThisCall scope).
    ///
    /// Returns true if the grant was found and marked.
    pub fn mark_used(&mut self, tool_name: &str, action: &str, path: Option<&str>) -> bool {
        let key = compute_cache_key(tool_name, action, path);
        if let Some(grant) = self.grants.get_mut(&key) {
            grant.mark_used();
            true
        } else {
            false
        }
    }

    /// Remove a grant from the cache.
    pub fn revoke(&mut self, tool_name: &str, action: &str, path: Option<&str>) -> Option<ApprovalGrant> {
        let key = compute_cache_key(tool_name, action, path);
        self.grants.remove(&key)
    }

    /// Clear all session-level grants (called on session end).
    pub fn clear_session_grants(&mut self) {
        self.grants.retain(|_, g| g.scope == ApprovalScope::Persistent);
    }

    /// Clear all grants (including persistent in-memory cache).
    pub fn clear_all(&mut self) {
        self.grants.clear();
    }

    /// Clean up expired grants.
    pub fn cleanup_expired(&mut self) -> usize {
        let before = self.grants.len();
        self.grants.retain(|_, g| g.is_valid());
        before - self.grants.len()
    }

    /// Get the number of active grants.
    pub fn len(&self) -> usize {
        self.grants.len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.grants.is_empty()
    }

    /// List all grants (for debugging/UI).
    pub fn list_grants(&self) -> Vec<&ApprovalGrant> {
        self.grants.values().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_cache_key() {
        let key1 = compute_cache_key("bash", "rm file.txt", None);
        let key2 = compute_cache_key("bash", "rm file.txt", None);
        let key3 = compute_cache_key("bash", "rm other.txt", None);
        let key4 = compute_cache_key("bash", "rm file.txt", Some("/tmp"));

        assert_eq!(key1, key2, "Same inputs should produce same key");
        assert_ne!(key1, key3, "Different actions should produce different keys");
        assert_ne!(key1, key4, "Different paths should produce different keys");
    }

    #[test]
    fn test_approval_grant_new() {
        let grant = ApprovalGrant::new("bash", "rm file.txt", Some("/tmp".to_string()), ApprovalScope::Session);

        assert_eq!(grant.tool_name, "bash");
        assert_eq!(grant.action, "rm file.txt");
        assert_eq!(grant.path, Some("/tmp".to_string()));
        assert_eq!(grant.scope, ApprovalScope::Session);
        assert!(!grant.used);
        assert!(grant.is_valid());
    }

    #[test]
    fn test_approval_grant_validity() {
        // ThisCall grant is valid until used
        let mut grant = ApprovalGrant::new("bash", "cmd", None, ApprovalScope::ThisCall);
        assert!(grant.is_valid());
        grant.mark_used();
        assert!(!grant.is_valid());

        // Session grant is valid until expired
        let grant = ApprovalGrant::new("bash", "cmd", None, ApprovalScope::Session);
        assert!(grant.is_valid());

        // Persistent grant is always valid
        let grant = ApprovalGrant::new("bash", "cmd", None, ApprovalScope::Persistent);
        assert!(grant.is_valid());
    }

    #[test]
    fn test_approval_grant_matches() {
        let grant = ApprovalGrant::new("bash", "rm file.txt", Some("/tmp".to_string()), ApprovalScope::Session);

        assert!(grant.matches("bash", "rm file.txt", Some("/tmp")));
        assert!(!grant.matches("bash", "rm file.txt", None));
        assert!(!grant.matches("bash", "rm other.txt", Some("/tmp")));
        assert!(!grant.matches("file", "rm file.txt", Some("/tmp")));
    }

    #[test]
    fn test_approval_cache_check() {
        let mut cache = ApprovalCache::new();

        // No grant initially
        assert!(cache.check("bash", "rm file.txt", None).is_none());

        // Add a grant
        cache.grant("bash", "rm file.txt", None, ApprovalScope::Session);

        // Should find the grant
        let result = cache.check("bash", "rm file.txt", None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().action, "rm file.txt");

        // Different action should not match
        assert!(cache.check("bash", "rm other.txt", None).is_none());
    }

    #[test]
    fn test_approval_cache_this_call_scope() {
        let mut cache = ApprovalCache::new();

        cache.grant("bash", "rm file.txt", None, ApprovalScope::ThisCall);

        // First check should work
        assert!(cache.check("bash", "rm file.txt", None).is_some());

        // Mark as used
        cache.mark_used("bash", "rm file.txt", None);

        // ThisCall grants are single-use
        assert!(cache.check("bash", "rm file.txt", None).is_none());
    }

    #[test]
    fn test_approval_cache_session_scope() {
        let mut cache = ApprovalCache::new();

        cache.grant("bash", "rm file.txt", None, ApprovalScope::Session);

        // Session grants can be used multiple times
        assert!(cache.check("bash", "rm file.txt", None).is_some());
        assert!(cache.check("bash", "rm file.txt", None).is_some());

        // Clear session grants
        cache.clear_session_grants();

        // Should be cleared
        assert!(cache.check("bash", "rm file.txt", None).is_none());
    }

    #[test]
    fn test_approval_cache_revoke() {
        let mut cache = ApprovalCache::new();

        cache.grant("bash", "rm file.txt", None, ApprovalScope::Session);
        assert!(cache.check("bash", "rm file.txt", None).is_some());

        let revoked = cache.revoke("bash", "rm file.txt", None);
        assert!(revoked.is_some());

        assert!(cache.check("bash", "rm file.txt", None).is_none());
    }

    #[test]
    fn test_approval_cache_cleanup_expired() {
        let mut cache = ApprovalCache::new();

        // Add a session grant (which has expiration)
        cache.grant("bash", "rm file.txt", None, ApprovalScope::Session);

        // Add a persistent grant (no expiration)
        cache.grant("bash", "rm other.txt", None, ApprovalScope::Persistent);

        assert_eq!(cache.len(), 2);

        // Cleanup should not remove valid grants
        let removed = cache.cleanup_expired();
        assert_eq!(removed, 0);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_approval_cache_clear_all() {
        let mut cache = ApprovalCache::new();

        cache.grant("bash", "cmd1", None, ApprovalScope::Session);
        cache.grant("bash", "cmd2", None, ApprovalScope::Persistent);

        assert_eq!(cache.len(), 2);

        cache.clear_all();

        assert!(cache.is_empty());
    }

    #[test]
    fn test_approval_cache_list_grants() {
        let mut cache = ApprovalCache::new();

        cache.grant("bash", "cmd1", None, ApprovalScope::Session);
        cache.grant("bash", "cmd2", None, ApprovalScope::Persistent);

        let grants = cache.list_grants();
        assert_eq!(grants.len(), 2);
    }
}
