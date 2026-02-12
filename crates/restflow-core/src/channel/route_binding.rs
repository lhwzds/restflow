//! Route Binding - Multi-dimension agent routing for channel messages.
//!
//! Allows binding specific peers, groups, or a default fallback to
//! specific agents. Resolution priority: Peer > Group > Default.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use restflow_storage::PairingStorage;

/// Type of route binding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteBindingType {
    /// Specific user -> agent (priority 0)
    Peer,
    /// Specific group/supergroup -> agent (priority 1)
    Group,
    /// Fallback for all (priority 2)
    Default,
}

impl RouteBindingType {
    /// Get the index prefix for this binding type.
    pub fn index_prefix(&self) -> &'static str {
        match self {
            Self::Peer => "peer",
            Self::Group => "group",
            Self::Default => "default",
        }
    }
}

impl std::fmt::Display for RouteBindingType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Peer => write!(f, "peer"),
            Self::Group => write!(f, "group"),
            Self::Default => write!(f, "default"),
        }
    }
}

/// A route binding that maps a target to an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteBinding {
    pub id: String,
    pub binding_type: RouteBindingType,
    /// peer_id, group_id, or "*" for default
    pub target_id: String,
    /// Which agent handles messages
    pub agent_id: String,
    pub created_at: i64,
    /// Lower = higher priority
    pub priority: u8,
}

/// Resolves which agent should handle a message based on route bindings.
pub struct RouteResolver {
    storage: Arc<PairingStorage>,
}

impl RouteResolver {
    /// Create a new RouteResolver.
    pub fn new(storage: Arc<PairingStorage>) -> Self {
        Self { storage }
    }

    /// Resolve which agent should handle a message.
    /// Priority: Peer binding > Group binding > Default binding
    pub fn resolve_agent(&self, sender_id: &str, chat_id: &str) -> Option<String> {
        // 1. Check peer binding
        let peer_key = format!("peer:{}", sender_id);
        if let Ok(Some(data)) = self.storage.resolve_route_by_key(&peer_key)
            && let Ok(binding) = serde_json::from_slice::<RouteBinding>(&data)
        {
            return Some(binding.agent_id);
        }

        // 2. Check group binding
        let group_key = format!("group:{}", chat_id);
        if let Ok(Some(data)) = self.storage.resolve_route_by_key(&group_key)
            && let Ok(binding) = serde_json::from_slice::<RouteBinding>(&data)
        {
            return Some(binding.agent_id);
        }

        // 3. Check default binding
        if let Ok(Some(data)) = self.storage.resolve_route_by_key("default:*")
            && let Ok(binding) = serde_json::from_slice::<RouteBinding>(&data)
        {
            return Some(binding.agent_id);
        }

        None
    }

    /// Add a route binding.
    pub fn bind(
        &self,
        binding_type: RouteBindingType,
        target_id: &str,
        agent_id: &str,
    ) -> Result<RouteBinding> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let priority = match binding_type {
            RouteBindingType::Peer => 0,
            RouteBindingType::Group => 1,
            RouteBindingType::Default => 2,
        };

        let binding = RouteBinding {
            id: id.clone(),
            binding_type: binding_type.clone(),
            target_id: target_id.to_string(),
            agent_id: agent_id.to_string(),
            created_at: now,
            priority,
        };

        let index_key = format!("{}:{}", binding_type.index_prefix(), target_id);
        let data = serde_json::to_vec(&binding)?;
        self.storage.add_route_binding(&id, &index_key, &data)?;

        Ok(binding)
    }

    /// Remove a route binding by id.
    pub fn unbind(&self, id: &str) -> Result<bool> {
        self.storage.remove_route_binding(id)
    }

    /// List all route bindings.
    pub fn list(&self) -> Result<Vec<RouteBinding>> {
        let raw = self.storage.list_route_bindings()?;
        let mut bindings = Vec::with_capacity(raw.len());
        for (_id, data) in raw {
            let binding: RouteBinding = serde_json::from_slice(&data)?;
            bindings.push(binding);
        }
        // Sort by priority
        bindings.sort_by_key(|b| b.priority);
        Ok(bindings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::Database;
    use tempfile::NamedTempFile;

    fn create_test_resolver() -> RouteResolver {
        let tmp = NamedTempFile::new().unwrap();
        let db = Arc::new(Database::create(tmp.path()).unwrap());
        let storage = Arc::new(PairingStorage::new(db).unwrap());
        RouteResolver::new(storage)
    }

    #[test]
    fn test_resolve_peer_binding() {
        let resolver = create_test_resolver();
        resolver
            .bind(RouteBindingType::Peer, "12345", "peer-agent")
            .unwrap();

        let agent = resolver.resolve_agent("12345", "any-chat");
        assert_eq!(agent, Some("peer-agent".to_string()));
    }

    #[test]
    fn test_resolve_group_binding() {
        let resolver = create_test_resolver();
        resolver
            .bind(RouteBindingType::Group, "group-1", "group-agent")
            .unwrap();

        // No peer binding, should fall through to group
        let agent = resolver.resolve_agent("unknown-peer", "group-1");
        assert_eq!(agent, Some("group-agent".to_string()));
    }

    #[test]
    fn test_resolve_default_binding() {
        let resolver = create_test_resolver();
        resolver
            .bind(RouteBindingType::Default, "*", "default-agent")
            .unwrap();

        let agent = resolver.resolve_agent("unknown-peer", "unknown-chat");
        assert_eq!(agent, Some("default-agent".to_string()));
    }

    #[test]
    fn test_no_binding_returns_none() {
        let resolver = create_test_resolver();

        let agent = resolver.resolve_agent("12345", "chat-1");
        assert_eq!(agent, None);
    }

    #[test]
    fn test_peer_takes_priority_over_group() {
        let resolver = create_test_resolver();
        resolver
            .bind(RouteBindingType::Group, "chat-1", "group-agent")
            .unwrap();
        resolver
            .bind(RouteBindingType::Peer, "12345", "peer-agent")
            .unwrap();
        resolver
            .bind(RouteBindingType::Default, "*", "default-agent")
            .unwrap();

        let agent = resolver.resolve_agent("12345", "chat-1");
        assert_eq!(agent, Some("peer-agent".to_string()));
    }

    #[test]
    fn test_group_takes_priority_over_default() {
        let resolver = create_test_resolver();
        resolver
            .bind(RouteBindingType::Default, "*", "default-agent")
            .unwrap();
        resolver
            .bind(RouteBindingType::Group, "chat-1", "group-agent")
            .unwrap();

        let agent = resolver.resolve_agent("unknown-peer", "chat-1");
        assert_eq!(agent, Some("group-agent".to_string()));
    }

    #[test]
    fn test_unbind() {
        let resolver = create_test_resolver();
        let binding = resolver
            .bind(RouteBindingType::Peer, "12345", "agent-1")
            .unwrap();

        assert!(resolver.resolve_agent("12345", "any").is_some());
        assert!(resolver.unbind(&binding.id).unwrap());
        assert!(resolver.resolve_agent("12345", "any").is_none());
    }

    #[test]
    fn test_list_bindings() {
        let resolver = create_test_resolver();
        resolver
            .bind(RouteBindingType::Default, "*", "default-agent")
            .unwrap();
        resolver
            .bind(RouteBindingType::Peer, "12345", "peer-agent")
            .unwrap();
        resolver
            .bind(RouteBindingType::Group, "group-1", "group-agent")
            .unwrap();

        let bindings = resolver.list().unwrap();
        assert_eq!(bindings.len(), 3);
        // Should be sorted by priority
        assert_eq!(bindings[0].priority, 0); // peer
        assert_eq!(bindings[1].priority, 1); // group
        assert_eq!(bindings[2].priority, 2); // default
    }
}
