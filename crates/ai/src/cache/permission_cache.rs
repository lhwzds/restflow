use dashmap::DashMap;
use std::time::{Duration, Instant};

/// Permission cache key
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct PermissionKey {
    session_id: String,
    tool_name: String,
    action: String,
    path: String,
}

/// Cached permission entry
#[derive(Debug, Clone)]
struct PermissionEntry {
    granted: bool,
    granted_at: Instant,
}

/// Session-scoped permission cache
#[derive(Debug)]
pub struct PermissionCache {
    cache: DashMap<PermissionKey, PermissionEntry>,
    ttl: Duration,
}

impl PermissionCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            cache: DashMap::new(),
            ttl,
        }
    }

    /// Check if permission is cached
    pub fn check(
        &self,
        session_id: &str,
        tool_name: &str,
        action: &str,
        path: &str,
    ) -> Option<bool> {
        let key = PermissionKey {
            session_id: session_id.to_string(),
            tool_name: tool_name.to_string(),
            action: action.to_string(),
            path: path.to_string(),
        };

        self.cache.get(&key).and_then(|entry| {
            if entry.granted_at.elapsed() < self.ttl {
                Some(entry.granted)
            } else {
                None
            }
        })
    }

    /// Store permission decision
    pub fn store(
        &self,
        session_id: &str,
        tool_name: &str,
        action: &str,
        path: &str,
        granted: bool,
    ) {
        let key = PermissionKey {
            session_id: session_id.to_string(),
            tool_name: tool_name.to_string(),
            action: action.to_string(),
            path: path.to_string(),
        };

        self.cache.insert(
            key,
            PermissionEntry {
                granted,
                granted_at: Instant::now(),
            },
        );
    }

    /// Clear all permissions for a session
    pub fn clear_session(&self, session_id: &str) {
        self.cache.retain(|key, _| key.session_id != session_id);
    }

    /// Clear expired entries (call periodically)
    pub fn cleanup(&self) {
        self.cache
            .retain(|_, value| value.granted_at.elapsed() < self.ttl);
    }
}
