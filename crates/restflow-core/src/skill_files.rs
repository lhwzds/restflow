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
    (
        "pr-shared-space-validation",
        include_str!("../assets/skills/pr-shared-space-validation/SKILL.md"),
    ),
    (
        "pr-submit-from-shared-space",
        include_str!("../assets/skills/pr-submit-from-shared-space/SKILL.md"),
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
    use regex::Regex;
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
    fn creates_new_kv_store_pr_skills() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var(RESTFLOW_DIR_ENV, temp.path()) };

        ensure_default_skill_files().unwrap();

        let validation_path = temp
            .path()
            .join("skills")
            .join("pr-shared-space-validation")
            .join(SKILL_FILE_NAME);
        let submit_path = temp
            .path()
            .join("skills")
            .join("pr-submit-from-shared-space")
            .join(SKILL_FILE_NAME);
        assert!(validation_path.exists());
        assert!(submit_path.exists());
        assert!(
            std::fs::read_to_string(submit_path)
                .unwrap()
                .contains("--body-file")
        );

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
            assert!(
                content.starts_with("---"),
                "skill {} missing frontmatter",
                skill_id
            );
            assert!(content.contains("name:"), "skill {} missing name", skill_id);
        }
    }

    #[test]
    fn validates_kv_store_pr_key_schema() {
        fn is_valid_key(task_id: &str, key: &str) -> bool {
            let prefix = format!("pr:{task_id}:");
            key.starts_with(&prefix)
        }

        assert!(is_valid_key("task-123", "pr:task-123:title"));
        assert!(is_valid_key("task-123", "pr:task-123:body"));
        assert!(!is_valid_key("task-123", "pr:task-456:title"));
        assert!(!is_valid_key("task-123", "workspace:task-123:title"));
    }

    #[test]
    fn rejects_submission_when_title_or_body_is_missing() {
        fn can_submit(title: Option<&str>, body: Option<&str>) -> bool {
            let title_ok = title.map(str::trim).is_some_and(|s| !s.is_empty());
            let body_ok = body.map(str::trim).is_some_and(|s| !s.is_empty());
            title_ok && body_ok
        }

        assert!(can_submit(Some("feat: demo"), Some("body")));
        assert!(!can_submit(None, Some("body")));
        assert!(!can_submit(Some("feat: demo"), None));
        assert!(!can_submit(Some(""), Some("body")));
        assert!(!can_submit(Some("feat: demo"), Some("")));
    }

    #[test]
    fn command_builder_uses_body_file_not_inline_body() {
        fn build_command(
            title_file: &str,
            body_file: &str,
            base: Option<&str>,
            head: Option<&str>,
        ) -> Vec<String> {
            let mut args = vec![
                "gh".to_string(),
                "pr".to_string(),
                "create".to_string(),
                "--title".to_string(),
                format!("$(cat {title_file})"),
                "--body-file".to_string(),
                body_file.to_string(),
            ];
            if let Some(base) = base {
                args.push("--base".to_string());
                args.push(base.to_string());
            }
            if let Some(head) = head {
                args.push("--head".to_string());
                args.push(head.to_string());
            }
            args
        }

        let args = build_command(
            "/tmp/pr-title.txt",
            "/tmp/pr-body.md",
            Some("main"),
            Some("feature/branch"),
        );
        let joined = args.join(" ");
        assert!(joined.contains("--body-file /tmp/pr-body.md"));
        assert!(!joined.contains("--body "));
    }

    #[test]
    fn detects_secret_like_patterns_in_pr_content() {
        fn has_secret_like_content(text: &str) -> bool {
            let patterns = [
                Regex::new(r"AKIA[0-9A-Z]{16}").unwrap(),
                Regex::new(r"ghp_[A-Za-z0-9]{36,}").unwrap(),
                Regex::new(r"sk-[A-Za-z0-9]{20,}").unwrap(),
                Regex::new(r"-----BEGIN (RSA|EC|OPENSSH|PGP) PRIVATE KEY-----").unwrap(),
            ];
            patterns.iter().any(|pattern| pattern.is_match(text))
        }

        assert!(!has_secret_like_content("normal release notes"));
        assert!(has_secret_like_content(
            "token=ghp_abcdefghijklmnopqrstuvwxyz1234567890"
        ));
    }
}
