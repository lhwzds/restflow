use anyhow::{Result, bail};
use restflow_core::AppCore;
use restflow_core::auth::{AuthManagerConfig, AuthProfileManager, AuthProvider};
use restflow_core::models::chat_session::{ChatMessage, ChatSession};
use restflow_core::paths;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::timeout;

use crate::cli::ClaudeArgs;
use crate::commands::utils::read_stdin_to_string;
use crate::output::{OutputFormat, json::print_json};

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

fn claude_credentials_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".claude").join("credentials.json"))
}

fn is_claude_configured() -> bool {
    claude_credentials_path()
        .map(|path| path.exists())
        .unwrap_or(false)
}

async fn setup_claude_token(token: &str) -> Result<()> {
    let mut cmd = Command::new("claude");
    cmd.arg("setup-token")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(token.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
    }

    let output = child.wait_with_output().await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to setup Claude token: {}", stderr.trim());
    }

    Ok(())
}

fn parse_viewport(viewport: &str) -> Result<(u32, u32)> {
    let parts: Vec<&str> = viewport.split('x').collect();
    if parts.len() != 2 {
        bail!("Viewport must be in WIDTHxHEIGHT format (example: 1280x720)");
    }

    let width = parts[0]
        .parse::<u32>()
        .map_err(|_| anyhow::anyhow!("Viewport width must be a number"))?;
    let height = parts[1]
        .parse::<u32>()
        .map_err(|_| anyhow::anyhow!("Viewport height must be a number"))?;

    Ok((width, height))
}

async fn ensure_npx_available() -> Result<()> {
    let output = Command::new("npx")
        .arg("--version")
        .output()
        .await
        .map_err(|_| anyhow::anyhow!("npx is required for Playwright MCP (install Node.js)"))?;

    if !output.status.success() {
        bail!("npx is required for Playwright MCP (install Node.js)");
    }

    Ok(())
}

async fn generate_mcp_config(args: &ClaudeArgs) -> Result<PathBuf> {
    let config_dir = paths::ensure_data_dir()?;
    let config_path = config_dir.join("claude_mcp.json");

    let mut servers = serde_json::Map::new();
    servers.insert(
        "restflow".to_string(),
        serde_json::json!({
            "command": "restflow",
            "args": ["mcp", "serve"],
            "env": {}
        }),
    );

    if args.browser {
        ensure_npx_available().await?;
        let mut playwright_args = vec!["-y".to_string(), "@playwright/mcp".to_string()];

        if args.headless {
            playwright_args.push("--headless".to_string());
        } else {
            playwright_args.push("--headless=false".to_string());
        }

        if let Some(ref viewport) = args.viewport {
            let (width, height) = parse_viewport(viewport)?;
            playwright_args.push("--viewport-size".to_string());
            playwright_args.push(format!("{}x{}", width, height));
        }

        servers.insert(
            "playwright".to_string(),
            serde_json::json!({
                "command": "npx",
                "args": playwright_args,
                "env": {}
            }),
        );
    }

    let config = serde_json::json!({ "mcpServers": servers });

    std::fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;
    Ok(config_path)
}

async fn resolve_session_id(core: &Arc<AppCore>, args: &ClaudeArgs) -> Result<Option<String>> {
    if args.new_session && args.session.is_some() {
        bail!("Use either --session or --new-session, not both");
    }

    if args.new_session {
        let mut session = ChatSession::new("claude-cli".to_string(), args.model.clone());
        session.rename(format!("Claude CLI - {}", args.model));
        core.storage.chat_sessions.create(&session)?;
        return Ok(Some(session.id));
    }

    if let Some(ref id) = args.session {
        if let Some(session) = core.storage.chat_sessions.get(id)? {
            return Ok(Some(session.id));
        }

        let sessions = core.storage.chat_sessions.list()?;
        let mut matches = sessions
            .iter()
            .filter(|session| session.id.starts_with(id))
            .collect::<Vec<_>>();

        return match matches.len() {
            0 => bail!("Session not found: {}", id),
            1 => Ok(Some(matches.remove(0).id.clone())),
            _ => bail!("Session id is ambiguous: {}", id),
        };
    }

    Ok(None)
}

fn save_conversation(
    core: &Arc<AppCore>,
    session_id: &str,
    user_input: &str,
    assistant_output: &str,
) -> Result<()> {
    let mut session = core
        .storage
        .chat_sessions
        .get(session_id)?
        .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

    session.add_message(ChatMessage::user(user_input));
    session.add_message(ChatMessage::assistant(assistant_output));

    if session.messages.len() == 2 {
        session.auto_name_from_first_message();
    }

    core.storage.chat_sessions.save(&session)?;
    Ok(())
}

pub async fn run(core: Arc<AppCore>, args: ClaudeArgs, format: OutputFormat) -> Result<()> {
    // Get prompt from args or stdin
    let prompt = match args.prompt.as_ref() {
        Some(p) => p.clone(),
        None => read_stdin_to_string()?,
    };

    if prompt.is_empty() {
        bail!("Prompt is required");
    }

    let session_id = resolve_session_id(&core, &args).await?;
    if args.new_session && let Some(ref id) = session_id {
        eprintln!("Created session: {}", id);
    }

    // Get OAuth token from RestFlow auth profile
    let oauth_token = get_api_key_from_profile(args.auth_profile.as_deref()).await?;

    if !is_claude_configured() {
        eprintln!("Claude CLI not configured. Running setup-token...");
        setup_claude_token(&oauth_token).await?;
    }

    let mcp_config_path = generate_mcp_config(&args).await?;

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
        .arg(&args.model)
        .arg("--mcp-config")
        .arg(&mcp_config_path);

    // Session handling
    if let Some(ref id) = session_id {
        cmd.arg("--session-id").arg(id);
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
    let mut result: ClaudeOutput = match serde_json::from_str(&stdout) {
        Ok(parsed) => parsed,
        Err(e) => {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                bail!("Claude CLI failed: {}\nstderr: {}", stdout, stderr);
            }
            bail!("Failed to parse Claude CLI output: {}\nraw: {}", e, stdout);
        }
    };

    if let Some(ref id) = session_id
        && result.session_id.is_none()
    {
        result.session_id = Some(id.clone());
    }

    // Check for error in response
    if result.is_error == Some(true)
        && let Some(text) = result.get_text()
    {
        bail!("Claude CLI error: {}", text);
    }

    if let Some(ref id) = session_id
        && let Some(text) = result.get_text()
        && let Err(err) = save_conversation(&core, id, &prompt, text)
    {
        tracing::warn!(session_id = %id, error = %err, "Failed to save CLI conversation");
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
        if let Some(ref id) = result.session_id {
            eprintln!("[Session: {}]", id);
        }
    }

    Ok(())
}
