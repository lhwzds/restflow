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

/// Standard fallback paths for a CLI executable name.
///
/// Checks `/opt/homebrew/bin/{name}`, `/usr/local/bin/{name}`,
/// `/usr/bin/{name}`, and `~/.local/bin/{name}`.
pub fn standard_fallbacks(name: &str) -> Vec<PathBuf> {
    let mut paths = vec![
        PathBuf::from(format!("/opt/homebrew/bin/{name}")),
        PathBuf::from(format!("/usr/local/bin/{name}")),
        PathBuf::from(format!("/usr/bin/{name}")),
    ];
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".local").join("bin").join(name));
    }
    paths
}

/// Parse a JSON response that has a `"response"` field and an optional `"error"` field.
///
/// Used by Gemini CLI and OpenCode CLI which share the same output format.
pub fn parse_json_response(output: &str, provider: &str) -> Result<String> {
    let value: serde_json::Value = serde_json::from_str(output.trim())
        .map_err(|e| AiError::Llm(format!("Failed to parse {} CLI output: {e}", provider)))?;

    if let Some(err) = value.get("error").and_then(|v| v.as_str()) {
        return Err(AiError::Llm(format!("{} CLI error: {err}", provider)));
    }

    let response = value
        .get("response")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AiError::Llm(format!("{} CLI output missing 'response' field", provider)))?;

    if response.trim().is_empty() {
        return Err(AiError::Llm(format!(
            "{} CLI returned empty output",
            provider
        )));
    }

    Ok(response.to_string())
}

/// Execute a CLI command and return its stdout as a string.
///
/// Returns an error if the command fails to spawn or exits with non-zero status.
pub async fn execute_cli_command(
    mut cmd: tokio::process::Command,
    provider: &str,
    install_hint: &str,
) -> Result<String> {
    let output = cmd.output().await.map_err(|e| {
        AiError::Llm(format!(
            "Failed to run {} CLI: {}. {}",
            provider, e, install_hint
        ))
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AiError::Llm(format!("{} CLI error: {}", provider, stderr)));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Return a stream that immediately yields an "unsupported" error.
pub fn unsupported_stream(provider: &str) -> crate::llm::StreamResult {
    let msg = format!("Streaming not supported with {}", provider);
    Box::pin(async_stream::stream! {
        yield Err(AiError::Llm(msg));
    })
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
        let messages = vec![msg(Role::User, "Hello"), msg(Role::Assistant, "World")];
        let result = build_prompt(&messages);
        assert_eq!(result, "Hello\n\nWorld");
    }

    #[test]
    fn test_is_executable_nonexistent() {
        assert!(!is_executable(Path::new("/nonexistent/path/to/binary")));
    }

    #[test]
    fn test_standard_fallbacks_contains_homebrew() {
        let paths = standard_fallbacks("claude");
        assert!(
            paths
                .iter()
                .any(|p| p.to_str().unwrap().contains("/opt/homebrew/bin/claude"))
        );
    }

    #[test]
    fn test_parse_json_response_success() {
        let output = r#"{"response":"Hello"}"#;
        let result = parse_json_response(output, "TestCLI").unwrap();
        assert_eq!(result, "Hello");
    }

    #[test]
    fn test_parse_json_response_error() {
        let output = r#"{"error":"auth failed"}"#;
        let err = parse_json_response(output, "TestCLI").unwrap_err();
        assert!(err.to_string().contains("TestCLI CLI error"));
    }

    #[test]
    fn test_parse_json_response_missing_field() {
        let output = r#"{"data":"hello"}"#;
        let err = parse_json_response(output, "TestCLI").unwrap_err();
        assert!(err.to_string().contains("missing 'response' field"));
    }
}
