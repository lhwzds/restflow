use std::path::PathBuf;

use anyhow::{Context, Result};
use tempfile::TempDir;
use tokio::process::Command;

use crate::loader::skill_folder::discover_skill_dirs;

/// Clone a Git repository and discover skills within it.
pub struct GitSkillSource;

impl GitSkillSource {
    pub async fn clone_and_discover(
        url: &str,
        subpath: Option<&str>,
    ) -> Result<(TempDir, Vec<PathBuf>)> {
        let temp = TempDir::new()?;
        let temp_path = temp.path();
        let target = temp_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Failed to resolve temp directory path"))?;

        let status = Command::new("git")
            .args(["clone", "--depth", "1", url, target])
            .status()
            .await
            .with_context(|| format!("Failed to run git clone for {}", url))?;

        if !status.success() {
            return Err(anyhow::anyhow!("Failed to clone: {}", url));
        }

        let search_dir = match subpath {
            Some(path) => temp_path.join(path),
            None => temp_path.to_path_buf(),
        };

        if !search_dir.exists() {
            return Err(anyhow::anyhow!(
                "Subpath not found in repository: {}",
                search_dir.display()
            ));
        }

        let skill_dirs = discover_skill_dirs(&search_dir)?;
        Ok((temp, skill_dirs))
    }
}
