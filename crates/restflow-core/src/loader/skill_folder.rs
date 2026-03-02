use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use tracing::warn;
use walkdir::WalkDir;

use crate::models::{Skill, SkillReference, SkillScript, StorageMode};
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
            Self::validate_declared_scripts(folder_path, &skill.scripts)?;
            self.fill_script_langs(folder_path, &mut skill.scripts);
        }

        if skill.references.is_empty() {
            skill.references = self.discover_references(folder_path)?;
        } else {
            Self::validate_declared_references(folder_path, &skill.references)?;
            self.fill_reference_metadata(folder_path, &mut skill.references);
        }

        Self::validate_unique_assets(
            &skill.scripts,
            "script",
            |script| &script.id,
            |script| &script.path,
        )?;
        Self::validate_unique_assets(
            &skill.references,
            "reference",
            |reference| &reference.id,
            |reference| &reference.path,
        )?;

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

    pub fn discover_references(&self, folder_path: &Path) -> Result<Vec<SkillReference>> {
        let references_dir = folder_path.join("references");
        if !references_dir.exists() {
            return Ok(Vec::new());
        }

        let mut references = Vec::new();
        for entry in WalkDir::new(&references_dir)
            .min_depth(1)
            .follow_links(false)
        {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();
            let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
            if !matches!(extension.to_ascii_lowercase().as_str(), "md" | "markdown") {
                continue;
            }

            let relative_path = path
                .strip_prefix(folder_path)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            let id = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("reference")
                .to_string();
            let (title, summary) = std::fs::read_to_string(path)
                .ok()
                .map(|content| Self::extract_reference_metadata(&content))
                .unwrap_or((None, None));

            references.push(SkillReference {
                id,
                path: relative_path,
                title,
                summary,
            });
        }

        references.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(references)
    }

    fn fill_reference_metadata(&self, folder_path: &Path, references: &mut [SkillReference]) {
        for reference in references {
            if reference.title.is_some() && reference.summary.is_some() {
                continue;
            }

            let path = Path::new(&reference.path);
            let candidate_path = if path.is_absolute() {
                path.to_path_buf()
            } else {
                folder_path.join(path)
            };

            let Ok(content) = std::fs::read_to_string(candidate_path) else {
                continue;
            };
            let (title, summary) = Self::extract_reference_metadata(&content);
            if reference.title.is_none() {
                reference.title = title;
            }
            if reference.summary.is_none() {
                reference.summary = summary;
            }
        }
    }

    fn extract_reference_metadata(content: &str) -> (Option<String>, Option<String>) {
        let normalized = content.replace("\r\n", "\n");
        let mut lines = normalized.lines();

        if normalized.starts_with("---\n") {
            let _ = lines.next();
            for line in &mut lines {
                if line.trim() == "---" {
                    break;
                }
            }
        }

        let mut title = None;
        let mut summary = None;
        for line in lines {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if title.is_none()
                && let Some(rest) = trimmed.strip_prefix("# ")
            {
                title = Some(rest.trim().to_string());
                continue;
            }

            if !trimmed.starts_with('#') {
                summary = Some(trimmed.to_string());
                break;
            }
        }

        (title, summary)
    }

    fn validate_declared_scripts(folder_path: &Path, scripts: &[SkillScript]) -> Result<()> {
        for script in scripts {
            let candidate = Self::resolve_declared_asset_path(folder_path, &script.path)
                .with_context(|| format!("Invalid script path for id '{}'", script.id))?;
            let metadata = std::fs::metadata(&candidate)
                .with_context(|| format!("Failed to read script metadata at {:?}", candidate))?;
            if !metadata.is_file() {
                return Err(anyhow::anyhow!(
                    "Declared script '{}' path is not a file: {}",
                    script.id,
                    script.path
                ));
            }
        }
        Ok(())
    }

    fn validate_declared_references(
        folder_path: &Path,
        references: &[SkillReference],
    ) -> Result<()> {
        for reference in references {
            if !Self::is_markdown_path(&reference.path) {
                return Err(anyhow::anyhow!(
                    "Declared reference '{}' must use markdown extension (.md or .markdown): {}",
                    reference.id,
                    reference.path
                ));
            }

            let candidate = Self::resolve_declared_asset_path(folder_path, &reference.path)
                .with_context(|| format!("Invalid reference path for id '{}'", reference.id))?;
            let metadata = std::fs::metadata(&candidate)
                .with_context(|| format!("Failed to read reference metadata at {:?}", candidate))?;
            if !metadata.is_file() {
                return Err(anyhow::anyhow!(
                    "Declared reference '{}' path is not a file: {}",
                    reference.id,
                    reference.path
                ));
            }
        }
        Ok(())
    }

    fn resolve_declared_asset_path(folder_path: &Path, raw_path: &str) -> Result<PathBuf> {
        let declared = Path::new(raw_path);
        if declared.is_absolute() {
            return Err(anyhow::anyhow!(
                "Path must be relative, got absolute path: {raw_path}"
            ));
        }
        if declared.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            return Err(anyhow::anyhow!(
                "Path must not contain traversal or root components: {raw_path}"
            ));
        }

        let candidate = folder_path.join(declared);
        if !candidate.exists() {
            return Err(anyhow::anyhow!(
                "Declared path does not exist inside skill folder: {raw_path}"
            ));
        }

        let canonical_folder = std::fs::canonicalize(folder_path)
            .with_context(|| format!("Failed to canonicalize skill folder {:?}", folder_path))?;
        let canonical_candidate = std::fs::canonicalize(&candidate)
            .with_context(|| format!("Failed to canonicalize declared path {:?}", candidate))?;
        if !canonical_candidate.starts_with(&canonical_folder) {
            return Err(anyhow::anyhow!(
                "Declared path resolves outside of skill folder: {raw_path}"
            ));
        }

        Ok(candidate)
    }

    fn is_markdown_path(path: &str) -> bool {
        let extension = Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");
        matches!(extension.to_ascii_lowercase().as_str(), "md" | "markdown")
    }

    fn validate_unique_assets<T, FId, FPath>(
        assets: &[T],
        kind: &str,
        id_fn: FId,
        path_fn: FPath,
    ) -> Result<()>
    where
        FId: Fn(&T) -> &str,
        FPath: Fn(&T) -> &str,
    {
        let mut ids = HashSet::new();
        let mut paths = HashSet::new();

        for asset in assets {
            let id = id_fn(asset);
            if !ids.insert(id.to_string()) {
                return Err(anyhow::anyhow!(
                    "Duplicate {kind} id found in skill frontmatter/discovery: {id}"
                ));
            }

            let path = path_fn(asset);
            if !paths.insert(path.to_string()) {
                return Err(anyhow::anyhow!(
                    "Duplicate {kind} path found in skill frontmatter/discovery: {path}"
                ));
            }
        }

        Ok(())
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

    #[test]
    fn test_load_skill_folder_discovers_references() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("SKILL.md"),
            "---\nname: Root\n---\n\n# Root skill",
        )
        .unwrap();
        let references_dir = temp.path().join("references");
        std::fs::create_dir_all(&references_dir).unwrap();
        std::fs::write(
            references_dir.join("api.md"),
            "# API Guide\n\nUse this guide to call the API safely.",
        )
        .unwrap();

        let loader = SkillFolderLoader::new(temp.path());
        let skill = loader.load_skill_folder(temp.path()).unwrap();

        assert_eq!(skill.references.len(), 1);
        let reference = &skill.references[0];
        assert_eq!(reference.id, "api");
        assert_eq!(reference.path, "references/api.md");
        assert_eq!(reference.title.as_deref(), Some("API Guide"));
        assert_eq!(
            reference.summary.as_deref(),
            Some("Use this guide to call the API safely.")
        );
    }

    #[test]
    fn test_load_skill_folder_rejects_declared_absolute_script_path() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("SKILL.md"),
            "---\nname: Invalid Script\nscripts:\n  - id: run\n    path: /tmp/run.sh\n---\n\n# Invalid",
        )
        .unwrap();

        let loader = SkillFolderLoader::new(temp.path());
        let err = loader.load_skill_folder(temp.path()).unwrap_err();
        let err = format!("{err:#}");
        assert!(err.contains("Path must be relative"));
    }

    #[test]
    fn test_load_skill_folder_rejects_declared_reference_traversal_and_non_markdown() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("SKILL.md"),
            "---\nname: Invalid Reference\nreferences:\n  - id: bad-traversal\n    path: ../outside.md\n---\n\n# Invalid",
        )
        .unwrap();

        let loader = SkillFolderLoader::new(temp.path());
        let err = loader.load_skill_folder(temp.path()).unwrap_err();
        let err = format!("{err:#}");
        assert!(err.contains("must not contain traversal"));

        std::fs::write(
            temp.path().join("SKILL.md"),
            "---\nname: Invalid Reference Type\nreferences:\n  - id: bad-ext\n    path: references/readme.txt\n---\n\n# Invalid",
        )
        .unwrap();
        let references_dir = temp.path().join("references");
        std::fs::create_dir_all(&references_dir).unwrap();
        std::fs::write(references_dir.join("readme.txt"), "text").unwrap();

        let err = loader.load_skill_folder(temp.path()).unwrap_err();
        let err = format!("{err:#}");
        assert!(err.contains("must use markdown extension"));
    }

    #[test]
    fn test_load_skill_folder_rejects_declared_missing_or_outside_reference() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("SKILL.md"),
            "---\nname: Missing Reference\nreferences:\n  - id: missing\n    path: references/missing.md\n---\n\n# Invalid",
        )
        .unwrap();

        let loader = SkillFolderLoader::new(temp.path());
        let err = loader.load_skill_folder(temp.path()).unwrap_err();
        let err = format!("{err:#}");
        assert!(err.contains("does not exist inside skill folder"));
    }

    #[test]
    fn test_load_skill_folder_rejects_duplicate_script_or_reference_id_or_path() {
        let temp = tempfile::tempdir().unwrap();
        let scripts_dir = temp.path().join("scripts");
        let references_dir = temp.path().join("references");
        std::fs::create_dir_all(&scripts_dir).unwrap();
        std::fs::create_dir_all(&references_dir).unwrap();
        std::fs::write(scripts_dir.join("a.sh"), "#!/bin/sh\necho a").unwrap();
        std::fs::write(scripts_dir.join("b.sh"), "#!/bin/sh\necho b").unwrap();
        std::fs::write(references_dir.join("a.md"), "# A").unwrap();
        std::fs::write(references_dir.join("b.md"), "# B").unwrap();

        std::fs::write(
            temp.path().join("SKILL.md"),
            "---\nname: Duplicate Script\nscripts:\n  - id: run\n    path: scripts/a.sh\n  - id: run\n    path: scripts/b.sh\n---\n\n# Invalid",
        )
        .unwrap();
        let loader = SkillFolderLoader::new(temp.path());
        let err = loader
            .load_skill_folder(temp.path())
            .unwrap_err()
            .to_string();
        assert!(err.contains("Duplicate script id"));

        std::fs::write(
            temp.path().join("SKILL.md"),
            "---\nname: Duplicate Reference Path\nreferences:\n  - id: ref-a\n    path: references/a.md\n  - id: ref-b\n    path: references/a.md\n---\n\n# Invalid",
        )
        .unwrap();
        let err = loader
            .load_skill_folder(temp.path())
            .unwrap_err()
            .to_string();
        assert!(err.contains("Duplicate reference path"));
    }

    #[test]
    fn test_scan_skips_skill_with_declared_path_validation_failure() {
        let temp = tempfile::tempdir().unwrap();
        let valid = temp.path().join("valid");
        let invalid = temp.path().join("invalid");
        std::fs::create_dir_all(&valid).unwrap();
        std::fs::create_dir_all(&invalid).unwrap();

        std::fs::write(valid.join("SKILL.md"), "---\nname: Valid\n---\n\n# Valid").unwrap();
        std::fs::write(
            invalid.join("SKILL.md"),
            "---\nname: Invalid\nreferences:\n  - id: bad\n    path: ../outside.md\n---\n\n# Invalid",
        )
        .unwrap();

        let loader = SkillFolderLoader::new(temp.path());
        let (skills, failed) = loader.scan().unwrap();
        assert_eq!(failed, 1);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "valid");
    }
}
