use restflow_storage::SystemConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    UnderDevelopment,
    Experimental,
    Stable,
    Deprecated,
    Removed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    BackgroundAgents,
    Triggers,
    WebSocketTransport,
    StuckDetection,
    ResourceTracker,
    PlanMode,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FeatureDescriptor {
    pub key: String,
    pub stage: Stage,
    pub description: &'static str,
    pub enabled: bool,
    pub requires_opt_in: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Features {
    experimental_opt_in: HashSet<Feature>,
}

impl Feature {
    pub const ALL: [Feature; 6] = [
        Feature::BackgroundAgents,
        Feature::Triggers,
        Feature::WebSocketTransport,
        Feature::StuckDetection,
        Feature::ResourceTracker,
        Feature::PlanMode,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Feature::BackgroundAgents => "background_agents",
            Feature::Triggers => "triggers",
            Feature::WebSocketTransport => "websocket_transport",
            Feature::StuckDetection => "stuck_detection",
            Feature::ResourceTracker => "resource_tracker",
            Feature::PlanMode => "plan_mode",
        }
    }

    pub fn stage(self) -> Stage {
        match self {
            Feature::BackgroundAgents => Stage::Stable,
            Feature::Triggers => Stage::Stable,
            Feature::WebSocketTransport => Stage::Experimental,
            Feature::StuckDetection => Stage::Experimental,
            Feature::ResourceTracker => Stage::UnderDevelopment,
            Feature::PlanMode => Stage::Experimental,
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Feature::BackgroundAgents => "Run long-lived AI tasks in background workers.",
            Feature::Triggers => "Activate workflows and tasks by event or schedule.",
            Feature::WebSocketTransport => "Use websocket transport for live client streams.",
            Feature::StuckDetection => "Detect and recover tasks that stop making progress.",
            Feature::ResourceTracker => "Track CPU and memory usage for running tasks.",
            Feature::PlanMode => "Allow explicit user-plan checkpoints in agent execution.",
        }
    }

    pub fn requires_opt_in(self) -> bool {
        matches!(self.stage(), Stage::Experimental)
    }
}

impl FromStr for Feature {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let normalized = value.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "background_agents" => Ok(Feature::BackgroundAgents),
            "triggers" => Ok(Feature::Triggers),
            "websocket_transport" | "websocket" => Ok(Feature::WebSocketTransport),
            "stuck_detection" => Ok(Feature::StuckDetection),
            "resource_tracker" => Ok(Feature::ResourceTracker),
            "plan_mode" => Ok(Feature::PlanMode),
            _ => Err(()),
        }
    }
}

impl Features {
    pub fn from_config(config: &SystemConfig) -> Self {
        let experimental_opt_in = config
            .experimental_features
            .iter()
            .filter_map(|value| Feature::from_str(value).ok())
            .filter(|feature| feature.requires_opt_in())
            .collect::<HashSet<_>>();

        Self {
            experimental_opt_in,
        }
    }

    pub fn is_enabled(&self, feature: Feature) -> bool {
        match feature.stage() {
            Stage::Stable => true,
            Stage::Experimental => self.experimental_opt_in.contains(&feature),
            Stage::UnderDevelopment | Stage::Deprecated | Stage::Removed => false,
        }
    }

    pub fn descriptors(&self) -> Vec<FeatureDescriptor> {
        Feature::ALL
            .iter()
            .copied()
            .map(|feature| FeatureDescriptor {
                key: feature.key().to_string(),
                stage: feature.stage(),
                description: feature.description(),
                enabled: self.is_enabled(feature),
                requires_opt_in: feature.requires_opt_in(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_flags(flags: &[&str]) -> SystemConfig {
        let mut config = SystemConfig::default();
        config.experimental_features = flags.iter().map(|v| (*v).to_string()).collect();
        config
    }

    #[test]
    fn test_stable_feature_enabled_by_default() {
        let features = Features::from_config(&SystemConfig::default());
        assert!(features.is_enabled(Feature::BackgroundAgents));
    }

    #[test]
    fn test_experimental_feature_requires_opt_in() {
        let without_opt_in = Features::from_config(&SystemConfig::default());
        assert!(!without_opt_in.is_enabled(Feature::PlanMode));

        let with_opt_in = Features::from_config(&config_with_flags(&["plan_mode"]));
        assert!(with_opt_in.is_enabled(Feature::PlanMode));
    }

    #[test]
    fn test_unknown_feature_in_config_is_ignored() {
        let config = config_with_flags(&["unknown_feature", "plan_mode"]);
        let features = Features::from_config(&config);
        assert!(features.is_enabled(Feature::PlanMode));
        assert!(!features.is_enabled(Feature::WebSocketTransport));
    }
}
