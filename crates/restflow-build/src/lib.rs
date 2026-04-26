pub mod artifact;
pub mod build;
pub mod run;
pub mod scaffold;
pub mod toolchain;

pub use artifact::{
    ArtifactKind, BuildProfile, SecretMode, SkillArtifactDownload, SkillArtifactMetadata,
    SkillArtifactProtocol, list_installed_skill_artifacts, read_skill_artifact_metadata,
    resolve_skill_binary_entry_path, write_skill_artifact_metadata,
};
pub use build::{BuildBinaryOptions, BuildBinaryResult, build_skill_binary};
pub use run::{RunBinaryOptions, RunBinaryResult, run_skill_binary};
pub use scaffold::{
    CreateSkillProjectOptions, CreateSkillProjectResult, ReadSkillProjectResult,
    UpdateSkillProjectOptions, create_skill_project, read_skill_project, skill_root_dir,
    update_skill_project,
};
pub use toolchain::{DEFAULT_TOOLCHAIN_ID, ToolchainInvocation, ensure_toolchain};

#[cfg(test)]
pub(crate) fn test_env_lock() -> std::sync::MutexGuard<'static, ()> {
    use std::sync::{Mutex, OnceLock};

    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
