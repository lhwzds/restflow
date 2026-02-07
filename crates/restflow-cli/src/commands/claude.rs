use anyhow::{Result, bail};
use reqwest::header::ACCEPT;
use restflow_core::auth::AuthProvider;
use restflow_core::daemon::{
    DaemonConfig, DaemonStatus, IpcClient, check_daemon_status, ensure_daemon_running_with_config,
    is_daemon_available, stop_daemon,
};
use restflow_core::models::ChatRole;
use restflow_core::paths;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
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

fn build_restflow_mcp_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/mcp")
}

async fn is_mcp_http_ready(port: u16) -> bool {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(client) => client,
        Err(_) => return false,
    };

    let response = client
        .get(build_restflow_mcp_url(port))
        .header(ACCEPT, "text/event-stream")
        .send()
        .await;

    match response {
        Ok(resp) => matches!(resp.status().as_u16(), 200 | 401 | 406),
        Err(_) => false,
    }
}

async fn wait_for_daemon_exit() -> Result<()> {
    for _ in 0..50 {
        match check_daemon_status()? {
            DaemonStatus::Running { .. } => tokio::time::sleep(Duration::from_millis(100)).await,
            DaemonStatus::NotRunning | DaemonStatus::Stale { .. } => return Ok(()),
        }
    }

    bail!("Daemon did not stop within timeout")
}

async fn ensure_daemon_with_mcp(mcp_port: u16) -> Result<()> {
    let daemon_config = DaemonConfig {
        mcp: true,
        mcp_port: Some(mcp_port),
        ..DaemonConfig::default()
    };

    ensure_daemon_running_with_config(daemon_config.clone()).await?;
    if is_mcp_http_ready(mcp_port).await {
        return Ok(());
    }

    if matches!(check_daemon_status()?, DaemonStatus::Running { .. }) {
        eprintln!(
            "RestFlow daemon is running without MCP HTTP. Restarting daemon with MCP enabled..."
        );
        let _ = stop_daemon()?;
        wait_for_daemon_exit().await?;
    }

    ensure_daemon_running_with_config(daemon_config).await?;
    if !is_mcp_http_ready(mcp_port).await {
        bail!(
            "RestFlow MCP HTTP server is not reachable at {}",
            build_restflow_mcp_url(mcp_port)
        );
    }

    Ok(())
}

async fn get_ipc_client() -> Result<IpcClient> {
    let socket_path = paths::socket_path()?;
    if !is_daemon_available(&socket_path).await {
        bail!("RestFlow daemon is not running. Start it with 'restflow start'.");
    }

    IpcClient::connect(&socket_path).await
}

/// Get API key from RestFlow auth profile
async fn get_api_key_from_profile(profile_id: Option<&str>) -> Result<String> {
    let mut client = get_ipc_client().await?;

    if let Some(id) = profile_id {
        let profiles = client.list_auth_profiles().await?;
        let profile = profiles
            .iter()
            .find(|p| p.id == id || p.id.starts_with(id))
            .ok_or_else(|| anyhow::anyhow!("Auth profile not found: {}", id))?;
        return client.get_api_key_for_profile(profile.id.clone()).await;
    }

    match client.get_api_key(AuthProvider::ClaudeCode).await {
        Ok(key) => Ok(key),
        Err(_) => bail!(
            "No available ClaudeCode auth profile found. Run 'restflow auth add --provider claude-code --key <your-oauth-token>' to add one."
        ),
    }
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

async fn generate_mcp_config(args: &ClaudeArgs, restflow_mcp_url: &str) -> Result<PathBuf> {
    let config_dir = paths::ensure_restflow_dir()?;
    let config_path = config_dir.join("claude_mcp.json");

    let mut servers = serde_json::Map::new();
    servers.insert(
        "restflow".to_string(),
        serde_json::json!({
            "type": "http",
            "url": restflow_mcp_url
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
                "type": "stdio",
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

async fn resolve_session_id(args: &ClaudeArgs) -> Result<Option<String>> {
    if args.new_session && args.session.is_some() {
        bail!("Use either --session or --new-session, not both");
    }

    let mut client = get_ipc_client().await?;

    if args.new_session {
        let session = client
            .create_session(
                Some("claude-cli".to_string()),
                Some(args.model.clone()),
                Some(format!("Claude CLI - {}", args.model)),
                None,
            )
            .await?;
        return Ok(Some(session.id));
    }

    if let Some(ref id) = args.session {
        if let Ok(session) = client.get_session(id.clone()).await {
            return Ok(Some(session.id));
        }

        let sessions = client.list_full_sessions().await?;
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

async fn save_conversation(
    session_id: &str,
    user_input: &str,
    assistant_output: &str,
) -> Result<()> {
    let mut client = get_ipc_client().await?;
    client
        .add_message(
            session_id.to_string(),
            ChatRole::User,
            user_input.to_string(),
        )
        .await?;
    client
        .add_message(
            session_id.to_string(),
            ChatRole::Assistant,
            assistant_output.to_string(),
        )
        .await?;
    Ok(())
}

pub async fn run(args: ClaudeArgs, format: OutputFormat) -> Result<()> {
    // Get prompt from args or stdin
    let prompt = match args.prompt.as_ref() {
        Some(p) => p.clone(),
        None => read_stdin_to_string()?,
    };

    if prompt.is_empty() {
        bail!("Prompt is required");
    }

    ensure_daemon_with_mcp(args.mcp_port).await?;

    let session_id = resolve_session_id(&args).await?;
    if args.new_session
        && let Some(ref id) = session_id
    {
        eprintln!("Created session: {}", id);
    }

    // Get OAuth token from RestFlow auth profile
    let oauth_token = get_api_key_from_profile(args.auth_profile.as_deref()).await?;

    if !is_claude_configured() {
        eprintln!("Claude CLI not configured. Running setup-token...");
        setup_claude_token(&oauth_token).await?;
    }

    let mcp_url = build_restflow_mcp_url(args.mcp_port);
    let mcp_config_path = generate_mcp_config(&args, &mcp_url).await?;

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

    add_default_home_dir(&mut cmd);

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
        && let Err(err) = save_conversation(id, &prompt, text).await
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

fn add_default_home_dir(cmd: &mut Command) {
    // Default to the user's home directory for parity with local permissions.
    if let Some(home) = dirs::home_dir() {
        cmd.arg("--add-dir").arg(home);
    }
}

#[cfg(test)]
mod tests {
    use super::{add_default_home_dir, build_restflow_mcp_url};
    use std::env;
    use tokio::process::Command;

    #[test]
    fn add_default_home_dir_appends_add_dir() {
        let temp = tempfile::tempdir().expect("temp dir");
        let home = temp.path().to_path_buf();

        let old_home = env::var_os("HOME");
        let old_userprofile = env::var_os("USERPROFILE");

        // SAFETY: This test runs serially and restores env vars before returning.
        unsafe {
            env::set_var("HOME", &home);
            env::set_var("USERPROFILE", &home);
        }

        let mut cmd = Command::new("claude");
        add_default_home_dir(&mut cmd);

        let args: Vec<String> = cmd
            .as_std()
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect();

        assert_eq!(
            args,
            vec!["--add-dir".to_string(), home.to_string_lossy().to_string()]
        );

        // SAFETY: Restoring original env vars; test runs serially.
        unsafe {
            match old_home {
                Some(value) => env::set_var("HOME", value),
                None => env::remove_var("HOME"),
            }
            match old_userprofile {
                Some(value) => env::set_var("USERPROFILE", value),
                None => env::remove_var("USERPROFILE"),
            }
        }
    }

    #[test]
    fn build_restflow_mcp_url_uses_loopback_and_port() {
        assert_eq!(build_restflow_mcp_url(8787), "http://127.0.0.1:8787/mcp");
    }
}
