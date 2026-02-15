use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tempfile::TempDir;

use crate::loader::skill_folder::discover_skill_dirs;

/// Import a .skill ZIP package and discover skills inside it.
pub struct SkillPackageImporter;

impl SkillPackageImporter {
    pub fn import(package_path: &Path) -> Result<(TempDir, Vec<PathBuf>)> {
        let temp = TempDir::new()?;

        let file = std::fs::File::open(package_path)
            .with_context(|| format!("Failed to open package: {}", package_path.display()))?;
        let mut archive = zip::ZipArchive::new(file)
            .with_context(|| format!("Failed to read zip archive: {}", package_path.display()))?;

        archive
            .extract(temp.path())
            .with_context(|| format!("Failed to extract package: {}", package_path.display()))?;

        let skill_dirs = discover_skill_dirs(temp.path())?;
        Ok((temp, skill_dirs))
    }
}
