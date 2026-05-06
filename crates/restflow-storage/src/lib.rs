//! RestFlow Storage - Low-level storage abstraction layer
//!
//! This crate provides config loading plus the reduced storage layer for
//! RestFlow secrets. Secrets use the only remaining redb table.
//!
//! # Architecture
//!
//! Higher-level typed wrappers are provided by `restflow-core`. New durable
//! product data should prefer explicit files such as session JSONL or agent
//! Markdown, not additional redb tables.

pub mod config;
pub mod paths;
pub mod secrets;

mod encryption;
pub mod time_utils;

pub use config::{
    AgentDefaults, AgentSettings, ApiDefaults, ApiSettings, CliConfig, ConfigDocument,
    ConfigSourcePathInfo, ConfigStorage, ConfigValueSourceInfo, ConfigValueSourceKind,
    EffectiveConfigSources, RegistryDefaults, RegistrySettings, RuntimeDefaults, RuntimeSettings,
    SystemConfig, SystemSection, effective_config_sources, load_cli_config, load_global_cli_config,
    write_cli_config,
};
pub use secrets::{Secret, SecretStorage, SecretStorageConfig};
