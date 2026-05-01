//! # codocia
//!
//! Restflow is the minimal compatibility facade for the engine boundary.
//!
//! ## Owns
//! - stable engine import shape for examples
//! - protocol entrypoint re-exports
//! - executable skill runtime re-exports
//!
//! ## Must Not
//! - own runtime behavior
//! - own persistence
//! - duplicate module logic
//!
//! ## Inputs
//! - engine boundary
//! - protocol types
//!
//! ## Outputs
//! - minimal Rust API surface
//!
//! ## Depends On
//! - engine
//! - proto
//!
//! ## Verify
//! - cargo check -p restflow

pub use engine::{Core, CoreStores};
pub use proto::{CoreCommand, CoreResponse, CoreSnapshot};
pub use runtime::{
    ArtifactKind, ArtifactProtocol, ArtifactSchema, ArtifactSource, BuildOptions, ScaffoldOptions,
    SkillArtifact, SkillRunOutput, build_skill, load_artifact, run_skill, save_artifact,
    scaffold_skill,
};
