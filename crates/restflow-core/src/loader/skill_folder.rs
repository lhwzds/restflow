use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use walkdir::WalkDir;

use crate::models::{Skill, SkillScript};

#[derive(Debug, Clone)]
pub struct SkillFolderLoader {
    base_dir: PathBuf,
}

impl SkillFolderLoader {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    pub fn scan(&self) -> Result<Vec<Skill>> {
        let mut skills = Vec::new();
        if !self.base_dir.exists() {
            return Ok(skills);
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

            let skill = self
                .load_skill_folder(folder_path)
                .with_context(|| format!("Failed to load skill folder at {:?}", folder_path))?;
            skills.push(skill);
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
