//! Route Binding - Multi-dimension agent routing for channel messages.
//!
//! Allows binding specific peers, accounts, channels, or a default fallback to
//! specific agents. Resolution priority: Peer > Account > Channel > Default.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::types::ChannelType;
use restflow_storage::PairingStorage;

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
    /// Legacy group binding (deprecated, maps to channel-level priority)
    /// Kept for backward compatibility with existing persisted data.
    #[serde(rename = "group")]
    Group,
}

impl RouteBindingType {
    /// Get the index prefix for this binding type.
    pub fn index_prefix(&self) -> &'static str {
        match self {
            Self::Peer => "peer",
            Self::Account => "account",
            Self::Channel => "channel",
            Self::Default => "default",
            Self::Group => "group", // Legacy: kept for migration
        }
    }

    /// Check if this is a legacy binding type that should be migrated.
    pub fn is_legacy(&self) -> bool {
        matches!(self, Self::Group)
    }
}

impl std::fmt::Display for RouteBindingType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Peer => write!(f, "peer"),
            Self::Account => write!(f, "account"),
            Self::Channel => write!(f, "channel"),
            Self::Default => write!(f, "default"),
            Self::Group => write!(f, "group"), // Legacy
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
    /// Matched by legacy group binding (deprecated)
    #[serde(rename = "group")]
    Group,
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

    /// Normalize channel identifier to stable plugin_id format.
    /// Accepts both "Telegram" and "telegram" formats, returns "telegram".
    fn normalize_channel_id(target_id: &str) -> String {
        // Try to match against known channel types (case-insensitive)
        let normalized_input = target_id.to_lowercase();
        match normalized_input.as_str() {
            "telegram" => "telegram".to_string(),
            "discord" => "discord".to_string(),
            "slack" => "slack".to_string(),
            "email" => "email".to_string(),
            "webhook" => "webhook".to_string(),
            // If unknown, return as-is (let caller handle validation)
            other => other.to_string(),
        }
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
        // Use stable plugin_id for channel key lookup (e.g., "telegram" not "Telegram")
        let channel_plugin_id = channel_type.plugin_id();
        // Use display name for session key readability
        let channel_display = channel_type.to_string();

        // 1. Check peer binding (most specific)
        let peer_key = format!("peer:{}", peer_id);
        if let Ok(Some(data)) = self.storage.resolve_route_by_key(&peer_key)
            && let Ok(binding) = serde_json::from_slice::<RouteBinding>(&data)
        {
            let session_key = format!(
                "agent:{}:{}:{}:{}:{}",
                binding.agent_id, channel_display, account_id, peer_id, chat_id
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
                binding.agent_id, channel_display, account_id, peer_id, chat_id
            );
            return Some(ResolvedRoute {
                agent_id: binding.agent_id,
                session_key,
                matched_by: MatchedBy::Account,
            });
        }

        // 3. Check channel binding
        // Try plugin_id first (stable), then display name (backward compatibility)
        let channel_key_plugin = format!("channel:{}", channel_plugin_id);
        let channel_key_display = format!("channel:{}", channel_display);

        if let Ok(Some(data)) = self.storage.resolve_route_by_key(&channel_key_plugin)
            && let Ok(binding) = serde_json::from_slice::<RouteBinding>(&data)
        {
            let session_key = format!(
                "agent:{}:{}:{}:{}:{}",
                binding.agent_id, channel_display, account_id, peer_id, chat_id
            );
            return Some(ResolvedRoute {
                agent_id: binding.agent_id,
                session_key,
                matched_by: MatchedBy::Channel,
            });
        }

        // Also try display name for backward compatibility with existing bindings
        if let Ok(Some(data)) = self.storage.resolve_route_by_key(&channel_key_display)
            && let Ok(binding) = serde_json::from_slice::<RouteBinding>(&data)
        {
            let session_key = format!(
                "agent:{}:{}:{}:{}:{}",
                binding.agent_id, channel_display, account_id, peer_id, chat_id
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
                binding.agent_id, channel_display, account_id, peer_id, chat_id
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
    /// Resolves agent using the original priority: Peer > Group > Default.
    ///
    /// # Arguments
    /// * `sender_id` - The sender's peer ID
    /// * `chat_id` - The chat/conversation ID (used for group-level binding)
    #[deprecated(note = "Use resolve_route() instead for full multi-dimension routing")]
    pub fn resolve_agent(&self, sender_id: &str, chat_id: &str) -> Option<String> {
        // 1. Check peer binding (most specific)
        let peer_key = format!("peer:{}", sender_id);
        if let Ok(Some(data)) = self.storage.resolve_route_by_key(&peer_key)
            && let Ok(binding) = serde_json::from_slice::<RouteBinding>(&data)
        {
            return Some(binding.agent_id);
        }

        // 2. Check legacy group binding (backward compatibility)
        // This preserves the original Peer > Group > Default semantics
        let group_key = format!("group:{}", chat_id);
        if let Ok(Some(data)) = self.storage.resolve_route_by_key(&group_key)
            && let Ok(binding) = serde_json::from_slice::<RouteBinding>(&data)
        {
            // Migrate legacy group binding to channel binding
            tracing::warn!(
                group_key = %group_key,
                agent_id = %binding.agent_id,
                "Using legacy group binding, consider migrating to channel binding"
            );
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
    ///
    /// Note: Creating a Group binding is deprecated and will log a warning.
    /// Use Channel binding instead.
    ///
    /// For Channel bindings, the target_id is normalized to plugin_id format
    /// (e.g., "telegram" instead of "Telegram") for stable key generation.
    pub fn bind(
        &self,
        binding_type: RouteBindingType,
        target_id: &str,
        agent_id: &str,
    ) -> Result<RouteBinding> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();

        // Normalize channel target_id to stable plugin_id format
        let normalized_target_id = if binding_type == RouteBindingType::Channel {
            Self::normalize_channel_id(target_id)
        } else {
            target_id.to_string()
        };

        let priority = match binding_type {
            RouteBindingType::Peer => 0,
            RouteBindingType::Account => 1,
            RouteBindingType::Channel => 2,
            RouteBindingType::Default => 3,
            RouteBindingType::Group => {
                tracing::warn!(
                    target_id = %target_id,
                    agent_id = %agent_id,
                    "Creating legacy Group binding, consider using Channel binding instead"
                );
                2 // Treat as Channel-level priority
            }
        };

        let binding = RouteBinding {
            id: id.clone(),
            binding_type: binding_type.clone(),
            target_id: normalized_target_id,
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
    ///
    /// Note: Silently skips bindings that fail to deserialize (e.g., corrupted data).
    /// This ensures backward compatibility when old binding types are removed.
    pub fn list(&self) -> Result<Vec<RouteBinding>> {
        let raw = self.storage.list_route_bindings()?;
        let mut bindings = Vec::with_capacity(raw.len());
        for (_id, data) in raw {
            match serde_json::from_slice::<RouteBinding>(&data) {
                Ok(binding) => bindings.push(binding),
                Err(e) => {
                    // Log and skip bindings that fail to deserialize
                    // This handles legacy binding types gracefully
                    tracing::warn!(
                        error = %e,
                        "Skipping route binding that failed to deserialize, possibly a legacy type"
                    );
                }
            }
        }
        // Sort by priority
        bindings.sort_by_key(|b| b.priority);
        Ok(bindings)
    }

    /// Migrate legacy group bindings to channel bindings.
    ///
    /// This is a one-time migration that converts all `group:{chat_id}` bindings
    /// to `channel:Telegram` bindings (or another appropriate channel type).
    pub fn migrate_group_bindings(&self) -> Result<usize> {
        let raw = self.storage.list_route_bindings()?;
        let mut migrated = 0;

        for (id, data) in raw {
            if let Ok(binding) = serde_json::from_slice::<RouteBinding>(&data)
                && binding.binding_type == RouteBindingType::Group
            {
                // Create a new channel binding with the same agent
                // Use stable plugin_id format for target_id
                let new_target_id = "telegram".to_string(); // Default to Telegram (plugin_id format)
                let new_binding = RouteBinding {
                    id: uuid::Uuid::new_v4().to_string(),
                    binding_type: RouteBindingType::Channel,
                    target_id: new_target_id.clone(),
                    agent_id: binding.agent_id.clone(),
                    created_at: binding.created_at,
                    priority: 2,
                };

                let new_key = format!("channel:{}", new_target_id);
                let new_data = serde_json::to_vec(&new_binding)?;
                self.storage
                    .add_route_binding(&new_binding.id, &new_key, &new_data)?;

                // Remove old binding
                self.storage.remove_route_binding(&id)?;

                migrated += 1;
                tracing::info!(
                    old_id = %id,
                    new_id = %new_binding.id,
                    agent_id = %binding.agent_id,
                    "Migrated group binding to channel binding"
                );
            }
        }

        Ok(migrated)
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

    #[test]
    fn test_legacy_resolve_agent_with_group_binding() {
        let resolver = create_test_resolver();

        // Create a legacy group binding
        resolver
            .bind(RouteBindingType::Group, "chat-123", "group-agent")
            .unwrap();

        // Test legacy resolve_agent method
        #[allow(deprecated)]
        let agent = resolver.resolve_agent("any-user", "chat-123");
        assert_eq!(agent, Some("group-agent".to_string()));
    }

    #[test]
    fn test_legacy_resolve_agent_peer_over_group() {
        let resolver = create_test_resolver();

        // Create both peer and group bindings
        resolver
            .bind(RouteBindingType::Peer, "user-1", "peer-agent")
            .unwrap();
        resolver
            .bind(RouteBindingType::Group, "chat-123", "group-agent")
            .unwrap();

        // Peer should win over group
        #[allow(deprecated)]
        let agent = resolver.resolve_agent("user-1", "chat-123");
        assert_eq!(agent, Some("peer-agent".to_string()));
    }

    #[test]
    fn test_group_over_default() {
        let resolver = create_test_resolver();

        // Create group and default bindings
        resolver
            .bind(RouteBindingType::Default, "*", "default-agent")
            .unwrap();
        resolver
            .bind(RouteBindingType::Group, "chat-123", "group-agent")
            .unwrap();

        // Group should win over default
        #[allow(deprecated)]
        let agent = resolver.resolve_agent("any-user", "chat-123");
        assert_eq!(agent, Some("group-agent".to_string()));
    }

    #[test]
    fn test_list_skips_corrupted_bindings() {
        let resolver = create_test_resolver();

        // Add a valid binding
        resolver
            .bind(RouteBindingType::Peer, "user-1", "peer-agent")
            .unwrap();

        // Manually add corrupted data (simulating unknown binding type from future version)
        let storage = resolver.storage.clone();
        let corrupted_id = uuid::Uuid::new_v4().to_string();
        let corrupted_key = "unknown:test".to_string();
        let corrupted_data = r#"{"id":"test","binding_type":"future_type","target_id":"test","agent_id":"test","created_at":0,"priority":99}"#;
        storage
            .add_route_binding(&corrupted_id, &corrupted_key, corrupted_data.as_bytes())
            .unwrap();

        // List should skip the corrupted binding and return only valid ones
        let bindings = resolver.list().unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].agent_id, "peer-agent");
    }

    #[test]
    fn test_group_is_legacy() {
        assert!(RouteBindingType::Group.is_legacy());
        assert!(!RouteBindingType::Peer.is_legacy());
        assert!(!RouteBindingType::Channel.is_legacy());
    }

    #[test]
    fn test_migrate_group_bindings() {
        let resolver = create_test_resolver();

        // Create a single legacy group binding for clear migration test
        resolver
            .bind(RouteBindingType::Group, "chat-1", "agent-1")
            .unwrap();
        // Also create a non-group binding
        resolver
            .bind(RouteBindingType::Peer, "user-1", "peer-agent")
            .unwrap();

        // Migrate
        let migrated = resolver.migrate_group_bindings().unwrap();
        assert_eq!(migrated, 1);

        // Verify old group binding is gone
        #[allow(deprecated)]
        let agent = resolver.resolve_agent("any-user", "chat-1");
        assert_eq!(agent, None);

        // Verify peer binding still exists
        #[allow(deprecated)]
        let agent = resolver.resolve_agent("user-1", "chat-1");
        assert_eq!(agent, Some("peer-agent".to_string()));

        // Verify the migrated channel binding exists
        let route =
            resolver.resolve_route(ChannelType::Telegram, "any-bot", "any-user", "any-chat");
        assert!(route.is_some());
        assert_eq!(route.unwrap().agent_id, "agent-1");
    }

    #[test]
    fn test_channel_binding_uses_plugin_id() {
        let resolver = create_test_resolver();

        // Bind with display name "Telegram"
        resolver
            .bind(RouteBindingType::Channel, "Telegram", "channel-agent")
            .unwrap();

        // Should be stored with plugin_id key "telegram"
        let route = resolver
            .resolve_route(ChannelType::Telegram, "any-bot", "any-user", "chat-1")
            .unwrap();
        assert_eq!(route.agent_id, "channel-agent");
        assert_eq!(route.matched_by, MatchedBy::Channel);

        // Verify stored with plugin_id
        let bindings = resolver.list().unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].target_id, "telegram");
    }

    #[test]
    fn test_channel_binding_accepts_plugin_id_format() {
        let resolver = create_test_resolver();

        // Bind directly with plugin_id format
        resolver
            .bind(RouteBindingType::Channel, "telegram", "channel-agent")
            .unwrap();

        // Should resolve correctly
        let route = resolver
            .resolve_route(ChannelType::Telegram, "any-bot", "any-user", "chat-1")
            .unwrap();
        assert_eq!(route.agent_id, "channel-agent");
    }

    #[test]
    fn test_channel_binding_backward_compatible_display_name() {
        let resolver = create_test_resolver();

        // Simulate a legacy binding stored with display name by manually inserting
        let storage = resolver.storage.clone();
        let legacy_binding = RouteBinding {
            id: uuid::Uuid::new_v4().to_string(),
            binding_type: RouteBindingType::Channel,
            target_id: "Telegram".to_string(), // Old display name format
            agent_id: "legacy-agent".to_string(),
            created_at: chrono::Utc::now().timestamp_millis(),
            priority: 2,
        };
        let legacy_key = "channel:Telegram".to_string();
        let legacy_data = serde_json::to_vec(&legacy_binding).unwrap();
        storage
            .add_route_binding(&legacy_binding.id, &legacy_key, &legacy_data)
            .unwrap();

        // Should still resolve via backward-compatible lookup
        let route = resolver
            .resolve_route(ChannelType::Telegram, "any-bot", "any-user", "chat-1")
            .unwrap();
        assert_eq!(route.agent_id, "legacy-agent");
    }
}
