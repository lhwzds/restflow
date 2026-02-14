use anyhow::{Context, Result};
use std::path::Path;

use crate::paths;

const SKILL_FILE_NAME: &str = "SKILL.md";
const DEFAULT_SKILLS: &[(&str, &str)] = &[
    (
        "self-heal-ops",
        include_str!("../assets/skills/self-heal-ops/SKILL.md"),
    ),
    (
        "structured-planner",
        include_str!("../assets/skills/structured-planner/SKILL.md"),
    ),
    (
        "address-pr-feedback",
        include_str!("../assets/skills/address-pr-feedback/SKILL.md"),
    ),
    (
        "pr-context-gatherer",
        include_str!("../assets/skills/pr-context-gatherer/SKILL.md"),
    ),
];

/// Ensure default skill files exist under ~/.restflow/skills/.
/// Existing user-edited files are preserved and never overwritten.
pub fn ensure_default_skill_files() -> Result<()> {
    let skills_root = paths::user_skills_dir()?;

    for (skill_id, content) in DEFAULT_SKILLS {
        ensure_skill_file(&skills_root, skill_id, content)?;
    }

    Ok(())
}

fn ensure_skill_file(skills_root: &Path, skill_id: &str, content: &str) -> Result<()> {
    let skill_dir = skills_root.join(skill_id);
    std::fs::create_dir_all(&skill_dir).with_context(|| {
        format!(
            "Failed to create default skill directory: {}",
            skill_dir.display()
        )
    })?;

    let skill_file = skill_dir.join(SKILL_FILE_NAME);
    if skill_file.exists() {
        return Ok(());
    }

    std::fs::write(&skill_file, content).with_context(|| {
        format!(
            "Failed to write default skill file: {}",
            skill_file.display()
        )
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    const RESTFLOW_DIR_ENV: &str = "RESTFLOW_DIR";

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    #[test]
    fn creates_default_skill_file_when_missing() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var(RESTFLOW_DIR_ENV, temp.path()) };

        ensure_default_skill_files().unwrap();

        for (skill_id, _) in DEFAULT_SKILLS {
            let path = temp
                .path()
                .join("skills")
                .join(skill_id)
                .join(SKILL_FILE_NAME);
            assert!(path.exists(), "skill {} should exist", skill_id);
        }

        unsafe { std::env::remove_var(RESTFLOW_DIR_ENV) };
    }

    #[test]
    fn does_not_overwrite_existing_skill_file() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var(RESTFLOW_DIR_ENV, temp.path()) };

        let skill_dir = temp.path().join("skills").join("self-heal-ops");
        std::fs::create_dir_all(&skill_dir).unwrap();
        let path = skill_dir.join(SKILL_FILE_NAME);
        std::fs::write(&path, "custom-user-content").unwrap();

        ensure_default_skill_files().unwrap();

        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "custom-user-content"
        );

        unsafe { std::env::remove_var(RESTFLOW_DIR_ENV) };
    }

    #[test]
    fn default_skill_content_is_valid_frontmatter() {
        for (skill_id, content) in DEFAULT_SKILLS {
            assert!(content.starts_with("---"), "skill {} missing frontmatter", skill_id);
            assert!(content.contains("name:"), "skill {} missing name", skill_id);
        }
    }
}
