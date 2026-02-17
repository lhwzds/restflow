//! Pre-commit hook installation for background agents.
//!
//! This module provides functions to install the pre-commit hook into git repositories
//! to prevent background agents from committing directly to protected branches.
//!
//! Related to PR #617: Worktree enforcement for CLI agents

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Install the pre-commit hook to a git repository.
///
/// This function copies the pre-commit hook from assets into the target
/// repository's `.git/hooks/` directory and sets executable permissions.
///
/// The hook prevents background agents (identified by `RESTFLOW_AGENT=1`)
/// from committing directly to protected branches like `main` or `master`.
pub fn install_pre_commit_hook(repo_path: &Path) -> Result<()> {
    let hook_target = repo_path.join(".git/hooks/pre-commit");
    
    // Read the hook content from the embedded asset
    let hook_content = include_str!("../../../assets/hooks/pre-commit");
    
    // Check if hook already exists with same content (skip if already installed)
    if hook_target.exists() {
        let existing = fs::read_to_string(&hook_target)
            .context("Failed to read existing hook")?;
        if existing.contains("RestFlow: Prevent background agents") {
            tracing::debug!("Pre-commit hook already installed at {:?}", hook_target);
            return Ok(());
        }
        // Backup existing hook
        let backup_path = repo_path.join(".git/hooks/pre-commit.backup");
        fs::copy(&hook_target, &backup_path)
            .context("Failed to backup existing hook")?;
        tracing::info!("Backed up existing hook to {:?}", backup_path);
    }
    
    // Ensure hooks directory exists
    if let Some(parent) = hook_target.parent() {
        fs::create_dir_all(parent)
            .context("Failed to create hooks directory")?;
    }
    
    // Write the hook file
    fs::write(&hook_target, hook_content)
        .with_context(|| format!("Failed to write hook to {:?}", hook_target))?;
    
    // Make it executable (Unix only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&hook_target)
            .context("Failed to get hook permissions")?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&hook_target, perms)
            .context("Failed to set hook executable permissions")?;
    }
    
    tracing::info!("Installed pre-commit hook to {:?}", hook_target);
    Ok(())
}

/// Check if the pre-commit hook is installed in a repository.
pub fn is_hook_installed(repo_path: &Path) -> bool {
    let hook_path = repo_path.join(".git/hooks/pre-commit");
    if !hook_path.exists() {
        return false;
    }
    
    // Check if it's our hook
    if let Ok(content) = fs::read_to_string(&hook_path) {
        content.contains("RestFlow: Prevent background agents")
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    fn create_test_git_repo(temp_dir: &TempDir) -> std::path::PathBuf {
        let git_dir = temp_dir.path().join(".git");
        fs::create_dir_all(git_dir.join("hooks")).unwrap();
        // Create minimal HEAD ref
        fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        temp_dir.path().to_path_buf()
    }
    
    #[test]
    fn test_install_pre_commit_hook() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = create_test_git_repo(&temp_dir);
        
        let result = install_pre_commit_hook(&repo_path);
        assert!(result.is_ok(), "Hook installation should succeed: {:?}", result);
        
        let hook_path = repo_path.join(".git/hooks/pre-commit");
        assert!(hook_path.exists(), "Hook file should exist");
        
        let content = fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains("RESTFLOW_AGENT"));
        assert!(content.contains("#!/bin/bash"));
    }
    
    #[test]
    #[cfg(unix)]
    fn test_hook_is_executable() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = create_test_git_repo(&temp_dir);
        
        install_pre_commit_hook(&repo_path).unwrap();
        
        let hook_path = repo_path.join(".git/hooks/pre-commit");
        let metadata = fs::metadata(&hook_path).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o755, "Hook should be executable (755)");
    }
    
    #[test]
    fn test_install_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = create_test_git_repo(&temp_dir);
        
        // Install twice
        install_pre_commit_hook(&repo_path).unwrap();
        let result = install_pre_commit_hook(&repo_path);
        
        assert!(result.is_ok(), "Second install should succeed (idempotent)");
    }
    
    #[test]
    fn test_is_hook_installed() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = create_test_git_repo(&temp_dir);
        
        assert!(!is_hook_installed(&repo_path), "Hook should not be installed initially");
        
        install_pre_commit_hook(&repo_path).unwrap();
        
        assert!(is_hook_installed(&repo_path), "Hook should be installed after install_pre_commit_hook");
    }
    
    #[test]
    fn test_hook_blocks_main_branch() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = create_test_git_repo(&temp_dir);
        
        install_pre_commit_hook(&repo_path).unwrap();
        
        let hook_path = repo_path.join(".git/hooks/pre-commit");
        let content = fs::read_to_string(&hook_path).unwrap();
        
        // Verify the hook logic checks for RESTFLOW_AGENT and blocks main/master
        assert!(content.contains("RESTFLOW_AGENT"));
        assert!(content.contains("main"));
        assert!(content.contains("master"));
        assert!(content.contains("exit 1"));
    }
}
