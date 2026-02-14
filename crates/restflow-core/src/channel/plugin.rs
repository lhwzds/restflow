//! Channel plugin registry and trait abstractions.
//!
//! This module provides a plugin-oriented registry so channels can be added
//! without modifying router internals.

use std::collections::HashMap;
use std::sync::Arc;

use super::traits::Channel;
use super::types::ChannelType;

/// Plugin contract for communication channels.
///
/// This extends [`Channel`] and adds stable plugin metadata for registration.
pub trait ChannelPlugin: Channel {
    /// Stable plugin identifier (e.g. `telegram`, `slack`).
    fn channel_id(&self) -> &'static str {
        self.channel_type().plugin_id()
    }

    /// Display name shown in UI/logs.
    fn display_name(&self) -> &'static str {
        self.channel_type().display_name()
    }

    /// Whether the plugin is currently connected.
    fn is_connected(&self) -> bool {
        self.is_configured()
    }
}

impl<T> ChannelPlugin for T where T: Channel + ?Sized {}

/// Registry for channel plugins.
pub struct ChannelRegistry {
    channels: HashMap<ChannelType, Arc<dyn ChannelPlugin>>,
    default_conversations: HashMap<ChannelType, String>,
}

impl ChannelRegistry {
    /// Create an empty plugin registry.
    pub fn new() -> Self {
        Self {
            channels: HashMap::new(),
            default_conversations: HashMap::new(),
        }
    }

    /// Register or replace a channel plugin.
    pub fn register<C: ChannelPlugin + 'static>(&mut self, channel: C) {
        self.channels
            .insert(channel.channel_type(), Arc::new(channel));
    }

    /// Register a plugin and its default conversation ID.
    pub fn register_with_default<C: ChannelPlugin + 'static>(
        &mut self,
        channel: C,
        default_conversation: impl Into<String>,
    ) {
        let channel_type = channel.channel_type();
        self.default_conversations
            .insert(channel_type, default_conversation.into());
        self.register(channel);
    }

    /// Get plugin by channel type.
    pub fn get(&self, channel_type: ChannelType) -> Option<&Arc<dyn ChannelPlugin>> {
        self.channels.get(&channel_type)
    }

    /// Check if channel has default conversation configured.
    pub fn has_default_conversation(&self, channel_type: ChannelType) -> bool {
        self.default_conversations.contains_key(&channel_type)
    }

    /// Return configured default conversation for channel.
    pub fn default_conversation(&self, channel_type: ChannelType) -> Option<&str> {
        self.default_conversations
            .get(&channel_type)
            .map(String::as_str)
    }

    /// Check whether a channel is registered and configured.
    pub fn is_available(&self, channel_type: ChannelType) -> bool {
        self.channels
            .get(&channel_type)
            .map(|c| c.is_configured())
            .unwrap_or(false)
    }

    /// Snapshot all registered channels.
    pub fn channels(&self) -> Vec<(ChannelType, Arc<dyn ChannelPlugin>)> {
        self.channels
            .iter()
            .map(|(channel_type, channel)| (*channel_type, Arc::clone(channel)))
            .collect()
    }

    /// Number of registered channels.
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// List all configured channels.
    pub fn list_configured(&self) -> Vec<ChannelType> {
        self.channels
            .iter()
            .filter(|(_, c)| c.is_configured())
            .map(|(t, _)| *t)
            .collect()
    }

    /// List interactive and configured channels.
    pub fn list_interactive(&self) -> Vec<ChannelType> {
        self.channels
            .iter()
            .filter(|(_, c)| c.is_configured() && c.supports_interaction())
            .map(|(t, _)| *t)
            .collect()
    }
}

impl Default for ChannelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::mock::MockChannel;

    #[test]
    fn test_registry_register_and_lookup() {
        let mut registry = ChannelRegistry::new();
        registry.register(MockChannel::new(ChannelType::Telegram));

        let plugin = registry.get(ChannelType::Telegram).expect("plugin exists");
        assert_eq!(plugin.channel_id(), "telegram");
        assert_eq!(plugin.display_name(), "Telegram");
        assert!(plugin.is_connected());
    }

    #[test]
    fn test_registry_default_conversation() {
        let mut registry = ChannelRegistry::new();
        registry.register_with_default(MockChannel::new(ChannelType::Discord), "room-1");

        assert!(registry.has_default_conversation(ChannelType::Discord));
        assert_eq!(
            registry.default_conversation(ChannelType::Discord),
            Some("room-1")
        );
    }
}
