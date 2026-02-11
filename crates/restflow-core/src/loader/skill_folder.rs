use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use tracing::warn;
use walkdir::WalkDir;

use crate::models::{Skill, SkillScript, StorageMode};
use crate::paths;

#[derive(Debug, Clone)]
pub struct SkillFolderLoader {
    base_dir: PathBuf,
}

pub fn discover_skill_dirs(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    if !dir.exists() {
        return Ok(found);
    }

    let root_skill = dir.join("SKILL.md");
    if root_skill.exists() {
        found.push(dir.to_path_buf());
    }

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let file_type = entry.file_type().ok();
            if !matches!(file_type, Some(t) if t.is_dir()) {
                continue;
            }
            let path = entry.path();
            if path.join("SKILL.md").exists() {
                found.push(path);
            }
        }
    }

    found.sort();
    Ok(found)
}

impl SkillFolderLoader {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    pub fn scan(&self) -> Result<(Vec<Skill>, usize)> {
        let mut skills = Vec::new();
        let mut failed = 0usize;
        if !self.base_dir.exists() {
            return Ok((skills, failed));
        }

        for entry in WalkDir::new(&self.base_dir)
            .min_depth(1)
            .max_depth(2)
            .follow_links(false)
        {
            let entry = entry?;
            if !entry.file_type().is_dir() {
                continue;
            }

            let folder_path = entry.path();
            let skill_path = folder_path.join("SKILL.md");
            if !skill_path.exists() {
                continue;
            }

            match self
                .load_skill_folder(folder_path)
                .with_context(|| format!("Failed to load skill folder at {:?}", folder_path))
            {
                Ok(skill) => skills.push(skill),
                Err(err) => {
                    failed += 1;
                    warn!(
                        path = ?folder_path,
                        error = %err,
                        "Skipping invalid skill folder"
                    );
                }
            }
        }

        Ok((skills, failed))
    }

    pub fn scan_all() -> Result<Vec<Skill>> {
        let mut skills = Vec::new();

        if let Ok(user_dir) = paths::user_skills_dir() {
            let loader = SkillFolderLoader::new(user_dir);
            let (loaded, _) = loader.scan()?;
            skills.extend(loaded);
        }

        Ok(skills)
    }

    pub fn load_skill_folder(&self, folder_path: &Path) -> Result<Skill> {
        let skill_id = folder_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid skill folder name"))?
            .to_string();

        let skill_path = folder_path.join("SKILL.md");
        let content = std::fs::read_to_string(&skill_path)
            .with_context(|| format!("Failed to read skill file at {:?}", skill_path))?;

        let mut skill = Skill::from_markdown(&skill_id, &content)?;
        skill.folder_path = Some(folder_path.to_string_lossy().to_string());
        skill.storage_mode = StorageMode::FileSystemOnly;
        skill.is_synced = true;
        skill.content_hash = Some(Self::hash_content(&content));

        if skill.scripts.is_empty() {
            skill.scripts = self.discover_scripts(folder_path)?;
        } else {
            self.fill_script_langs(folder_path, &mut skill.scripts);
        }

        Ok(skill)
    }

    pub fn discover_scripts(&self, folder_path: &Path) -> Result<Vec<SkillScript>> {
        let scripts_dir = folder_path.join("scripts");
        if !scripts_dir.exists() {
            return Ok(Vec::new());
        }

        let mut scripts = Vec::new();
        for entry in WalkDir::new(&scripts_dir).min_depth(1).follow_links(false) {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();
            let relative_path = path
                .strip_prefix(folder_path)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            let id = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("script")
                .to_string();
            let lang = Self::detect_lang(path);

            scripts.push(SkillScript {
                id,
                path: relative_path,
                lang,
            });
        }

        scripts.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(scripts)
    }

    pub fn detect_lang(path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?.to_lowercase();
        let lang = match ext.as_str() {
            "sh" | "bash" => "bash",
            "py" => "python",
            "lua" => "lua",
            "js" => "javascript",
            "ts" => "typescript",
            "rb" => "ruby",
            "ps1" => "powershell",
            _ => return None,
        };
        Some(lang.to_string())
    }

    fn hash_content(content: &str) -> String {
        hex::encode(Sha256::digest(content.as_bytes()))
    }

    fn fill_script_langs(&self, folder_path: &Path, scripts: &mut [SkillScript]) {
        for script in scripts {
            if script.lang.is_some() {
                continue;
            }

            let candidate_path = folder_path.join(&script.path);
            script.lang = Self::detect_lang(&candidate_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SkillFolderLoader, discover_skill_dirs};

    #[test]
    fn test_discover_skill_dirs_root() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("SKILL.md"),
            "---\nname: Root\n---\n\n# Root",
        )
        .unwrap();

        let found = discover_skill_dirs(temp.path()).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], temp.path());
    }

    #[test]
    fn test_discover_skill_dirs_subdirs() {
        let temp = tempfile::tempdir().unwrap();
        let skill_dir = temp.path().join("skill-a");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "---\nname: Sub\n---\n\n# Sub").unwrap();

        let found = discover_skill_dirs(temp.path()).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], skill_dir);
    }

    #[test]
    fn test_scan_empty_folder_returns_no_skills_or_failures() {
        let temp = tempfile::tempdir().unwrap();
        let loader = SkillFolderLoader::new(temp.path());

        let (skills, failed) = loader.scan().unwrap();
        assert!(skills.is_empty());
        assert_eq!(failed, 0);
    }

    #[test]
    fn test_scan_skips_invalid_skill_and_continues_loading_valid_skills() {
        let temp = tempfile::tempdir().unwrap();
        let valid_a = temp.path().join("valid-a");
        let invalid = temp.path().join("invalid");
        let valid_b = temp.path().join("valid-b");

        std::fs::create_dir_all(&valid_a).unwrap();
        std::fs::create_dir_all(&invalid).unwrap();
        std::fs::create_dir_all(&valid_b).unwrap();

        std::fs::write(valid_a.join("SKILL.md"), "---\nname: Valid A\n---\n\n# A").unwrap();
        std::fs::write(invalid.join("SKILL.md"), "this is not valid frontmatter").unwrap();
        std::fs::write(valid_b.join("SKILL.md"), "---\nname: Valid B\n---\n\n# B").unwrap();

        let loader = SkillFolderLoader::new(temp.path());
        let (skills, failed) = loader.scan().unwrap();

        let mut ids: Vec<String> = skills.into_iter().map(|skill| skill.id).collect();
        ids.sort();

        assert_eq!(failed, 1);
        assert_eq!(ids, vec!["valid-a".to_string(), "valid-b".to_string()]);
    }
}
