//! Skill Registry module for managing skill sources and installation.
//!
//! This module provides the infrastructure for:
//! - Discovering skills from multiple sources (local, builtin, marketplace, GitHub)
//! - Installing and updating skills with dependency resolution
//! - Checking gating requirements before installation

mod cache;
mod gating;
mod github;
mod marketplace;
mod provider;
#[allow(clippy::module_inception)]
mod registry;
mod resolver;

pub use gating::GatingChecker;
pub use github::GitHubProvider;
pub use marketplace::{DEFAULT_MARKETPLACE_URL, MarketplaceProvider};
pub use provider::{
    BuiltinSkillProvider, LocalSkillProvider, SkillProvider, SkillProviderError, SkillSearchQuery,
    SkillSearchResult, SkillSortOrder,
};
pub use registry::{SkillRegistry, SkillRegistryConfig};
pub use resolver::{DependencyError, DependencyResolver, InstallAction, InstallPlan};
