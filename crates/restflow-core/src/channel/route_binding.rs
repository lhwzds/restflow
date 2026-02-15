//! Route Binding - Multi-dimension agent routing for channel messages.
//!
//! Allows binding specific peers, accounts, channels, or a default fallback to
//! specific agents. Resolution priority: Peer > Account > Channel > Default.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use restflow_storage::PairingStorage;
use super::types::ChannelType;

/// Type of route binding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteBindingType {
    /// Specific user -> agent (priority 0)
    Peer,
    /// Specific bot account -> agent (priority 1)
    Account,
    /// Specific channel type -> agent (priority 2)
    Channel,
    /// Fallback for all (priority 3)
    Default,
}

impl RouteBindingType {
    /// Get the index prefix for this binding type.
    pub fn index_prefix(&self) -> &'static str {
        match self {
            Self::Peer => "peer",
            Self::Account => "account",
            Self::Channel => "channel",
            Self::Default => "default",
        }
    }
}

impl std::fmt::Display for RouteBindingType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Peer => write!(f, "peer"),
            Self::Account => write!(f, "account"),
            Self::Channel => write!(f, "channel"),
            Self::Default => write!(f, "default"),
        }
    }
}

/// A route binding that maps a target to an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteBinding {
    pub id: String,
    pub binding_type: RouteBindingType,
    /// peer_id, account_id, channel_type, or "*" for default
    pub target_id: String,
    /// Which agent handles messages
    pub agent_id: String,
    pub created_at: i64,
    /// Lower = higher priority
    pub priority: u8,
}

/// How a route was matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchedBy {
    /// Matched by peer-specific binding
    Peer,
    /// Matched by account-specific binding
    Account,
    /// Matched by channel-specific binding
    Channel,
    /// Matched by default binding
    Default,
}

/// Resolved route with agent ID and match metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedRoute {
    /// Agent ID to handle the message
    pub agent_id: String,
    /// Session key for isolation: `agent:{id}:{channel}:{account}:{peer}`
    pub session_key: String,
    /// How the route was matched
    pub matched_by: MatchedBy,
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
    /// Priority: Peer binding > Account binding > Channel binding > Default binding
    ///
    /// # Arguments
    /// * `channel_type` - The channel type (Telegram, Discord, etc.)
    /// * `account_id` - The bot account identifier (e.g., bot username or token hash)
    /// * `peer_id` - The sender's peer ID
    /// * `chat_id` - The chat/conversation ID (used for session key)
    pub fn resolve_route(
        &self,
        channel_type: ChannelType,
        account_id: &str,
        peer_id: &str,
        chat_id: &str,
    ) -> Option<ResolvedRoute> {
        let channel_str = channel_type.to_string();

        // 1. Check peer binding (most specific)
        let peer_key = format!("peer:{}", peer_id);
        if let Ok(Some(data)) = self.storage.resolve_route_by_key(&peer_key)
            && let Ok(binding) = serde_json::from_slice::<RouteBinding>(&data)
        {
            let session_key = format!(
                "agent:{}:{}:{}:{}:{}",
                binding.agent_id, channel_str, account_id, peer_id, chat_id
            );
            return Some(ResolvedRoute {
                agent_id: binding.agent_id,
                session_key,
                matched_by: MatchedBy::Peer,
            });
        }

        // 2. Check account binding
        let account_key = format!("account:{}", account_id);
        if let Ok(Some(data)) = self.storage.resolve_route_by_key(&account_key)
            && let Ok(binding) = serde_json::from_slice::<RouteBinding>(&data)
        {
            let session_key = format!(
                "agent:{}:{}:{}:{}:{}",
                binding.agent_id, channel_str, account_id, peer_id, chat_id
            );
            return Some(ResolvedRoute {
                agent_id: binding.agent_id,
                session_key,
                matched_by: MatchedBy::Account,
            });
        }

        // 3. Check channel binding
        let channel_key = format!("channel:{}", channel_str);
        if let Ok(Some(data)) = self.storage.resolve_route_by_key(&channel_key)
            && let Ok(binding) = serde_json::from_slice::<RouteBinding>(&data)
        {
            let session_key = format!(
                "agent:{}:{}:{}:{}:{}",
                binding.agent_id, channel_str, account_id, peer_id, chat_id
            );
            return Some(ResolvedRoute {
                agent_id: binding.agent_id,
                session_key,
                matched_by: MatchedBy::Channel,
            });
        }

        // 4. Check default binding
        if let Ok(Some(data)) = self.storage.resolve_route_by_key("default:*")
            && let Ok(binding) = serde_json::from_slice::<RouteBinding>(&data)
        {
            let session_key = format!(
                "agent:{}:{}:{}:{}:{}",
                binding.agent_id, channel_str, account_id, peer_id, chat_id
            );
            return Some(ResolvedRoute {
                agent_id: binding.agent_id,
                session_key,
                matched_by: MatchedBy::Default,
            });
        }

        None
    }

    /// Legacy method for backward compatibility.
    /// Resolves agent using only peer_id and chat_id.
    #[deprecated(note = "Use resolve_route() instead for full multi-dimension routing")]
    pub fn resolve_agent(&self, sender_id: &str, _chat_id: &str) -> Option<String> {
        // Check peer binding only for legacy compatibility
        let peer_key = format!("peer:{}", sender_id);
        if let Ok(Some(data)) = self.storage.resolve_route_by_key(&peer_key)
            && let Ok(binding) = serde_json::from_slice::<RouteBinding>(&data)
        {
            return Some(binding.agent_id);
        }

        // Check default binding
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
            RouteBindingType::Account => 1,
            RouteBindingType::Channel => 2,
            RouteBindingType::Default => 3,
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
            .bind(RouteBindingType::Peer, "user-12345", "peer-agent")
            .unwrap();

        let route = resolver
            .resolve_route(ChannelType::Telegram, "bot-1", "user-12345", "chat-1")
            .unwrap();
        assert_eq!(route.agent_id, "peer-agent");
        assert_eq!(route.matched_by, MatchedBy::Peer);
        assert!(route.session_key.contains("user-12345"));
    }

    #[test]
    fn test_resolve_account_binding() {
        let resolver = create_test_resolver();
        resolver
            .bind(RouteBindingType::Account, "my-bot", "account-agent")
            .unwrap();

        let route = resolver
            .resolve_route(ChannelType::Telegram, "my-bot", "any-user", "chat-1")
            .unwrap();
        assert_eq!(route.agent_id, "account-agent");
        assert_eq!(route.matched_by, MatchedBy::Account);
    }

    #[test]
    fn test_resolve_channel_binding() {
        let resolver = create_test_resolver();
        resolver
            .bind(RouteBindingType::Channel, "Telegram", "channel-agent")
            .unwrap();

        let route = resolver
            .resolve_route(ChannelType::Telegram, "any-bot", "any-user", "chat-1")
            .unwrap();
        assert_eq!(route.agent_id, "channel-agent");
        assert_eq!(route.matched_by, MatchedBy::Channel);
    }

    #[test]
    fn test_resolve_default_binding() {
        let resolver = create_test_resolver();
        resolver
            .bind(RouteBindingType::Default, "*", "default-agent")
            .unwrap();

        let route = resolver
            .resolve_route(ChannelType::Telegram, "any-bot", "any-user", "chat-1")
            .unwrap();
        assert_eq!(route.agent_id, "default-agent");
        assert_eq!(route.matched_by, MatchedBy::Default);
    }

    #[test]
    fn test_no_binding_returns_none() {
        let resolver = create_test_resolver();

        let route = resolver.resolve_route(ChannelType::Telegram, "bot-1", "user-1", "chat-1");
        assert!(route.is_none());
    }

    #[test]
    fn test_peer_takes_priority_over_account() {
        let resolver = create_test_resolver();
        resolver
            .bind(RouteBindingType::Account, "bot-1", "account-agent")
            .unwrap();
        resolver
            .bind(RouteBindingType::Peer, "user-12345", "peer-agent")
            .unwrap();

        let route = resolver
            .resolve_route(ChannelType::Telegram, "bot-1", "user-12345", "chat-1")
            .unwrap();
        assert_eq!(route.agent_id, "peer-agent");
        assert_eq!(route.matched_by, MatchedBy::Peer);
    }

    #[test]
    fn test_account_takes_priority_over_channel() {
        let resolver = create_test_resolver();
        resolver
            .bind(RouteBindingType::Channel, "Telegram", "channel-agent")
            .unwrap();
        resolver
            .bind(RouteBindingType::Account, "bot-1", "account-agent")
            .unwrap();

        let route = resolver
            .resolve_route(ChannelType::Telegram, "bot-1", "any-user", "chat-1")
            .unwrap();
        assert_eq!(route.agent_id, "account-agent");
        assert_eq!(route.matched_by, MatchedBy::Account);
    }

    #[test]
    fn test_channel_takes_priority_over_default() {
        let resolver = create_test_resolver();
        resolver
            .bind(RouteBindingType::Default, "*", "default-agent")
            .unwrap();
        resolver
            .bind(RouteBindingType::Channel, "Telegram", "channel-agent")
            .unwrap();

        let route = resolver
            .resolve_route(ChannelType::Telegram, "any-bot", "any-user", "chat-1")
            .unwrap();
        assert_eq!(route.agent_id, "channel-agent");
        assert_eq!(route.matched_by, MatchedBy::Channel);
    }

    #[test]
    fn test_full_priority_chain() {
        let resolver = create_test_resolver();
        resolver
            .bind(RouteBindingType::Default, "*", "default-agent")
            .unwrap();
        resolver
            .bind(RouteBindingType::Channel, "Telegram", "channel-agent")
            .unwrap();
        resolver
            .bind(RouteBindingType::Account, "bot-1", "account-agent")
            .unwrap();
        resolver
            .bind(RouteBindingType::Peer, "user-12345", "peer-agent")
            .unwrap();

        // Peer wins
        let route = resolver
            .resolve_route(ChannelType::Telegram, "bot-1", "user-12345", "chat-1")
            .unwrap();
        assert_eq!(route.agent_id, "peer-agent");

        // Account wins (no peer binding for this user)
        let route = resolver
            .resolve_route(ChannelType::Telegram, "bot-1", "other-user", "chat-1")
            .unwrap();
        assert_eq!(route.agent_id, "account-agent");

        // Channel wins (no account binding for this bot)
        let route = resolver
            .resolve_route(ChannelType::Telegram, "other-bot", "other-user", "chat-1")
            .unwrap();
        assert_eq!(route.agent_id, "channel-agent");

        // Default wins (different channel with no bindings)
        let route = resolver
            .resolve_route(ChannelType::Discord, "any-bot", "any-user", "chat-1")
            .unwrap();
        assert_eq!(route.agent_id, "default-agent");
    }

    #[test]
    fn test_session_key_isolation() {
        let resolver = create_test_resolver();
        resolver
            .bind(RouteBindingType::Default, "*", "default-agent")
            .unwrap();

        let route1 = resolver
            .resolve_route(ChannelType::Telegram, "bot-1", "user-1", "chat-1")
            .unwrap();
        let route2 = resolver
            .resolve_route(ChannelType::Telegram, "bot-1", "user-2", "chat-1")
            .unwrap();
        let route3 = resolver
            .resolve_route(ChannelType::Discord, "bot-1", "user-1", "chat-1")
            .unwrap();

        // Each should have unique session key
        assert_ne!(route1.session_key, route2.session_key);
        assert_ne!(route1.session_key, route3.session_key);
        assert_ne!(route2.session_key, route3.session_key);
    }

    #[test]
    fn test_unbind() {
        let resolver = create_test_resolver();
        let binding = resolver
            .bind(RouteBindingType::Peer, "user-12345", "agent-1")
            .unwrap();

        assert!(
            resolver
                .resolve_route(ChannelType::Telegram, "bot-1", "user-12345", "chat-1")
                .is_some()
        );
        assert!(resolver.unbind(&binding.id).unwrap());
        assert!(
            resolver
                .resolve_route(ChannelType::Telegram, "bot-1", "user-12345", "chat-1")
                .is_none()
        );
    }

    #[test]
    fn test_list_bindings() {
        let resolver = create_test_resolver();
        resolver
            .bind(RouteBindingType::Default, "*", "default-agent")
            .unwrap();
        resolver
            .bind(RouteBindingType::Peer, "user-12345", "peer-agent")
            .unwrap();
        resolver
            .bind(RouteBindingType::Channel, "Telegram", "channel-agent")
            .unwrap();
        resolver
            .bind(RouteBindingType::Account, "bot-1", "account-agent")
            .unwrap();

        let bindings = resolver.list().unwrap();
        assert_eq!(bindings.len(), 4);
        // Should be sorted by priority
        assert_eq!(bindings[0].priority, 0); // peer
        assert_eq!(bindings[1].priority, 1); // account
        assert_eq!(bindings[2].priority, 2); // channel
        assert_eq!(bindings[3].priority, 3); // default
    }
}
