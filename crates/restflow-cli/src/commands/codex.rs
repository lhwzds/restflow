use anyhow::{Result, bail};
use restflow_core::AppCore;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

use crate::cli::CodexArgs;
use crate::commands::utils::read_stdin_to_string;
use crate::output::{OutputFormat, json::print_json};

#[derive(Debug, Deserialize)]
struct CodexAuthFile {
    tokens: CodexAuthTokens,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CodexAuthTokens {
    access_token: String,
    refresh_token: Option<String>,
    account_id: Option<String>,
}

fn codex_auth_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".codex").join("auth.json"))
}

fn ensure_codex_credentials() -> Result<CodexAuthTokens> {
    let path = codex_auth_path().ok_or_else(|| anyhow::anyhow!("No home directory found"))?;
    if !path.exists() {
        bail!(
            "Codex credentials not found at {}. Run 'codex login' first.",
            path.display()
        );
    }

    let content = std::fs::read_to_string(&path)?;
    let parsed: CodexAuthFile = serde_json::from_str(&content)?;
    if parsed.tokens.access_token.trim().is_empty() {
        bail!("Codex access token is empty. Run 'codex login' again.");
    }

    Ok(parsed.tokens)
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CodexJsonlLine {
    #[serde(rename = "type")]
    line_type: Option<String>,
    content: Option<String>,
    thread_id: Option<String>,
    error: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct CodexResult {
    text: String,
    session_id: Option<String>,
    is_error: bool,
}

fn parse_jsonl_output(output: &str) -> Result<CodexResult> {
    let mut text = String::new();
    let mut session_id = None;
    let mut is_error = false;

    for line in output.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parsed: CodexJsonlLine = serde_json::from_str(line)?;
        if let Some(content) = parsed.content {
            text.push_str(&content);
        }
        if let Some(thread_id) = parsed.thread_id {
            session_id = Some(thread_id);
        }
        if parsed.error.is_some() {
            is_error = true;
        }
    }

    Ok(CodexResult {
        text,
        session_id,
        is_error,
    })
}

pub async fn run(_core: Arc<AppCore>, args: CodexArgs, format: OutputFormat) -> Result<()> {
    // Validate Codex credentials
    let _tokens = ensure_codex_credentials()?;

    // Get prompt from args or stdin
    let prompt = match args.prompt.as_ref() {
        Some(prompt) => prompt.clone(),
        None => read_stdin_to_string()?,
    };

    if prompt.is_empty() {
        bail!("Prompt is required");
    }

    let mut cmd = Command::new("codex");

    if let Some(ref session_id) = args.session {
        cmd.args(["exec", "resume", session_id]);
        cmd.args([
            "--color",
            "never",
            "--sandbox",
            "read-only",
            "--skip-git-repo-check",
        ]);
        cmd.arg(&prompt);
    } else {
        cmd.args([
            "exec",
            "--json",
            "--color",
            "never",
            "--sandbox",
            "read-only",
            "--skip-git-repo-check",
            "--model",
            &args.model,
        ]);
        cmd.arg(&prompt);
    }

    if let Some(ref cwd) = args.cwd {
        cmd.current_dir(cwd);
    }

    let output = timeout(Duration::from_secs(args.timeout), cmd.output()).await??;

    if let Some(ref session_id) = args.session {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let result = CodexResult {
            text: stdout.to_string(),
            session_id: Some(session_id.clone()),
            is_error: !output.status.success(),
        };

        if result.is_error {
            bail!("Codex CLI failed: {}", result.text.trim());
        }

        if format.is_json() {
            print_json(&result)?;
        } else {
            println!("{}", result.text);
            if let Some(ref id) = result.session_id {
                eprintln!("[Session: {}]", id);
            }
        }

        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut result = parse_jsonl_output(&stdout)?;

    if result.session_id.is_none()
        && let Some(thread_id) = result.text.lines().find_map(|line| {
            line.strip_prefix("[Session: ")
                .and_then(|l| l.strip_suffix(']'))
        })
    {
        result.session_id = Some(thread_id.to_string());
    }

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = if result.text.trim().is_empty() {
            stderr.trim()
        } else {
            result.text.trim()
        };
        bail!("Codex CLI failed: {}", message);
    }

    if result.is_error {
        bail!("Codex CLI error: {}", result.text.trim());
    }

    if format.is_json() {
        print_json(&result)?;
    } else {
        println!("{}", result.text);
        if let Some(ref id) = result.session_id {
            eprintln!("[Session: {}]", id);
        }
    }

    Ok(())
}
