use std::path::PathBuf;

use anyhow::{Context, Result};
use tempfile::TempDir;

use crate::loader::skill_folder::discover_skill_dirs;

pub struct GitSkillSource;

impl GitSkillSource {
    pub async fn clone_and_discover(
        url: &str,
        subpath: Option<&str>,
    ) -> Result<(TempDir, Vec<PathBuf>)> {
        let temp = TempDir::new()?;
        let temp_path = temp.path();

        let status = tokio::process::Command::new("git")
            .args(["clone", "--depth", "1", url, temp_path.to_str().unwrap()])
            .status()
            .await
            .with_context(|| format!("Failed to clone git repo: {}", url))?;

        if !status.success() {
            return Err(anyhow::anyhow!("Failed to clone: {}", url));
        }

        let search_dir = match subpath {
            Some(path) => temp_path.join(path),
            None => temp_path.to_path_buf(),
        };

        if !search_dir.exists() {
            return Err(anyhow::anyhow!(
                "Subpath does not exist in repository: {}",
                search_dir.display()
            ));
        }

        let skill_dirs = discover_skill_dirs(&search_dir)?;
        Ok((temp, skill_dirs))
    }
}
