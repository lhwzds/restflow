use std::time::Duration;

/// Common generated/build directories to skip during recursive traversal.
pub(crate) const COMMON_SKIP_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    ".tox",
    "target",
    "dist",
    "build",
    ".next",
    ".nuxt",
    ".venv",
    "venv",
];

/// Additional skip directories used by glob traversal.
const GLOB_EXTRA_SKIP_DIRS: &[&str] = &[".node_modules"];

/// Maximum number of LSP diagnostic errors to include in output.
pub(crate) const MAX_LSP_DIAGNOSTIC_ERRORS: usize = 20;

/// Timeout for waiting on LSP diagnostics after file edits.
pub(crate) const LSP_DIAGNOSTIC_TIMEOUT: Duration = Duration::from_secs(3);

/// Returns true when a directory name should be skipped by grep traversal.
pub(crate) fn should_skip_grep_dir(name: &str) -> bool {
    name.starts_with('.') || COMMON_SKIP_DIRS.contains(&name)
}

/// Returns true when a directory name should be skipped by glob traversal.
pub(crate) fn should_skip_glob_dir(name: &str) -> bool {
    name.starts_with('.')
        || COMMON_SKIP_DIRS.contains(&name)
        || GLOB_EXTRA_SKIP_DIRS.contains(&name)
}

/// Check if a file is likely binary based on extension.
pub(crate) fn is_likely_binary(name: &str) -> bool {
    let binary_extensions = [
        ".exe", ".dll", ".so", ".dylib", ".a", ".o", ".obj", ".png", ".jpg", ".jpeg", ".gif",
        ".bmp", ".ico", ".webp", ".mp3", ".mp4", ".avi", ".mov", ".mkv", ".wav", ".flac", ".zip",
        ".tar", ".gz", ".bz2", ".xz", ".7z", ".rar", ".pdf", ".doc", ".docx", ".xls", ".xlsx",
        ".ppt", ".pptx", ".wasm", ".pyc", ".pyo", ".class", ".jar", ".ttf", ".otf", ".woff",
        ".woff2", ".eot",
    ];

    let lower = name.to_lowercase();
    binary_extensions.iter().any(|ext| lower.ends_with(ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_skip_grep_dir() {
        assert!(should_skip_grep_dir(".git"));
        assert!(should_skip_grep_dir("target"));
        assert!(should_skip_grep_dir(".hidden"));
        assert!(!should_skip_grep_dir("src"));
    }

    #[test]
    fn test_should_skip_glob_dir() {
        assert!(should_skip_glob_dir(".git"));
        assert!(should_skip_glob_dir(".node_modules"));
        assert!(should_skip_glob_dir(".hidden"));
        assert!(!should_skip_glob_dir("src"));
    }

    #[test]
    fn test_is_likely_binary() {
        assert!(is_likely_binary("image.png"));
        assert!(is_likely_binary("archive.zip"));
        assert!(is_likely_binary("video.MP4"));
        assert!(!is_likely_binary("code.rs"));
        assert!(!is_likely_binary("readme.md"));
    }
}
