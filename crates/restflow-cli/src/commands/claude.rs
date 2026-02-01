use anyhow::{bail, Result};
use restflow_core::auth::{AuthManagerConfig, AuthProfileManager, AuthProvider};
use restflow_core::paths;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

use crate::cli::ClaudeArgs;
use crate::commands::utils::read_stdin_to_string;
use crate::output::{json::print_json, OutputFormat};

/// Claude CLI output structure (matches actual claude CLI JSON output)
#[derive(Debug, Deserialize, Serialize)]
struct ClaudeOutput {
    /// Response text (claude CLI uses "result" field)
    result: Option<String>,
    /// Alternative: some versions may use "message"
    message: Option<String>,
    /// Alternative: some versions may use "content"
    content: Option<String>,
    /// Session ID for conversation continuity
    session_id: Option<String>,
    /// Whether there was an error
    is_error: Option<bool>,
    /// Token usage statistics
    usage: Option<ClaudeUsage>,
    /// Execution duration in milliseconds
    duration_ms: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ClaudeUsage {
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
    cache_read_input_tokens: Option<u32>,
    cache_creation_input_tokens: Option<u32>,
}

impl ClaudeOutput {
    /// Get the response text from whichever field contains it
    fn get_text(&self) -> Option<&str> {
        self.result
            .as_deref()
            .or(self.message.as_deref())
            .or(self.content.as_deref())
    }
}


/// Get API key from RestFlow auth profile
async fn get_api_key_from_profile(profile_id: Option<&str>) -> Result<String> {
    let mut config = AuthManagerConfig::default();
    let profiles_path = paths::ensure_data_dir()?.join("auth_profiles.json");
    config.profiles_path = Some(profiles_path);

    let manager = AuthProfileManager::with_config(config);
    manager.initialize().await?;

    let profiles = manager.list_profiles().await;

    // If profile ID specified, find that specific profile
    if let Some(id) = profile_id {
        let profile = profiles
            .iter()
            .find(|p| p.id == id || p.id.starts_with(id))
            .ok_or_else(|| anyhow::anyhow!("Auth profile not found: {}", id))?;

        return Ok(profile.get_api_key().to_string());
    }

    // Otherwise, find first available ClaudeCode profile
    let claude_code_profile = profiles
        .iter()
        .find(|p| p.provider == AuthProvider::ClaudeCode && p.is_available())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No available ClaudeCode auth profile found. Run 'restflow auth add --provider claude-code --key <your-oauth-token>' to add one."
            )
        })?;

    Ok(claude_code_profile.get_api_key().to_string())
}

pub async fn run(args: ClaudeArgs, format: OutputFormat) -> Result<()> {
    // Get prompt from args or stdin
    let prompt = match args.prompt {
        Some(p) => p,
        None => read_stdin_to_string()?,
    };

    if prompt.is_empty() {
        bail!("Prompt is required");
    }

    // Get OAuth token from RestFlow auth profile
    let oauth_token = get_api_key_from_profile(args.auth_profile.as_deref()).await?;

    // Build environment with OAuth token
    // Use CLAUDE_CODE_OAUTH_TOKEN for setup tokens (sk-ant-oat01-...)
    let mut env: HashMap<String, String> = std::env::vars().collect();
    env.remove("ANTHROPIC_API_KEY");
    env.remove("ANTHROPIC_API_KEY_OLD");
    env.insert("CLAUDE_CODE_OAUTH_TOKEN".to_string(), oauth_token);

    // Build command
    let mut cmd = Command::new("claude");
    cmd.arg("-p")
        .arg("--output-format")
        .arg("json")
        .arg("--dangerously-skip-permissions")
        .arg("--model")
        .arg(&args.model);

    // Session handling
    if let Some(ref session_id) = args.session_id {
        if args.resume {
            cmd.arg("--resume").arg(session_id);
        } else {
            cmd.arg("--session-id").arg(session_id);
        }
    }

    // Working directory
    if let Some(ref cwd) = args.cwd {
        cmd.current_dir(cwd);
    }

    // Set environment
    cmd.env_clear();
    cmd.envs(env);

    // Prompt as positional argument
    cmd.arg(&prompt);

    // Execute with timeout
    let output = timeout(Duration::from_secs(args.timeout), cmd.output()).await??;

    // Parse output even if exit code is non-zero (claude CLI returns JSON even on error)
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: ClaudeOutput = match serde_json::from_str(&stdout) {
        Ok(parsed) => parsed,
        Err(e) => {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                bail!("Claude CLI failed: {}\nstderr: {}", stdout, stderr);
            }
            bail!("Failed to parse Claude CLI output: {}\nraw: {}", e, stdout);
        }
    };

    // Check for error in response
    if result.is_error == Some(true)
        && let Some(text) = result.get_text()
    {
        bail!("Claude CLI error: {}", text);
    }

    if format.is_json() {
        print_json(&result)?;
    } else {
        if let Some(text) = result.get_text() {
            println!("{}", text);
        }
        if let Some(ref usage) = result.usage {
            eprintln!(
                "\n[Tokens: {} in, {} out]",
                usage.input_tokens.unwrap_or(0),
                usage.output_tokens.unwrap_or(0)
            );
        }
        if let Some(ref session_id) = result.session_id {
            eprintln!("[Session: {}]", session_id);
        }
    }

    Ok(())
}
