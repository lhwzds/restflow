use std::path::{Path, PathBuf};

use anyhow::Result;
use tempfile::TempDir;

use crate::loader::skill_folder::discover_skill_dirs;

pub struct SkillPackageImporter;

impl SkillPackageImporter {
    pub fn import(package_path: &Path) -> Result<(TempDir, Vec<PathBuf>)> {
        let temp = TempDir::new()?;

        let file = std::fs::File::open(package_path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        archive.extract(temp.path())?;

        let skill_dirs = discover_skill_dirs(temp.path())?;
        Ok((temp, skill_dirs))
    }
}
