//! Shared path resolution and normalization utilities for file-based tools.
//!
//! This module centralizes `normalize_path` and `resolve_path` so that every
//! file-oriented tool (`FileTool`, `EditTool`, `MultiEditTool`, `PatchTool`)
//! uses the same logic for base-directory enforcement and symlink-safe
//! canonicalization.

use std::path::{Path, PathBuf};

/// Normalize a path without canonicalizing (for non-existent paths).
///
/// Resolves `.` and `..` components purely lexically, which is useful when
/// the path (or parts of it) does not yet exist on disk.
pub(crate) fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                result.pop();
            }
            std::path::Component::CurDir => {}
            c => result.push(c),
        }
    }
    result
}

/// Resolve and validate a path against an optional base directory.
///
/// When `base_dir` is `Some`, the resolved path is checked to ensure it does
/// not escape the base directory.  For paths that already exist on disk the
/// check uses `canonicalize`; for paths that do not yet exist it falls back to
/// lexical normalization (via [`normalize_path`]) combined with ancestor
/// canonicalization when possible.
///
/// When `base_dir` is `None`, absolute paths are accepted as-is and relative
/// paths are rejected. Callers that require a workspace root can use
/// [`resolve_path_with_policy`] with `require_base_dir = true`.
#[cfg(test)]
pub(crate) fn resolve_path(path: &str, base_dir: Option<&Path>) -> Result<PathBuf, String> {
    resolve_path_with_policy(path, base_dir, false)
}

pub(crate) fn resolve_path_with_policy(
    path: &str,
    base_dir: Option<&Path>,
    require_base_dir: bool,
) -> Result<PathBuf, String> {
    let path = PathBuf::from(path);

    if let Some(base) = base_dir {
        let resolved = if path.is_absolute() {
            path
        } else {
            base.join(&path)
        };

        // Compute canonical base early so every branch shares it.
        let canonical_base = if base.exists() {
            base.canonicalize().map_err(|e| e.to_string())?
        } else {
            normalize_path(base)
        };

        if resolved.exists() {
            let canonical = resolved.canonicalize().map_err(|e| e.to_string())?;
            if !canonical.starts_with(&canonical_base) {
                return Err(format!(
                    "Path '{}' escapes allowed base directory '{}'. All file operations must be within this directory.",
                    canonical.display(),
                    canonical_base.display()
                ));
            }
            return Ok(canonical);
        }

        // The resolved path does not exist yet.  If the base itself exists we
        // try to find a real ancestor so that symlinks in existing prefixes are
        // resolved correctly.
        if base.exists() {
            let Some((ancestor, suffix)) = find_existing_ancestor(&resolved) else {
                return Err(format!(
                    "Path '{}' escapes allowed base directory '{}'. All file operations must be within this directory.",
                    resolved.display(),
                    canonical_base.display()
                ));
            };
            let canonical_parent = ancestor.canonicalize().map_err(|e| e.to_string())?;
            let candidate = normalize_path(&canonical_parent.join(suffix));
            if !candidate.starts_with(&canonical_base) {
                return Err(format!(
                    "Path '{}' escapes allowed base directory '{}'. All file operations must be within this directory.",
                    candidate.display(),
                    canonical_base.display()
                ));
            }
            return Ok(candidate);
        }

        let normalized = normalize_path(&resolved);
        if !normalized.starts_with(&canonical_base) {
            return Err(format!(
                "Path '{}' escapes allowed base directory '{}'. All file operations must be within this directory.",
                normalized.display(),
                canonical_base.display()
            ));
        }

        Ok(normalized)
    } else {
        if require_base_dir {
            return Err(
                "This tool requires an explicit workspace root or base directory.".to_string(),
            );
        }

        // No base directory restriction.
        if path.is_absolute() {
            Ok(path)
        } else {
            Err("Relative paths require an explicit workspace root or base directory.".to_string())
        }
    }
}

/// Walk up from `path` until we find an existing ancestor directory.
///
/// Returns `(existing_ancestor, remaining_suffix)` so the caller can
/// canonicalize the ancestor and re-attach the suffix.
fn find_existing_ancestor(path: &Path) -> Option<(PathBuf, PathBuf)> {
    let mut ancestor = path.to_path_buf();
    loop {
        if ancestor.exists() {
            let suffix = path
                .strip_prefix(&ancestor)
                .unwrap_or_else(|_| Path::new(""))
                .to_path_buf();
            return Some((ancestor, suffix));
        }

        if !ancestor.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_path_requires_base_dir_for_relative_paths() {
        let error = resolve_path("relative.txt", None).unwrap_err();
        assert!(error.contains("Relative paths require an explicit workspace root"));
    }

    #[test]
    fn test_resolve_path_with_policy_requires_base_dir() {
        let error = resolve_path_with_policy("/tmp/file.txt", None, true).unwrap_err();
        assert!(error.contains("explicit workspace root or base directory"));
    }
}
