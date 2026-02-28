//! AI-first browser runtime for RestFlow.
//!
//! This crate provides a session-oriented browser automation service designed
//! for agent tool usage. The current implementation uses Chromium CDP directly
//! (without Playwright runtime dependency) and exposes:
//! - Runtime probing
//! - Session lifecycle management
//! - JavaScript/TypeScript execution in page context
//! - Structured browser action plans

use anyhow::{Result, anyhow, bail};
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet, VecDeque};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::process::{Child, Command};
use tokio::sync::RwLock;
use tokio::time::{sleep, timeout};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};
use uuid::Uuid;

const DEFAULT_TIMEOUT_SECS: u64 = 120;
const CDP_POLL_INTERVAL_MS: u64 = 100;
const CDP_SHUTDOWN_TIMEOUT_SECS: u64 = 5;
const NETWORK_IDLE_GRACE_MS: u64 = 500;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BrowserKind {
    #[default]
    Chromium,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScriptLanguage {
    #[default]
    Js,
    Ts,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScriptRuntime {
    #[default]
    Auto,
    Node,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InputModifier {
    Alt,
    Control,
    Meta,
    Shift,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeProbe {
    pub node_available: bool,
    pub node_version: Option<String>,
    pub node_typescript_available: bool,
    pub playwright_package_available: bool,
    pub chromium_cache_detected: bool,
    pub ready: bool,
    pub notes: Vec<String>,
}

impl RuntimeProbe {
    fn empty() -> Self {
        Self {
            node_available: false,
            node_version: None,
            node_typescript_available: false,
            playwright_package_available: false,
            chromium_cache_detected: false,
            ready: false,
            notes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewSessionRequest {
    #[serde(default)]
    pub browser: BrowserKind,
    #[serde(default = "default_headless")]
    pub headless: bool,
}

impl Default for NewSessionRequest {
    fn default() -> Self {
        Self {
            browser: BrowserKind::Chromium,
            headless: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserSession {
    pub id: String,
    pub browser: BrowserKind,
    pub headless: bool,
    pub created_at_ms: i64,
    pub session_dir: String,
    pub profile_dir: String,
    pub artifacts_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunScriptRequest {
    pub session_id: String,
    pub code: String,
    #[serde(default)]
    pub language: ScriptLanguage,
    #[serde(default)]
    pub runtime: ScriptRuntime,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunActionsRequest {
    pub session_id: String,
    pub actions: Vec<BrowserAction>,
    #[serde(default)]
    pub runtime: ScriptRuntime,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrowserAction {
    Navigate {
        url: String,
        #[serde(default)]
        wait_until: Option<String>,
    },
    Click {
        selector: String,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    Fill {
        selector: String,
        text: String,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    Type {
        selector: String,
        text: String,
        #[serde(default)]
        delay_ms: Option<u64>,
    },
    Press {
        key: String,
        #[serde(default)]
        selector: Option<String>,
    },
    KeyDown {
        key: String,
        #[serde(default)]
        selector: Option<String>,
        #[serde(default)]
        modifiers: Vec<InputModifier>,
    },
    KeyUp {
        key: String,
        #[serde(default)]
        selector: Option<String>,
        #[serde(default)]
        modifiers: Vec<InputModifier>,
    },
    MouseMove {
        x: f64,
        y: f64,
        #[serde(default)]
        modifiers: Vec<InputModifier>,
    },
    MouseDown {
        x: f64,
        y: f64,
        #[serde(default)]
        button: MouseButton,
        #[serde(default = "default_click_count")]
        click_count: u32,
        #[serde(default)]
        modifiers: Vec<InputModifier>,
    },
    MouseUp {
        x: f64,
        y: f64,
        #[serde(default)]
        button: MouseButton,
        #[serde(default = "default_click_count")]
        click_count: u32,
        #[serde(default)]
        modifiers: Vec<InputModifier>,
    },
    MouseClick {
        x: f64,
        y: f64,
        #[serde(default)]
        button: MouseButton,
        #[serde(default = "default_click_count")]
        click_count: u32,
        #[serde(default)]
        modifiers: Vec<InputModifier>,
    },
    MouseWheel {
        x: f64,
        y: f64,
        delta_x: f64,
        delta_y: f64,
        #[serde(default)]
        modifiers: Vec<InputModifier>,
    },
    WaitForSelector {
        selector: String,
        #[serde(default)]
        state: Option<String>,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    ExtractText {
        selector: String,
        #[serde(default)]
        all: bool,
    },
    Screenshot {
        path: String,
        #[serde(default)]
        full_page: bool,
    },
    Evaluate {
        expression: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserExecutionResult {
    pub runtime: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub stdout: String,
    pub stderr: String,
    pub payload: Option<Value>,
}

impl BrowserExecutionResult {
    pub fn failed_message(&self) -> String {
        if let Some(payload) = &self.payload
            && let Some(error) = payload.get("error").and_then(Value::as_str)
        {
            return error.to_string();
        }

        let stderr = self.stderr.trim();
        if !stderr.is_empty() {
            return stderr.to_string();
        }

        format!("Browser execution failed with exit code {}", self.exit_code)
    }
}

#[async_trait]
pub trait BrowserExecutor: Send + Sync {
    async fn probe_runtime(&self) -> Result<RuntimeProbe>;

    async fn run_script(
        &self,
        session: &BrowserSession,
        request: &RunScriptRequest,
    ) -> Result<BrowserExecutionResult>;

    async fn run_actions(
        &self,
        session: &BrowserSession,
        request: &RunActionsRequest,
    ) -> Result<BrowserExecutionResult>;
}

/// Browser automation service with session lifecycle and executor delegation.
pub struct BrowserService {
    root_dir: PathBuf,
    sessions: RwLock<HashMap<String, BrowserSession>>,
    executor: Arc<dyn BrowserExecutor>,
}

impl BrowserService {
    pub fn new() -> Result<Self> {
        let root = resolve_default_root_dir()?;
        Self::new_with_executor(root, Arc::new(CdpExecutor::new()))
    }

    pub fn new_with_executor(
        root_dir: PathBuf,
        executor: Arc<dyn BrowserExecutor>,
    ) -> Result<Self> {
        std::fs::create_dir_all(&root_dir)?;
        Ok(Self {
            root_dir,
            sessions: RwLock::new(HashMap::new()),
            executor,
        })
    }

    pub async fn probe_runtime(&self) -> Result<RuntimeProbe> {
        self.executor.probe_runtime().await
    }

    pub async fn new_session(&self, request: NewSessionRequest) -> Result<BrowserSession> {
        if request.browser != BrowserKind::Chromium {
            bail!("Only chromium is supported in this version");
        }

        let id = Uuid::new_v4().to_string();
        let session_dir = self.root_dir.join(&id);
        let profile_dir = session_dir.join("profile");
        let artifacts_dir = session_dir.join("artifacts");

        std::fs::create_dir_all(&profile_dir)?;
        std::fs::create_dir_all(&artifacts_dir)?;

        let session = BrowserSession {
            id: id.clone(),
            browser: request.browser,
            headless: request.headless,
            created_at_ms: Utc::now().timestamp_millis(),
            session_dir: session_dir.display().to_string(),
            profile_dir: profile_dir.display().to_string(),
            artifacts_dir: artifacts_dir.display().to_string(),
        };

        let mut sessions = self.sessions.write().await;
        sessions.insert(id, session.clone());

        Ok(session)
    }

    pub async fn list_sessions(&self) -> Vec<BrowserSession> {
        let sessions = self.sessions.read().await;
        let mut values: Vec<BrowserSession> = sessions.values().cloned().collect();
        values.sort_by_key(|session| session.created_at_ms);
        values
    }

    pub async fn close_session(&self, session_id: &str) -> Result<bool> {
        let mut sessions = self.sessions.write().await;
        let Some(session) = sessions.remove(session_id) else {
            return Ok(false);
        };

        let session_dir = PathBuf::from(session.session_dir);
        if session_dir.exists() {
            std::fs::remove_dir_all(session_dir)?;
        }

        Ok(true)
    }

    pub async fn run_script(&self, request: &RunScriptRequest) -> Result<BrowserExecutionResult> {
        let session = self.get_session(&request.session_id).await?;
        self.executor.run_script(&session, request).await
    }

    pub async fn run_actions(&self, request: &RunActionsRequest) -> Result<BrowserExecutionResult> {
        let session = self.get_session(&request.session_id).await?;
        self.executor.run_actions(&session, request).await
    }

    async fn get_session(&self, session_id: &str) -> Result<BrowserSession> {
        let sessions = self.sessions.read().await;
        sessions
            .get(session_id)
            .cloned()
            .ok_or_else(|| anyhow!("Session not found: {}", session_id))
    }
}

#[derive(Default)]
pub struct CdpExecutor;

impl CdpExecutor {
    pub fn new() -> Self {
        Self
    }

    async fn run_script_inner(
        &self,
        session: &BrowserSession,
        request: &RunScriptRequest,
    ) -> Result<Value> {
        let script_source = if request.language == ScriptLanguage::Ts {
            let cwd = request.cwd.as_deref().map(Path::new);
            transpile_typescript_source(&request.code, cwd, request.timeout_secs).await?
        } else {
            request.code.clone()
        };

        let mut runtime =
            CdpRuntime::start(session.headless, &session.profile_dir, request.timeout_secs).await?;

        let eval_script = build_user_script_wrapper(&script_source)?;
        let value = runtime
            .evaluate_page_script(&eval_script)
            .await
            .and_then(extract_action_result)?;

        let shutdown_result = runtime.shutdown().await;
        if let Err(error) = shutdown_result {
            tracing::warn!("CDP runtime shutdown error: {}", error);
        }

        Ok(value)
    }

    async fn run_actions_inner(
        &self,
        session: &BrowserSession,
        request: &RunActionsRequest,
    ) -> Result<Vec<Value>> {
        let mut runtime =
            CdpRuntime::start(session.headless, &session.profile_dir, request.timeout_secs).await?;

        let mut outputs = Vec::with_capacity(request.actions.len());

        for action in &request.actions {
            let output = runtime
                .execute_action(action, &session.artifacts_dir)
                .await?;
            outputs.push(output);
        }

        let shutdown_result = runtime.shutdown().await;
        if let Err(error) = shutdown_result {
            tracing::warn!("CDP runtime shutdown error: {}", error);
        }

        Ok(outputs)
    }
}

#[async_trait]
impl BrowserExecutor for CdpExecutor {
    async fn probe_runtime(&self) -> Result<RuntimeProbe> {
        let mut probe = RuntimeProbe::empty();

        let node_probe = run_command_capture("node", &["--version".to_string()], None, 10).await;
        if let Ok(output) = node_probe
            && output.exit_code == 0
        {
            probe.node_available = true;
            probe.node_version = Some(output.stdout.trim().to_string());
        }

        if probe.node_available {
            let ts_probe = run_command_capture(
                "node",
                &[
                    "--experimental-strip-types".to_string(),
                    "-e".to_string(),
                    "const value: number = 1; console.log(value);".to_string(),
                ],
                None,
                10,
            )
            .await;
            probe.node_typescript_available = ts_probe
                .map(|output| output.exit_code == 0)
                .unwrap_or(false);

            let playwright_probe = run_command_capture(
                "node",
                &[
                    "--input-type=module".to_string(),
                    "-e".to_string(),
                    "import('playwright').then(() => process.exit(0)).catch(() => process.exit(1));"
                        .to_string(),
                ],
                None,
                10,
            )
            .await;
            probe.playwright_package_available = playwright_probe
                .map(|output| output.exit_code == 0)
                .unwrap_or(false);
        }

        let chromium_path = resolve_chromium_binary();
        probe.chromium_cache_detected = chromium_path.is_some();
        probe.ready = probe.chromium_cache_detected;

        if let Some(path) = chromium_path {
            probe
                .notes
                .push(format!("Chromium executable detected: {}", path));
        } else {
            probe.notes.push(
                "Chromium executable not found. Set RESTFLOW_CHROMIUM_PATH or install Chrome/Chromium in PATH."
                    .to_string(),
            );
        }

        probe
            .notes
            .push("Runtime mode: native CDP (Playwright package not required).".to_string());

        Ok(probe)
    }

    async fn run_script(
        &self,
        session: &BrowserSession,
        request: &RunScriptRequest,
    ) -> Result<BrowserExecutionResult> {
        let started = Instant::now();
        match self.run_script_inner(session, request).await {
            Ok(value) => Ok(BrowserExecutionResult {
                runtime: "cdp_chromium".to_string(),
                exit_code: 0,
                duration_ms: started.elapsed().as_millis() as u64,
                stdout: String::new(),
                stderr: String::new(),
                payload: Some(json!({"success": true, "result": value})),
            }),
            Err(error) => Ok(BrowserExecutionResult {
                runtime: "cdp_chromium".to_string(),
                exit_code: 1,
                duration_ms: started.elapsed().as_millis() as u64,
                stdout: String::new(),
                stderr: error.to_string(),
                payload: Some(json!({"success": false, "error": error.to_string()})),
            }),
        }
    }

    async fn run_actions(
        &self,
        session: &BrowserSession,
        request: &RunActionsRequest,
    ) -> Result<BrowserExecutionResult> {
        let started = Instant::now();
        match self.run_actions_inner(session, request).await {
            Ok(values) => Ok(BrowserExecutionResult {
                runtime: "cdp_chromium".to_string(),
                exit_code: 0,
                duration_ms: started.elapsed().as_millis() as u64,
                stdout: String::new(),
                stderr: String::new(),
                payload: Some(json!({"success": true, "result": values})),
            }),
            Err(error) => Ok(BrowserExecutionResult {
                runtime: "cdp_chromium".to_string(),
                exit_code: 1,
                duration_ms: started.elapsed().as_millis() as u64,
                stdout: String::new(),
                stderr: error.to_string(),
                payload: Some(json!({"success": false, "error": error.to_string()})),
            }),
        }
    }
}

struct CdpRuntime {
    process: ChromiumProcess,
    cdp: CdpClient,
    page_session_id: String,
}

impl CdpRuntime {
    async fn start(headless: bool, profile_dir: &str, timeout_secs: u64) -> Result<Self> {
        let process = ChromiumProcess::launch(headless, profile_dir, timeout_secs).await?;
        let mut cdp = CdpClient::connect(&process.ws_endpoint).await?;

        let create_result = cdp
            .send_command(None, "Target.createTarget", json!({"url": "about:blank"}))
            .await?;
        let target_id = create_result
            .get("targetId")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("CDP Target.createTarget did not return targetId"))?
            .to_string();

        let attach_result = cdp
            .send_command(
                None,
                "Target.attachToTarget",
                json!({"targetId": target_id, "flatten": true}),
            )
            .await?;
        let page_session_id = attach_result
            .get("sessionId")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("CDP Target.attachToTarget did not return sessionId"))?
            .to_string();

        cdp.send_command(Some(&page_session_id), "Runtime.enable", json!({}))
            .await?;
        cdp.send_command(Some(&page_session_id), "Page.enable", json!({}))
            .await?;
        cdp.send_command(
            Some(&page_session_id),
            "Page.setLifecycleEventsEnabled",
            json!({"enabled": true}),
        )
        .await?;
        cdp.send_command(Some(&page_session_id), "Network.enable", json!({}))
            .await?;

        Ok(Self {
            process,
            cdp,
            page_session_id,
        })
    }

    async fn shutdown(mut self) -> Result<()> {
        let _ = self
            .cdp
            .send_command(None, "Browser.close", json!({}))
            .await;
        self.process.shutdown().await
    }

    async fn execute_action(
        &mut self,
        action: &BrowserAction,
        artifacts_dir: &str,
    ) -> Result<Value> {
        match action {
            BrowserAction::Navigate { url, wait_until } => {
                let result = self
                    .cdp
                    .send_command(
                        Some(&self.page_session_id),
                        "Page.navigate",
                        json!({"url": url}),
                    )
                    .await?;

                if let Some(error_text) = result.get("errorText").and_then(Value::as_str) {
                    bail!("Navigation failed: {}", error_text);
                }

                let state = wait_until.as_deref().unwrap_or("load").to_ascii_lowercase();
                if state != "commit" {
                    self.wait_for_readiness(&state, Duration::from_secs(30))
                        .await?;
                }

                Ok(json!({"type": "navigate", "url": url}))
            }
            BrowserAction::Click {
                selector,
                timeout_ms,
            } => {
                self.wait_for_selector(selector, "visible", timeout_ms.unwrap_or(10_000))
                    .await?;
                let script = format!(
                    "(function() {{\n  const selector = {};\n  const element = document.querySelector(selector);\n  if (!element) return {{ ok: false, error: `Selector not found: ${{selector}}` }};\n  element.click();\n  return {{ ok: true }};\n}})()",
                    serde_json::to_string(selector)?
                );
                let result = self.evaluate_page_script(&script).await?;
                extract_action_result(result)?;
                Ok(json!({"type": "click", "selector": selector}))
            }
            BrowserAction::Fill {
                selector,
                text,
                timeout_ms,
            } => {
                self.wait_for_selector(selector, "visible", timeout_ms.unwrap_or(10_000))
                    .await?;
                let script = format!(
                    "(function() {{\n  const selector = {};\n  const value = {};\n  const element = document.querySelector(selector);\n  if (!element) return {{ ok: false, error: `Selector not found: ${{selector}}` }};\n  element.focus?.();\n  element.value = value;\n  element.dispatchEvent(new Event('input', {{ bubbles: true }}));\n  element.dispatchEvent(new Event('change', {{ bubbles: true }}));\n  return {{ ok: true }};\n}})()",
                    serde_json::to_string(selector)?,
                    serde_json::to_string(text)?
                );
                let result = self.evaluate_page_script(&script).await?;
                extract_action_result(result)?;
                Ok(json!({"type": "fill", "selector": selector}))
            }
            BrowserAction::Type {
                selector,
                text,
                delay_ms,
            } => {
                let _ = delay_ms;
                self.wait_for_selector(selector, "visible", 10_000).await?;
                let script = format!(
                    "(function() {{\n  const selector = {};\n  const value = {};\n  const element = document.querySelector(selector);\n  if (!element) return {{ ok: false, error: `Selector not found: ${{selector}}` }};\n  element.focus?.();\n  const existing = typeof element.value === 'string' ? element.value : '';\n  element.value = existing + value;\n  element.dispatchEvent(new Event('input', {{ bubbles: true }}));\n  element.dispatchEvent(new Event('change', {{ bubbles: true }}));\n  return {{ ok: true }};\n}})()",
                    serde_json::to_string(selector)?,
                    serde_json::to_string(text)?
                );
                let result = self.evaluate_page_script(&script).await?;
                extract_action_result(result)?;
                Ok(json!({"type": "type", "selector": selector}))
            }
            BrowserAction::Press { key, selector } => {
                if let Some(selector) = selector {
                    self.focus_selector(selector, 10_000).await?;
                }

                self.dispatch_key_down(key, &[]).await?;
                self.dispatch_key_up(key, &[]).await?;
                Ok(json!({"type": "press", "key": key}))
            }
            BrowserAction::KeyDown {
                key,
                selector,
                modifiers,
            } => {
                if let Some(selector) = selector {
                    self.focus_selector(selector, 10_000).await?;
                }

                self.dispatch_key_down(key, modifiers).await?;
                Ok(json!({
                    "type": "key_down",
                    "key": key,
                    "modifiers": modifiers
                }))
            }
            BrowserAction::KeyUp {
                key,
                selector,
                modifiers,
            } => {
                if let Some(selector) = selector {
                    self.focus_selector(selector, 10_000).await?;
                }

                self.dispatch_key_up(key, modifiers).await?;
                Ok(json!({
                    "type": "key_up",
                    "key": key,
                    "modifiers": modifiers
                }))
            }
            BrowserAction::MouseMove { x, y, modifiers } => {
                self.dispatch_mouse_move(*x, *y, modifiers).await?;
                Ok(json!({
                    "type": "mouse_move",
                    "x": x,
                    "y": y,
                    "modifiers": modifiers
                }))
            }
            BrowserAction::MouseDown {
                x,
                y,
                button,
                click_count,
                modifiers,
            } => {
                self.dispatch_mouse_down(*x, *y, *button, *click_count, modifiers)
                    .await?;
                Ok(json!({
                    "type": "mouse_down",
                    "x": x,
                    "y": y,
                    "button": button,
                    "click_count": click_count,
                    "modifiers": modifiers
                }))
            }
            BrowserAction::MouseUp {
                x,
                y,
                button,
                click_count,
                modifiers,
            } => {
                self.dispatch_mouse_up(*x, *y, *button, *click_count, modifiers)
                    .await?;
                Ok(json!({
                    "type": "mouse_up",
                    "x": x,
                    "y": y,
                    "button": button,
                    "click_count": click_count,
                    "modifiers": modifiers
                }))
            }
            BrowserAction::MouseClick {
                x,
                y,
                button,
                click_count,
                modifiers,
            } => {
                self.dispatch_mouse_down(*x, *y, *button, *click_count, modifiers)
                    .await?;
                self.dispatch_mouse_up(*x, *y, *button, *click_count, modifiers)
                    .await?;
                Ok(json!({
                    "type": "mouse_click",
                    "x": x,
                    "y": y,
                    "button": button,
                    "click_count": click_count,
                    "modifiers": modifiers
                }))
            }
            BrowserAction::MouseWheel {
                x,
                y,
                delta_x,
                delta_y,
                modifiers,
            } => {
                self.dispatch_mouse_wheel(*x, *y, *delta_x, *delta_y, modifiers)
                    .await?;
                Ok(json!({
                    "type": "mouse_wheel",
                    "x": x,
                    "y": y,
                    "delta_x": delta_x,
                    "delta_y": delta_y,
                    "modifiers": modifiers
                }))
            }
            BrowserAction::WaitForSelector {
                selector,
                state,
                timeout_ms,
            } => {
                let state = state.as_deref().unwrap_or("visible");
                self.wait_for_selector(selector, state, timeout_ms.unwrap_or(10_000))
                    .await?;
                Ok(json!({"type": "wait_for_selector", "selector": selector, "state": state}))
            }
            BrowserAction::ExtractText { selector, all } => {
                let script = format!(
                    "(function() {{\n  const selector = {};\n  if ({}) {{\n    const values = Array.from(document.querySelectorAll(selector)).map(e => e.textContent ?? '');\n    return {{ ok: true, value: values }};\n  }}\n  const element = document.querySelector(selector);\n  if (!element) return {{ ok: false, error: `Selector not found: ${{selector}}` }};\n  return {{ ok: true, value: element.textContent ?? null }};\n}})()",
                    serde_json::to_string(selector)?,
                    if *all { "true" } else { "false" }
                );
                let result = self.evaluate_page_script(&script).await?;
                let value = extract_action_result(result)?;
                Ok(json!({"type": "extract_text", "selector": selector, "value": value}))
            }
            BrowserAction::Screenshot { path, full_page } => {
                let target = resolve_artifact_path(artifacts_dir, path);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let result = self
                    .cdp
                    .send_command(
                        Some(&self.page_session_id),
                        "Page.captureScreenshot",
                        json!({
                            "format": "png",
                            "captureBeyondViewport": full_page,
                            "fromSurface": true
                        }),
                    )
                    .await?;

                let data = result
                    .get("data")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("Page.captureScreenshot did not return image data"))?;
                let bytes = BASE64_STANDARD
                    .decode(data)
                    .map_err(|error| anyhow!("Failed to decode screenshot data: {}", error))?;
                std::fs::write(&target, bytes)?;

                Ok(json!({
                    "type": "screenshot",
                    "path": target.display().to_string()
                }))
            }
            BrowserAction::Evaluate { expression } => {
                let script = build_dynamic_eval_script(expression)?;
                let result = self.evaluate_page_script(&script).await?;
                let value = extract_action_result(result)?;
                Ok(json!({"type": "evaluate", "value": value}))
            }
        }
    }

    async fn evaluate_page_script(&mut self, expression: &str) -> Result<Value> {
        let result = self
            .cdp
            .send_command(
                Some(&self.page_session_id),
                "Runtime.evaluate",
                json!({
                    "expression": expression,
                    "awaitPromise": true,
                    "returnByValue": true,
                    "replMode": false,
                }),
            )
            .await?;

        if let Some(exception) = result.get("exceptionDetails") {
            let message = exception
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("JavaScript execution failed")
                .to_string();
            bail!("{}", message);
        }

        let remote = result.get("result").cloned().unwrap_or(Value::Null);
        if let Some(value) = remote.get("value") {
            return Ok(value.clone());
        }

        if remote.get("type").and_then(Value::as_str) == Some("undefined") {
            return Ok(Value::Null);
        }

        if let Some(description) = remote.get("description").and_then(Value::as_str) {
            return Ok(Value::String(description.to_string()));
        }

        Ok(Value::Null)
    }

    async fn wait_for_readiness(
        &mut self,
        wait_until: &str,
        timeout_window: Duration,
    ) -> Result<()> {
        let start = Instant::now();
        let mut readiness = NavigationReadinessState::default();
        let mut last_network_activity = Instant::now();
        let state = wait_until.to_ascii_lowercase();

        loop {
            let ready_state = self
                .evaluate_page_script("document.readyState")
                .await
                .unwrap_or(Value::String("loading".to_string()));
            let ready_state = ready_state.as_str().unwrap_or("loading");

            update_readiness_from_document_state(&mut readiness, ready_state);

            let network_idle = readiness.saw_load
                && readiness.inflight_requests == 0
                && last_network_activity.elapsed() >= Duration::from_millis(NETWORK_IDLE_GRACE_MS);

            let done = match state.as_str() {
                "domcontentloaded" => readiness.saw_domcontentloaded,
                "networkidle" => network_idle,
                _ => readiness.saw_load,
            };

            if done {
                return Ok(());
            }

            if start.elapsed() > timeout_window {
                bail!("Timed out waiting for page readiness ({})", wait_until);
            }

            let remaining = timeout_window.saturating_sub(start.elapsed());
            let event_timeout = remaining.min(Duration::from_millis(CDP_POLL_INTERVAL_MS));

            if let Some(event) = self.cdp.poll_event(event_timeout).await?
                && apply_navigation_event(&mut readiness, &event, &self.page_session_id)
            {
                last_network_activity = Instant::now();
            }
        }
    }

    async fn wait_for_selector(
        &mut self,
        selector: &str,
        state: &str,
        timeout_ms: u64,
    ) -> Result<()> {
        let start = Instant::now();
        let timeout_window = Duration::from_millis(timeout_ms.max(1));

        loop {
            let script = format!(
                "(function() {{\n  const selector = {};\n  const element = document.querySelector(selector);\n  const present = !!element;\n  let visible = false;\n  if (element) {{\n    const style = window.getComputedStyle(element);\n    const rect = element.getBoundingClientRect();\n    visible = style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;\n  }}\n  return {{ present, visible }};\n}})()",
                serde_json::to_string(selector)?
            );

            let result = self.evaluate_page_script(&script).await?;
            let present = result
                .get("present")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let visible = result
                .get("visible")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            if selector_state_matches(state, present, visible) {
                return Ok(());
            }

            if start.elapsed() > timeout_window {
                bail!(
                    "Timed out waiting for selector '{}' in state '{}'",
                    selector,
                    state
                );
            }

            sleep(Duration::from_millis(CDP_POLL_INTERVAL_MS)).await;
        }
    }

    async fn focus_selector(&mut self, selector: &str, timeout_ms: u64) -> Result<()> {
        self.wait_for_selector(selector, "visible", timeout_ms)
            .await?;
        let focus_script = format!(
            "(function() {{\n  const selector = {};\n  const element = document.querySelector(selector);\n  if (!element) return {{ ok: false, error: `Selector not found: ${{selector}}` }};\n  element.focus?.();\n  return {{ ok: true }};\n}})()",
            serde_json::to_string(selector)?
        );
        let result = self.evaluate_page_script(&focus_script).await?;
        extract_action_result(result)?;
        Ok(())
    }

    async fn dispatch_key_down(&mut self, key: &str, modifiers: &[InputModifier]) -> Result<()> {
        self.dispatch_key_event("keyDown", key, modifiers, true)
            .await
    }

    async fn dispatch_key_up(&mut self, key: &str, modifiers: &[InputModifier]) -> Result<()> {
        self.dispatch_key_event("keyUp", key, modifiers, false)
            .await
    }

    async fn dispatch_key_event(
        &mut self,
        event_type: &str,
        key: &str,
        modifiers: &[InputModifier],
        include_text: bool,
    ) -> Result<()> {
        let descriptor = cdp_key_descriptor(key);
        let mut params = serde_json::Map::new();
        params.insert("type".to_string(), Value::String(event_type.to_string()));
        params.insert("key".to_string(), Value::String(descriptor.key.clone()));
        params.insert("code".to_string(), Value::String(descriptor.code));
        params.insert(
            "windowsVirtualKeyCode".to_string(),
            Value::from(descriptor.virtual_key_code),
        );
        params.insert(
            "nativeVirtualKeyCode".to_string(),
            Value::from(descriptor.virtual_key_code),
        );
        params.insert(
            "modifiers".to_string(),
            Value::from(modifier_mask(modifiers)),
        );
        if include_text && let Some(text) = descriptor.text {
            params.insert("text".to_string(), Value::String(text.clone()));
            params.insert("unmodifiedText".to_string(), Value::String(text));
        }

        self.cdp
            .send_command(
                Some(&self.page_session_id),
                "Input.dispatchKeyEvent",
                Value::Object(params),
            )
            .await?;
        Ok(())
    }

    async fn dispatch_mouse_move(
        &mut self,
        x: f64,
        y: f64,
        modifiers: &[InputModifier],
    ) -> Result<()> {
        self.dispatch_mouse_event("mouseMoved", (x, y), MouseButton::Left, 0, modifiers, None)
            .await
    }

    async fn dispatch_mouse_down(
        &mut self,
        x: f64,
        y: f64,
        button: MouseButton,
        click_count: u32,
        modifiers: &[InputModifier],
    ) -> Result<()> {
        self.dispatch_mouse_event(
            "mousePressed",
            (x, y),
            button,
            click_count.max(1),
            modifiers,
            None,
        )
        .await
    }

    async fn dispatch_mouse_up(
        &mut self,
        x: f64,
        y: f64,
        button: MouseButton,
        click_count: u32,
        modifiers: &[InputModifier],
    ) -> Result<()> {
        self.dispatch_mouse_event(
            "mouseReleased",
            (x, y),
            button,
            click_count.max(1),
            modifiers,
            None,
        )
        .await
    }

    async fn dispatch_mouse_wheel(
        &mut self,
        x: f64,
        y: f64,
        delta_x: f64,
        delta_y: f64,
        modifiers: &[InputModifier],
    ) -> Result<()> {
        self.dispatch_mouse_event(
            "mouseWheel",
            (x, y),
            MouseButton::Left,
            0,
            modifiers,
            Some((delta_x, delta_y)),
        )
        .await
    }

    async fn dispatch_mouse_event(
        &mut self,
        event_type: &str,
        position: (f64, f64),
        button: MouseButton,
        click_count: u32,
        modifiers: &[InputModifier],
        wheel_delta: Option<(f64, f64)>,
    ) -> Result<()> {
        let (x, y) = position;
        let mut params = serde_json::Map::new();
        params.insert("type".to_string(), Value::String(event_type.to_string()));
        params.insert("x".to_string(), Value::from(x));
        params.insert("y".to_string(), Value::from(y));
        params.insert(
            "modifiers".to_string(),
            Value::from(modifier_mask(modifiers)),
        );

        match event_type {
            "mouseMoved" => {
                params.insert("button".to_string(), Value::String("none".to_string()));
                params.insert("buttons".to_string(), Value::from(0));
            }
            "mouseWheel" => {
                params.insert("button".to_string(), Value::String("none".to_string()));
                params.insert("buttons".to_string(), Value::from(0));
                if let Some((delta_x, delta_y)) = wheel_delta {
                    params.insert("deltaX".to_string(), Value::from(delta_x));
                    params.insert("deltaY".to_string(), Value::from(delta_y));
                }
            }
            _ => {
                params.insert(
                    "button".to_string(),
                    Value::String(mouse_button_name(button).to_string()),
                );
                params.insert(
                    "buttons".to_string(),
                    Value::from(mouse_button_mask(button)),
                );
                params.insert("clickCount".to_string(), Value::from(click_count.max(1)));
            }
        }

        self.cdp
            .send_command(
                Some(&self.page_session_id),
                "Input.dispatchMouseEvent",
                Value::Object(params),
            )
            .await?;
        Ok(())
    }
}

struct ChromiumProcess {
    child: Child,
    ws_endpoint: String,
}

impl ChromiumProcess {
    async fn launch(headless: bool, profile_dir: &str, timeout_secs: u64) -> Result<Self> {
        let chromium = resolve_chromium_binary()
            .ok_or_else(|| anyhow!("Chromium executable not found. Set RESTFLOW_CHROMIUM_PATH"))?;
        let debug_port = allocate_free_port()?;

        let mut args = vec![
            format!("--remote-debugging-port={}", debug_port),
            format!("--user-data-dir={}", profile_dir),
            "--no-first-run".to_string(),
            "--no-default-browser-check".to_string(),
            "--disable-background-networking".to_string(),
            "--disable-popup-blocking".to_string(),
            "--disable-dev-shm-usage".to_string(),
            "about:blank".to_string(),
        ];

        if headless {
            args.push("--headless=new".to_string());
            args.push("--hide-scrollbars".to_string());
        }

        if cfg!(target_os = "linux") {
            args.push("--no-sandbox".to_string());
        }

        let mut command = Command::new(&chromium);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);

        let mut child = command.spawn().map_err(|error| {
            anyhow!(
                "Failed to launch chromium executable '{}': {}",
                chromium,
                error
            )
        })?;

        let ws_endpoint = wait_for_debugger_ws_url(debug_port, timeout_secs, &mut child).await?;

        Ok(Self { child, ws_endpoint })
    }

    async fn shutdown(&mut self) -> Result<()> {
        let wait_result = timeout(
            Duration::from_secs(CDP_SHUTDOWN_TIMEOUT_SECS),
            self.child.wait(),
        )
        .await;

        match wait_result {
            Ok(_) => Ok(()),
            Err(_) => {
                self.child.kill().await?;
                Ok(())
            }
        }
    }
}

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

struct CdpClient {
    socket: WsStream,
    next_id: i64,
    queued_events: VecDeque<Value>,
    queued_responses: HashMap<i64, Value>,
}

impl CdpClient {
    async fn connect(ws_endpoint: &str) -> Result<Self> {
        let (socket, _) = connect_async(ws_endpoint)
            .await
            .map_err(|error| anyhow!("Failed to connect to CDP endpoint: {}", error))?;
        Ok(Self {
            socket,
            next_id: 0,
            queued_events: VecDeque::new(),
            queued_responses: HashMap::new(),
        })
    }

    async fn send_command(
        &mut self,
        session_id: Option<&str>,
        method: &str,
        params: Value,
    ) -> Result<Value> {
        self.next_id += 1;
        let request_id = self.next_id;

        let mut request = serde_json::Map::new();
        request.insert("id".to_string(), json!(request_id));
        request.insert("method".to_string(), Value::String(method.to_string()));
        request.insert("params".to_string(), params);
        if let Some(session_id) = session_id {
            request.insert(
                "sessionId".to_string(),
                Value::String(session_id.to_string()),
            );
        }

        self.socket
            .send(Message::Text(Value::Object(request).to_string().into()))
            .await
            .map_err(|error| anyhow!("Failed to send CDP command '{}': {}", method, error))?;

        if let Some(payload) = self.queued_responses.remove(&request_id) {
            return Self::extract_command_result(method, payload);
        }

        loop {
            let payload = self.read_json_message().await?;

            let Some(response_id) = payload.get("id").and_then(Value::as_i64) else {
                self.queued_events.push_back(payload);
                continue;
            };

            if response_id != request_id {
                self.queued_responses.insert(response_id, payload);
                continue;
            }

            return Self::extract_command_result(method, payload);
        }
    }

    async fn poll_event(&mut self, timeout_window: Duration) -> Result<Option<Value>> {
        if let Some(event) = self.queued_events.pop_front() {
            return Ok(Some(event));
        }

        let payload = match timeout(timeout_window, self.read_json_message()).await {
            Ok(result) => result?,
            Err(_) => return Ok(None),
        };

        if let Some(response_id) = payload.get("id").and_then(Value::as_i64) {
            self.queued_responses.insert(response_id, payload);
            return Ok(None);
        }

        Ok(Some(payload))
    }

    fn extract_command_result(method: &str, payload: Value) -> Result<Value> {
        if let Some(error) = payload.get("error") {
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Unknown CDP error")
                .to_string();
            bail!("CDP command '{}' failed: {}", method, message);
        }

        Ok(payload.get("result").cloned().unwrap_or_else(|| json!({})))
    }

    async fn read_json_message(&mut self) -> Result<Value> {
        loop {
            let message = self
                .socket
                .next()
                .await
                .ok_or_else(|| anyhow!("CDP websocket stream ended"))?
                .map_err(|error| anyhow!("CDP websocket read failed: {}", error))?;

            let text = match message {
                Message::Text(text) => text.to_string(),
                Message::Binary(bytes) => String::from_utf8(bytes.to_vec())
                    .map_err(|error| anyhow!("Invalid UTF-8 CDP payload: {}", error))?,
                Message::Ping(payload) => {
                    self.socket.send(Message::Pong(payload)).await?;
                    continue;
                }
                Message::Pong(_) => continue,
                Message::Close(_) => bail!("CDP websocket closed by peer"),
                Message::Frame(_) => continue,
            };

            let value = serde_json::from_str::<Value>(&text)
                .map_err(|error| anyhow!("Invalid CDP JSON payload: {}", error))?;
            return Ok(value);
        }
    }
}

fn cdp_event_matches_session(event: &Value, expected_session: &str) -> bool {
    match event.get("sessionId").and_then(Value::as_str) {
        Some(session_id) => session_id == expected_session,
        None => true,
    }
}

#[derive(Debug, Default)]
struct NavigationReadinessState {
    saw_domcontentloaded: bool,
    saw_load: bool,
    inflight_requests: usize,
    inflight_request_ids: HashSet<String>,
}

fn update_readiness_from_document_state(state: &mut NavigationReadinessState, ready_state: &str) {
    if ready_state == "complete" {
        state.saw_domcontentloaded = true;
        state.saw_load = true;
    } else if ready_state == "interactive" {
        state.saw_domcontentloaded = true;
    }
}

fn apply_navigation_event(
    state: &mut NavigationReadinessState,
    event: &Value,
    expected_session: &str,
) -> bool {
    if !cdp_event_matches_session(event, expected_session) {
        return false;
    }

    let Some(method) = event.get("method").and_then(Value::as_str) else {
        return false;
    };

    match method {
        "Page.lifecycleEvent" => {
            let lifecycle_name = event.get("params").and_then(|params| params.get("name"));
            match lifecycle_name.and_then(Value::as_str) {
                Some("DOMContentLoaded") => {
                    state.saw_domcontentloaded = true;
                    false
                }
                Some("load") => {
                    state.saw_domcontentloaded = true;
                    state.saw_load = true;
                    false
                }
                _ => false,
            }
        }
        "Network.requestWillBeSent" => {
            if let Some(request_id) = event
                .get("params")
                .and_then(|params| params.get("requestId"))
                .and_then(Value::as_str)
            {
                if state.inflight_request_ids.insert(request_id.to_string()) {
                    state.inflight_requests = state.inflight_requests.saturating_add(1);
                }
            } else {
                state.inflight_requests = state.inflight_requests.saturating_add(1);
            }
            true
        }
        "Network.loadingFinished" | "Network.loadingFailed" => {
            if let Some(request_id) = event
                .get("params")
                .and_then(|params| params.get("requestId"))
                .and_then(Value::as_str)
            {
                if state.inflight_request_ids.remove(request_id) {
                    state.inflight_requests = state.inflight_requests.saturating_sub(1);
                }
            } else {
                state.inflight_requests = state.inflight_requests.saturating_sub(1);
            }
            true
        }
        _ => false,
    }
}

async fn wait_for_debugger_ws_url(
    port: u16,
    timeout_secs: u64,
    child: &mut Child,
) -> Result<String> {
    let endpoint = format!("http://127.0.0.1:{}/json/version", port);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;

    let started = Instant::now();
    let timeout_window = Duration::from_secs(timeout_secs.max(1));

    loop {
        if let Some(status) = child.try_wait()? {
            bail!(
                "Chromium exited before CDP endpoint became available: {}",
                status
            );
        }

        if let Ok(response) = client.get(&endpoint).send().await
            && response.status().is_success()
        {
            let body: Value = response.json().await?;
            if let Some(ws_url) = body.get("webSocketDebuggerUrl").and_then(Value::as_str) {
                return Ok(ws_url.to_string());
            }
        }

        if started.elapsed() > timeout_window {
            bail!("Timed out waiting for CDP endpoint at {}", endpoint);
        }

        sleep(Duration::from_millis(CDP_POLL_INTERVAL_MS)).await;
    }
}

async fn transpile_typescript_source(
    source: &str,
    cwd: Option<&Path>,
    timeout_secs: u64,
) -> Result<String> {
    let source_json = serde_json::to_string(source)?;
    let transform_script = "import { stripTypeScriptTypes } from 'node:module';\nconst sourceJson = process.argv[process.argv.length - 1];\nif (!sourceJson) {\n  console.error('Missing TypeScript source argument');\n  process.exit(1);\n}\nconst source = JSON.parse(sourceJson);\nconst output = stripTypeScriptTypes(source, { mode: 'strip' });\nprocess.stdout.write(output);";
    let args = vec![
        "--experimental-strip-types".to_string(),
        "--input-type=module".to_string(),
        "-e".to_string(),
        transform_script.to_string(),
        source_json,
    ];
    let output = run_command_capture("node", &args, cwd, timeout_secs)
        .await
        .map_err(|error| {
            anyhow!(
                "TypeScript transpilation requires Node.js with '--experimental-strip-types': {}",
                error
            )
        })?;

    if output.exit_code != 0 {
        let stderr = output.stderr.trim();
        if !stderr.is_empty() {
            bail!("TypeScript transpilation failed: {}", stderr);
        }

        let stdout = output.stdout.trim();
        if !stdout.is_empty() {
            bail!("TypeScript transpilation failed: {}", stdout);
        }

        bail!(
            "TypeScript transpilation failed with exit code {}",
            output.exit_code
        );
    }

    if output.stdout.is_empty() && !source.is_empty() {
        bail!("TypeScript transpilation produced empty output");
    }

    Ok(output.stdout)
}

#[derive(Debug)]
struct CdpKeyDescriptor {
    key: String,
    code: String,
    virtual_key_code: u32,
    text: Option<String>,
}

fn cdp_key_descriptor(key: &str) -> CdpKeyDescriptor {
    fn descriptor(
        key: &str,
        code: &str,
        virtual_key_code: u32,
        text: Option<&str>,
    ) -> CdpKeyDescriptor {
        CdpKeyDescriptor {
            key: key.to_string(),
            code: code.to_string(),
            virtual_key_code,
            text: text.map(ToString::to_string),
        }
    }

    let normalized = key.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "enter" => descriptor("Enter", "Enter", 13, Some("\r")),
        "tab" => descriptor("Tab", "Tab", 9, Some("\t")),
        "backspace" => descriptor("Backspace", "Backspace", 8, None),
        "escape" | "esc" => descriptor("Escape", "Escape", 27, None),
        "delete" => descriptor("Delete", "Delete", 46, None),
        "insert" => descriptor("Insert", "Insert", 45, None),
        "home" => descriptor("Home", "Home", 36, None),
        "end" => descriptor("End", "End", 35, None),
        "pageup" => descriptor("PageUp", "PageUp", 33, None),
        "pagedown" => descriptor("PageDown", "PageDown", 34, None),
        "arrowleft" | "left" => descriptor("ArrowLeft", "ArrowLeft", 37, None),
        "arrowup" | "up" => descriptor("ArrowUp", "ArrowUp", 38, None),
        "arrowright" | "right" => descriptor("ArrowRight", "ArrowRight", 39, None),
        "arrowdown" | "down" => descriptor("ArrowDown", "ArrowDown", 40, None),
        "space" => descriptor(" ", "Space", 32, Some(" ")),
        "shift" | "shiftleft" | "shiftright" => descriptor("Shift", "ShiftLeft", 16, None),
        "control" | "ctrl" | "controlleft" | "controlright" => {
            descriptor("Control", "ControlLeft", 17, None)
        }
        "alt" | "altleft" | "altright" => descriptor("Alt", "AltLeft", 18, None),
        "meta" | "command" | "cmd" | "metaleft" | "metaright" => {
            descriptor("Meta", "MetaLeft", 91, None)
        }
        _ => {
            if key.chars().count() == 1 {
                let ch = key.chars().next().unwrap_or_default();
                if ch.is_ascii_alphabetic() {
                    let upper = ch.to_ascii_uppercase();
                    return CdpKeyDescriptor {
                        key: ch.to_string(),
                        code: format!("Key{}", upper),
                        virtual_key_code: upper as u32,
                        text: Some(ch.to_string()),
                    };
                }

                if ch.is_ascii_digit() {
                    return CdpKeyDescriptor {
                        key: ch.to_string(),
                        code: format!("Digit{}", ch),
                        virtual_key_code: ch as u32,
                        text: Some(ch.to_string()),
                    };
                }

                let code = match ch {
                    '.' => "Period",
                    ',' => "Comma",
                    '/' => "Slash",
                    ';' => "Semicolon",
                    '\'' => "Quote",
                    '[' => "BracketLeft",
                    ']' => "BracketRight",
                    '\\' => "Backslash",
                    '-' => "Minus",
                    '=' => "Equal",
                    '`' => "Backquote",
                    _ => "Unidentified",
                };
                let text = if ch.is_control() {
                    None
                } else {
                    Some(ch.to_string())
                };

                return CdpKeyDescriptor {
                    key: ch.to_string(),
                    code: code.to_string(),
                    virtual_key_code: 0,
                    text,
                };
            }

            CdpKeyDescriptor {
                key: key.to_string(),
                code: key.to_string(),
                virtual_key_code: 0,
                text: None,
            }
        }
    }
}

fn modifier_mask(modifiers: &[InputModifier]) -> u8 {
    modifiers.iter().fold(0u8, |mask, modifier| {
        mask | match modifier {
            InputModifier::Alt => 1,
            InputModifier::Control => 2,
            InputModifier::Meta => 4,
            InputModifier::Shift => 8,
        }
    })
}

fn mouse_button_name(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "left",
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
        MouseButton::Back => "back",
        MouseButton::Forward => "forward",
    }
}

fn mouse_button_mask(button: MouseButton) -> u8 {
    match button {
        MouseButton::Left => 1,
        MouseButton::Right => 2,
        MouseButton::Middle => 4,
        MouseButton::Back => 8,
        MouseButton::Forward => 16,
    }
}

fn build_user_script_wrapper(source: &str) -> Result<String> {
    let source = serde_json::to_string(source)?;
    Ok(format!(
        "(async () => {{\n  const rf = {{\n    url: () => location.href,\n    title: () => document.title,\n    text: (selector) => document.querySelector(selector)?.textContent ?? null,\n    click: (selector) => {{ const el = document.querySelector(selector); if (!el) throw new Error(`Selector not found: ${{selector}}`); el.click(); return true; }},\n    fill: (selector, value) => {{ const el = document.querySelector(selector); if (!el) throw new Error(`Selector not found: ${{selector}}`); el.focus?.(); el.value = value; el.dispatchEvent(new Event('input', {{ bubbles: true }})); el.dispatchEvent(new Event('change', {{ bubbles: true }})); return true; }},\n  }};\n  let __restflowResult = null;\n  const setRestflowResult = (value) => {{ __restflowResult = value; }};\n  const __source = {};\n  const __userMain = new Function('rf', 'setRestflowResult', `return (async () => {{\\n${{__source}}\\n}})();`);\n  const returned = await __userMain(rf, setRestflowResult);\n  return {{ ok: true, value: returned === undefined ? __restflowResult : returned }};\n}})()",
        source
    ))
}

fn build_dynamic_eval_script(source: &str) -> Result<String> {
    let source = serde_json::to_string(source)?;
    Ok(format!(
        "(async () => {{\n  const __source = {};\n  try {{\n    const expressionResult = await (0, eval)('(' + __source + ')');\n    return {{ ok: true, value: expressionResult }};\n  }} catch (_ignored) {{}}\n  try {{\n    const statementResult = await (0, eval)(__source);\n    return {{ ok: true, value: statementResult }};\n  }} catch (error) {{\n    return {{ ok: false, error: error?.stack ?? String(error) }};\n  }}\n}})()",
        source
    ))
}

fn extract_action_result(value: Value) -> Result<Value> {
    if let Some(ok) = value.get("ok").and_then(Value::as_bool) {
        if ok {
            return Ok(value.get("value").cloned().unwrap_or(Value::Null));
        }

        let message = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("Unknown browser action error");
        bail!("{}", message);
    }

    Ok(value)
}

fn selector_state_matches(state: &str, present: bool, visible: bool) -> bool {
    match state {
        "attached" => present,
        "detached" => !present,
        "hidden" => !present || !visible,
        _ => present && visible,
    }
}

fn resolve_artifact_path(artifacts_dir: &str, path: &str) -> PathBuf {
    let target = PathBuf::from(path);
    if target.is_absolute() {
        target
    } else {
        PathBuf::from(artifacts_dir).join(target)
    }
}

async fn run_command_capture(
    program: &str,
    args: &[String],
    cwd: Option<&Path>,
    timeout_secs: u64,
) -> Result<CommandCapture> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }

    let output = match timeout(Duration::from_secs(timeout_secs), command.output()).await {
        Ok(result) => result?,
        Err(_) => bail!("Command timed out after {} seconds", timeout_secs),
    };

    Ok(CommandCapture {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

struct CommandCapture {
    exit_code: i32,
    stdout: String,
    stderr: String,
}

fn resolve_chromium_binary() -> Option<String> {
    let env_candidates = ["RESTFLOW_CHROMIUM_PATH", "CHROMIUM_PATH", "CHROME_PATH"];
    for key in env_candidates {
        if let Ok(value) = std::env::var(key)
            && !value.trim().is_empty()
        {
            let path = PathBuf::from(value.trim());
            if path.exists() {
                return Some(path.display().to_string());
            }
        }
    }

    if cfg!(target_os = "macos") {
        let app_paths = [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        ];
        for path in app_paths {
            if Path::new(path).exists() {
                return Some(path.to_string());
            }
        }
    }

    if cfg!(target_os = "windows") {
        let windows_paths = [
            r"C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
            r"C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
            r"C:\\Program Files\\Chromium\\Application\\chrome.exe",
        ];
        for path in windows_paths {
            if Path::new(path).exists() {
                return Some(path.to_string());
            }
        }
    }

    let command_candidates = [
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
        "chrome",
        "msedge",
    ];

    for name in command_candidates {
        if is_executable_in_path(name) {
            return Some(name.to_string());
        }
    }

    None
}

fn is_executable_in_path(name: &str) -> bool {
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };

    for path in std::env::split_paths(&path_var) {
        let candidate = path.join(name);
        if candidate.exists() {
            return true;
        }
        if cfg!(target_os = "windows") {
            let exe_candidate = path.join(format!("{}.exe", name));
            if exe_candidate.exists() {
                return true;
            }
        }
    }

    false
}

fn allocate_free_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

fn resolve_default_root_dir() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("RESTFLOW_BROWSER_DIR") {
        return Ok(PathBuf::from(path));
    }

    let base = std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir());
    Ok(base.join(".restflow-browser"))
}

fn default_timeout_secs() -> u64 {
    DEFAULT_TIMEOUT_SECS
}

fn default_click_count() -> u32 {
    1
}

fn default_headless() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::tempdir;

    #[derive(Default)]
    struct MockExecutor {
        script_calls: AtomicUsize,
        action_calls: AtomicUsize,
    }

    #[async_trait]
    impl BrowserExecutor for MockExecutor {
        async fn probe_runtime(&self) -> Result<RuntimeProbe> {
            Ok(RuntimeProbe {
                node_available: true,
                node_version: Some("v25.0.0".to_string()),
                node_typescript_available: true,
                playwright_package_available: true,
                chromium_cache_detected: true,
                ready: true,
                notes: Vec::new(),
            })
        }

        async fn run_script(
            &self,
            _session: &BrowserSession,
            _request: &RunScriptRequest,
        ) -> Result<BrowserExecutionResult> {
            self.script_calls.fetch_add(1, Ordering::Relaxed);
            Ok(BrowserExecutionResult {
                runtime: "mock".to_string(),
                exit_code: 0,
                duration_ms: 2,
                stdout: String::new(),
                stderr: String::new(),
                payload: Some(json!({"success": true, "result": {"kind": "script"}})),
            })
        }

        async fn run_actions(
            &self,
            _session: &BrowserSession,
            _request: &RunActionsRequest,
        ) -> Result<BrowserExecutionResult> {
            self.action_calls.fetch_add(1, Ordering::Relaxed);
            Ok(BrowserExecutionResult {
                runtime: "mock".to_string(),
                exit_code: 0,
                duration_ms: 3,
                stdout: String::new(),
                stderr: String::new(),
                payload: Some(json!({"success": true, "result": [{"ok": true}]})),
            })
        }
    }

    #[tokio::test]
    async fn session_lifecycle_works() {
        let temp = tempdir().unwrap();
        let service = BrowserService::new_with_executor(
            temp.path().join("browser"),
            Arc::new(MockExecutor::default()),
        )
        .unwrap();

        let session = service
            .new_session(NewSessionRequest::default())
            .await
            .unwrap();
        assert_eq!(session.browser, BrowserKind::Chromium);

        let sessions = service.list_sessions().await;
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, session.id);

        let closed = service.close_session(&session.id).await.unwrap();
        assert!(closed);
        assert!(service.list_sessions().await.is_empty());
    }

    #[tokio::test]
    async fn run_script_forwards_to_executor() {
        let temp = tempdir().unwrap();
        let executor = Arc::new(MockExecutor::default());
        let service =
            BrowserService::new_with_executor(temp.path().join("browser"), executor.clone())
                .unwrap();

        let session = service
            .new_session(NewSessionRequest::default())
            .await
            .unwrap();
        let output = service
            .run_script(&RunScriptRequest {
                session_id: session.id,
                code: "setRestflowResult({ ok: true });".to_string(),
                language: ScriptLanguage::Js,
                runtime: ScriptRuntime::Auto,
                timeout_secs: 30,
                cwd: None,
            })
            .await
            .unwrap();

        assert_eq!(output.runtime, "mock");
        assert_eq!(executor.script_calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn run_actions_requires_existing_session() {
        let temp = tempdir().unwrap();
        let service = BrowserService::new_with_executor(
            temp.path().join("browser"),
            Arc::new(MockExecutor::default()),
        )
        .unwrap();

        let result = service
            .run_actions(&RunActionsRequest {
                session_id: "missing".to_string(),
                actions: vec![BrowserAction::Navigate {
                    url: "https://example.com".to_string(),
                    wait_until: None,
                }],
                runtime: ScriptRuntime::Auto,
                timeout_secs: 30,
                cwd: None,
            })
            .await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Session not found")
        );
    }

    #[test]
    fn selector_state_matching_behaves_as_expected() {
        assert!(selector_state_matches("attached", true, false));
        assert!(selector_state_matches("detached", false, false));
        assert!(selector_state_matches("visible", true, true));
        assert!(selector_state_matches("hidden", false, false));
        assert!(selector_state_matches("hidden", true, false));
        assert!(!selector_state_matches("visible", true, false));
    }

    #[test]
    fn browser_action_deserializes_extended_input_variants() {
        let key_down: BrowserAction = serde_json::from_value(json!({
            "type": "key_down",
            "key": "a",
            "modifiers": ["control", "shift"]
        }))
        .unwrap();
        match key_down {
            BrowserAction::KeyDown {
                key,
                selector,
                modifiers,
            } => {
                assert_eq!(key, "a");
                assert!(selector.is_none());
                assert_eq!(
                    modifiers,
                    vec![InputModifier::Control, InputModifier::Shift]
                );
            }
            _ => panic!("expected key_down action"),
        }

        let click: BrowserAction = serde_json::from_value(json!({
            "type": "mouse_click",
            "x": 100.0,
            "y": 50.0
        }))
        .unwrap();
        match click {
            BrowserAction::MouseClick {
                x,
                y,
                button,
                click_count,
                modifiers,
            } => {
                assert_eq!(x, 100.0);
                assert_eq!(y, 50.0);
                assert_eq!(button, MouseButton::Left);
                assert_eq!(click_count, 1);
                assert!(modifiers.is_empty());
            }
            _ => panic!("expected mouse_click action"),
        }
    }

    #[test]
    fn modifier_masks_match_cdp_bitflags() {
        assert_eq!(modifier_mask(&[]), 0);
        assert_eq!(modifier_mask(&[InputModifier::Alt]), 1);
        assert_eq!(modifier_mask(&[InputModifier::Control]), 2);
        assert_eq!(modifier_mask(&[InputModifier::Meta]), 4);
        assert_eq!(modifier_mask(&[InputModifier::Shift]), 8);
        assert_eq!(
            modifier_mask(&[InputModifier::Control, InputModifier::Shift]),
            10
        );
    }

    #[test]
    fn key_descriptors_cover_special_and_printable_keys() {
        let enter = cdp_key_descriptor("Enter");
        assert_eq!(enter.key, "Enter");
        assert_eq!(enter.code, "Enter");
        assert_eq!(enter.virtual_key_code, 13);
        assert_eq!(enter.text, Some("\r".to_string()));

        let alpha = cdp_key_descriptor("a");
        assert_eq!(alpha.key, "a");
        assert_eq!(alpha.code, "KeyA");
        assert_eq!(alpha.virtual_key_code, 65);
        assert_eq!(alpha.text, Some("a".to_string()));
    }

    #[test]
    fn mouse_button_helpers_match_cdp_values() {
        assert_eq!(mouse_button_name(MouseButton::Left), "left");
        assert_eq!(mouse_button_name(MouseButton::Right), "right");
        assert_eq!(mouse_button_name(MouseButton::Middle), "middle");
        assert_eq!(mouse_button_name(MouseButton::Back), "back");
        assert_eq!(mouse_button_name(MouseButton::Forward), "forward");

        assert_eq!(mouse_button_mask(MouseButton::Left), 1);
        assert_eq!(mouse_button_mask(MouseButton::Right), 2);
        assert_eq!(mouse_button_mask(MouseButton::Middle), 4);
        assert_eq!(mouse_button_mask(MouseButton::Back), 8);
        assert_eq!(mouse_button_mask(MouseButton::Forward), 16);
    }

    #[test]
    fn artifact_path_resolves_relative_and_absolute() {
        let relative = resolve_artifact_path("/tmp/artifacts", "shots/page.png");
        assert_eq!(relative, PathBuf::from("/tmp/artifacts/shots/page.png"));

        let absolute = resolve_artifact_path("/tmp/artifacts", "/var/tmp/page.png");
        assert_eq!(absolute, PathBuf::from("/var/tmp/page.png"));
    }

    #[test]
    fn dynamic_eval_script_contains_user_source() {
        let script = build_dynamic_eval_script("1 + 2").unwrap();
        assert!(script.contains("1 + 2"));
        assert!(script.contains("eval"));
    }

    #[test]
    fn cdp_event_session_matching_behaves_as_expected() {
        let sessioned = json!({"sessionId": "abc", "method": "Page.loadEventFired"});
        let without_session = json!({"method": "Browser.downloadProgress"});

        assert!(cdp_event_matches_session(&sessioned, "abc"));
        assert!(!cdp_event_matches_session(&sessioned, "xyz"));
        assert!(cdp_event_matches_session(&without_session, "xyz"));
    }

    #[test]
    fn update_readiness_tracks_document_ready_state() {
        let mut state = NavigationReadinessState::default();

        update_readiness_from_document_state(&mut state, "loading");
        assert!(!state.saw_domcontentloaded);
        assert!(!state.saw_load);

        update_readiness_from_document_state(&mut state, "interactive");
        assert!(state.saw_domcontentloaded);
        assert!(!state.saw_load);

        update_readiness_from_document_state(&mut state, "complete");
        assert!(state.saw_domcontentloaded);
        assert!(state.saw_load);
    }

    #[test]
    fn apply_navigation_event_tracks_lifecycle_and_network() {
        let mut state = NavigationReadinessState::default();

        let dom_event = json!({
            "sessionId": "page-session",
            "method": "Page.lifecycleEvent",
            "params": {"name": "DOMContentLoaded"}
        });
        assert!(!apply_navigation_event(
            &mut state,
            &dom_event,
            "page-session"
        ));
        assert!(state.saw_domcontentloaded);
        assert!(!state.saw_load);

        let request_event = json!({
            "sessionId": "page-session",
            "method": "Network.requestWillBeSent"
        });
        assert!(apply_navigation_event(
            &mut state,
            &request_event,
            "page-session"
        ));
        assert_eq!(state.inflight_requests, 1);

        let load_event = json!({
            "sessionId": "page-session",
            "method": "Page.lifecycleEvent",
            "params": {"name": "load"}
        });
        assert!(!apply_navigation_event(
            &mut state,
            &load_event,
            "page-session"
        ));
        assert!(state.saw_load);

        let finished_event = json!({
            "sessionId": "page-session",
            "method": "Network.loadingFinished"
        });
        assert!(apply_navigation_event(
            &mut state,
            &finished_event,
            "page-session"
        ));
        assert_eq!(state.inflight_requests, 0);

        let failed_event = json!({
            "sessionId": "page-session",
            "method": "Network.loadingFailed"
        });
        assert!(apply_navigation_event(
            &mut state,
            &failed_event,
            "page-session"
        ));
        assert_eq!(state.inflight_requests, 0);
    }

    #[test]
    fn apply_navigation_event_ignores_other_sessions() {
        let mut state = NavigationReadinessState::default();
        let event = json!({
            "sessionId": "other-session",
            "method": "Network.requestWillBeSent"
        });

        assert!(!apply_navigation_event(&mut state, &event, "page-session"));
        assert_eq!(state.inflight_requests, 0);
    }

    #[test]
    fn apply_navigation_event_deduplicates_request_ids() {
        let mut state = NavigationReadinessState::default();

        let first = json!({
            "sessionId": "page-session",
            "method": "Network.requestWillBeSent",
            "params": {"requestId": "req-1"}
        });
        let duplicate = json!({
            "sessionId": "page-session",
            "method": "Network.requestWillBeSent",
            "params": {"requestId": "req-1"}
        });
        let unknown_finish = json!({
            "sessionId": "page-session",
            "method": "Network.loadingFinished",
            "params": {"requestId": "unknown"}
        });
        let matched_finish = json!({
            "sessionId": "page-session",
            "method": "Network.loadingFinished",
            "params": {"requestId": "req-1"}
        });

        assert!(apply_navigation_event(&mut state, &first, "page-session"));
        assert_eq!(state.inflight_requests, 1);

        assert!(apply_navigation_event(
            &mut state,
            &duplicate,
            "page-session"
        ));
        assert_eq!(state.inflight_requests, 1);

        assert!(apply_navigation_event(
            &mut state,
            &unknown_finish,
            "page-session"
        ));
        assert_eq!(state.inflight_requests, 1);

        assert!(apply_navigation_event(
            &mut state,
            &matched_finish,
            "page-session"
        ));
        assert_eq!(state.inflight_requests, 0);
    }
}
