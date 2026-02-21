//! Shared utilities for CLI-based LLM providers.

use std::path::{Path, PathBuf};

use crate::error::{AiError, Result};
use crate::llm::client::Role;

/// Build a prompt string from messages, excluding system messages.
pub fn build_prompt(messages: &[crate::llm::Message]) -> String {
    messages
        .iter()
        .filter(|m| m.role != Role::System)
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Resolve a CLI executable by checking an env override, PATH, and fallback locations.
pub fn resolve_executable(
    name: &str,
    override_env: &str,
    fallbacks: &[PathBuf],
) -> Result<PathBuf> {
    if let Ok(raw) = std::env::var(override_env)
        && !raw.trim().is_empty()
    {
        let path = PathBuf::from(raw);
        if is_executable(&path) {
            return Ok(path);
        }
        return Err(AiError::Llm(format!(
            "{} points to non-executable path: {}",
            override_env,
            path.display()
        )));
    }

    if let Some(path) = resolve_from_path(name) {
        return Ok(path);
    }

    for fallback in fallbacks {
        if is_executable(fallback) {
            return Ok(fallback.clone());
        }
    }

    Err(AiError::Llm(format!(
        "Failed to locate '{}' executable in PATH or fallback locations",
        name
    )))
}

/// Search PATH for an executable by name.
pub fn resolve_from_path(name: &str) -> Option<PathBuf> {
    let path_value = std::env::var_os("PATH")?;
    for entry in std::env::split_paths(&path_value) {
        let candidate = entry.join(name);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Check whether a path points to an executable file.
pub fn is_executable(path: &Path) -> bool {
    if !path.exists() || !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(path) {
            return metadata.permissions().mode() & 0o111 != 0;
        }
        false
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::client::{Message, Role};

    fn msg(role: Role, content: &str) -> Message {
        Message {
            role,
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    #[test]
    fn test_build_prompt_filters_system() {
        let messages = vec![
            msg(Role::System, "You are a helper"),
            msg(Role::User, "Hello"),
        ];
        let result = build_prompt(&messages);
        assert_eq!(result, "Hello");
        assert!(!result.contains("helper"));
    }

    #[test]
    fn test_build_prompt_joins_with_double_newline() {
        let messages = vec![
            msg(Role::User, "Hello"),
            msg(Role::Assistant, "World"),
        ];
        let result = build_prompt(&messages);
        assert_eq!(result, "Hello\n\nWorld");
    }

    #[test]
    fn test_is_executable_nonexistent() {
        assert!(!is_executable(Path::new("/nonexistent/path/to/binary")));
    }
}
