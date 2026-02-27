//! AI-first browser runtime for RestFlow.
//!
//! This crate provides a session-oriented browser automation service that is
//! designed for agent tool usage. It supports:
//! - Runtime probing for JavaScript/TypeScript execution prerequisites
//! - Session lifecycle management
//! - Direct JS/TS script execution against Chromium (via Playwright)
//! - Structured action plans for common browser workflows

use anyhow::{Result, anyhow, bail};
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::sync::RwLock;
use tokio::time::timeout;
use uuid::Uuid;

const RESULT_MARKER: &str = "__RESTFLOW_BROWSER_RESULT__=";
const DEFAULT_TIMEOUT_SECS: u64 = 120;

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
        Self::new_with_executor(root, Arc::new(PlaywrightExecutor::new()))
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
pub struct PlaywrightExecutor;

impl PlaywrightExecutor {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl BrowserExecutor for PlaywrightExecutor {
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
                15,
            )
            .await;
            probe.playwright_package_available = playwright_probe
                .map(|output| output.exit_code == 0)
                .unwrap_or(false);
        }

        probe.chromium_cache_detected = detect_chromium_cache();
        probe.ready = probe.node_available && probe.playwright_package_available;

        if !probe.node_available {
            probe.notes.push(
                "Node.js not found. Install Node.js 20+ to enable browser runtime.".to_string(),
            );
        }

        if probe.node_available && !probe.playwright_package_available {
            probe
                .notes
                .push("Playwright npm package not found. Run: npm i -D playwright".to_string());
        }

        if probe.ready && !probe.chromium_cache_detected {
            probe.notes.push(
                "Chromium browser binary not found in Playwright cache. Run: npx playwright install chromium".to_string(),
            );
        }

        if !probe.node_typescript_available {
            probe.notes.push(
                "TypeScript direct execution is unavailable in current Node runtime (requires --experimental-strip-types)."
                    .to_string(),
            );
        }

        Ok(probe)
    }

    async fn run_script(
        &self,
        session: &BrowserSession,
        request: &RunScriptRequest,
    ) -> Result<BrowserExecutionResult> {
        let probe = self.probe_runtime().await?;
        ensure_probe_ready(&probe, request.language)?;
        run_node_job(
            build_script_runner(session, request)?,
            request.language,
            request.runtime,
            request.timeout_secs,
            request.cwd.as_deref(),
            &probe,
        )
        .await
    }

    async fn run_actions(
        &self,
        session: &BrowserSession,
        request: &RunActionsRequest,
    ) -> Result<BrowserExecutionResult> {
        let probe = self.probe_runtime().await?;
        ensure_probe_ready(&probe, ScriptLanguage::Js)?;
        run_node_job(
            build_action_runner(session, request)?,
            ScriptLanguage::Js,
            request.runtime,
            request.timeout_secs,
            request.cwd.as_deref(),
            &probe,
        )
        .await
    }
}

fn ensure_probe_ready(probe: &RuntimeProbe, language: ScriptLanguage) -> Result<()> {
    if !probe.node_available {
        bail!("Node.js is required for browser execution");
    }
    if !probe.playwright_package_available {
        bail!("Playwright npm package is not available. Install it with: npm i -D playwright");
    }
    if language == ScriptLanguage::Ts && !probe.node_typescript_available {
        bail!("TypeScript execution requires Node.js support for --experimental-strip-types");
    }
    Ok(())
}

fn build_script_runner(session: &BrowserSession, request: &RunScriptRequest) -> Result<String> {
    let session_literal = json!({
        "id": session.id,
        "headless": session.headless,
        "profileDir": session.profile_dir,
        "artifactsDir": session.artifacts_dir,
    })
    .to_string();

    let user_code = indent_code(&request.code, 4);

    let mut script = String::new();
    script.push_str("import fs from 'node:fs';\n");
    script.push_str("import path from 'node:path';\n\n");
    script.push_str("const RESULT_MARKER = '__RESTFLOW_BROWSER_RESULT__=';\n");
    script.push_str(&format!("const session = {};%n", session_literal).replace("%n", "\n"));
    script.push_str(
        "const storageStatePath = path.join(session.profileDir, 'storage-state.json');\n",
    );
    script.push_str("await fs.promises.mkdir(session.profileDir, { recursive: true });\n");
    script.push_str("await fs.promises.mkdir(session.artifactsDir, { recursive: true });\n\n");

    script.push_str("let chromium;\n");
    script.push_str("try {\n");
    script.push_str("  ({ chromium } = await import('playwright'));\n");
    script.push_str("} catch (error) {\n");
    script.push_str("  const message = error && error.stack ? error.stack : String(error);\n");
    script.push_str("  process.stderr.write(message + '\\n');\n");
    script.push_str("  process.stdout.write(`${RESULT_MARKER}${JSON.stringify({ success: false, error: message })}\\n`);\n");
    script.push_str("  process.exitCode = 1;\n");
    script.push_str("  process.exit();\n");
    script.push_str("}\n\n");

    script.push_str("const browser = await chromium.launch({ headless: session.headless });\n");
    script.push_str("const contextOptions = {};\n");
    script.push_str("if (fs.existsSync(storageStatePath)) {\n");
    script.push_str("  contextOptions.storageState = storageStatePath;\n");
    script.push_str("}\n");
    script.push_str("const context = await browser.newContext(contextOptions);\n");
    script.push_str("const page = await context.newPage();\n\n");

    script.push_str("const rf = {\n");
    script.push_str("  sessionId: session.id,\n");
    script.push_str("  page,\n");
    script.push_str("  context,\n");
    script.push_str("  browser,\n");
    script.push_str("  artifactsDir: session.artifactsDir,\n");
    script.push_str("  storageStatePath,\n");
    script.push_str("  async saveState() {\n");
    script.push_str("    await context.storageState({ path: storageStatePath });\n");
    script.push_str("  },\n");
    script.push_str("};\n\n");

    script.push_str("globalThis.rf = rf;\n");
    script.push_str("globalThis.__restflowResult = null;\n");
    script.push_str("globalThis.setRestflowResult = (value) => {\n");
    script.push_str("  globalThis.__restflowResult = value;\n");
    script.push_str("};\n\n");

    script.push_str("try {\n");
    script.push_str("  const __userMain = async (rf) => {\n");
    script.push_str(&user_code);
    script.push_str("\n  };\n");
    script.push_str("  const returned = await __userMain(rf);\n");
    script.push_str("  if (returned !== undefined) {\n");
    script.push_str("    globalThis.__restflowResult = returned;\n");
    script.push_str("  }\n");
    script.push_str("  await rf.saveState();\n");
    script.push_str("  process.stdout.write(`${RESULT_MARKER}${JSON.stringify({ success: true, result: globalThis.__restflowResult })}\\n`);\n");
    script.push_str("} catch (error) {\n");
    script.push_str("  const message = error && error.stack ? error.stack : String(error);\n");
    script.push_str("  process.stderr.write(message + '\\n');\n");
    script.push_str("  process.stdout.write(`${RESULT_MARKER}${JSON.stringify({ success: false, error: message })}\\n`);\n");
    script.push_str("  process.exitCode = 1;\n");
    script.push_str("} finally {\n");
    script.push_str("  await context.close().catch(() => {});\n");
    script.push_str("  await browser.close().catch(() => {});\n");
    script.push_str("}\n");

    Ok(script)
}

fn build_action_runner(session: &BrowserSession, request: &RunActionsRequest) -> Result<String> {
    let session_literal = json!({
        "id": session.id,
        "headless": session.headless,
        "profileDir": session.profile_dir,
        "artifactsDir": session.artifacts_dir,
    })
    .to_string();

    let actions_literal = serde_json::to_string(&request.actions)?;

    let mut script = String::new();
    script.push_str("import fs from 'node:fs';\n");
    script.push_str("import path from 'node:path';\n\n");
    script.push_str("const RESULT_MARKER = '__RESTFLOW_BROWSER_RESULT__=';\n");
    script.push_str(&format!("const session = {};%n", session_literal).replace("%n", "\n"));
    script.push_str(&format!("const actions = {};%n", actions_literal).replace("%n", "\n"));
    script.push_str(
        "const storageStatePath = path.join(session.profileDir, 'storage-state.json');\n",
    );
    script.push_str("await fs.promises.mkdir(session.profileDir, { recursive: true });\n");
    script.push_str("await fs.promises.mkdir(session.artifactsDir, { recursive: true });\n\n");

    script.push_str("let chromium;\n");
    script.push_str("try {\n");
    script.push_str("  ({ chromium } = await import('playwright'));\n");
    script.push_str("} catch (error) {\n");
    script.push_str("  const message = error && error.stack ? error.stack : String(error);\n");
    script.push_str("  process.stderr.write(message + '\\n');\n");
    script.push_str("  process.stdout.write(`${RESULT_MARKER}${JSON.stringify({ success: false, error: message })}\\n`);\n");
    script.push_str("  process.exitCode = 1;\n");
    script.push_str("  process.exit();\n");
    script.push_str("}\n\n");

    script.push_str("const browser = await chromium.launch({ headless: session.headless });\n");
    script.push_str("const contextOptions = {};\n");
    script.push_str("if (fs.existsSync(storageStatePath)) {\n");
    script.push_str("  contextOptions.storageState = storageStatePath;\n");
    script.push_str("}\n");
    script.push_str("const context = await browser.newContext(contextOptions);\n");
    script.push_str("const page = await context.newPage();\n\n");

    script.push_str("const rf = {\n");
    script.push_str("  page,\n");
    script.push_str("  context,\n");
    script.push_str("  browser,\n");
    script.push_str("  session,\n");
    script.push_str("  storageStatePath,\n");
    script.push_str("  async saveState() {\n");
    script.push_str("    await context.storageState({ path: storageStatePath });\n");
    script.push_str("  },\n");
    script.push_str("};\n\n");

    script.push_str("async function executeAction(action) {\n");
    script.push_str("  const timeoutMs = action.timeout_ms ?? 10000;\n");
    script.push_str("  switch (action.type) {\n");
    script.push_str("    case 'navigate': {\n");
    script.push_str(
        "      await rf.page.goto(action.url, { waitUntil: action.wait_until ?? 'load' });\n",
    );
    script.push_str("      return { type: action.type, url: action.url };\n");
    script.push_str("    }\n");
    script.push_str("    case 'click': {\n");
    script.push_str("      const locator = rf.page.locator(action.selector).first();\n");
    script.push_str("      await locator.waitFor({ state: 'visible', timeout: timeoutMs });\n");
    script.push_str("      await locator.click({ timeout: timeoutMs });\n");
    script.push_str("      return { type: action.type, selector: action.selector };\n");
    script.push_str("    }\n");
    script.push_str("    case 'fill': {\n");
    script.push_str("      const locator = rf.page.locator(action.selector).first();\n");
    script.push_str("      await locator.waitFor({ state: 'visible', timeout: timeoutMs });\n");
    script.push_str("      await locator.fill(action.text, { timeout: timeoutMs });\n");
    script.push_str("      return { type: action.type, selector: action.selector };\n");
    script.push_str("    }\n");
    script.push_str("    case 'type': {\n");
    script.push_str("      const locator = rf.page.locator(action.selector).first();\n");
    script.push_str("      await locator.waitFor({ state: 'visible', timeout: timeoutMs });\n");
    script.push_str("      await locator.type(action.text, { delay: action.delay_ms ?? 0, timeout: timeoutMs });\n");
    script.push_str("      return { type: action.type, selector: action.selector };\n");
    script.push_str("    }\n");
    script.push_str("    case 'press': {\n");
    script.push_str("      if (action.selector) {\n");
    script.push_str("        const locator = rf.page.locator(action.selector).first();\n");
    script.push_str("        await locator.waitFor({ state: 'visible', timeout: timeoutMs });\n");
    script.push_str("        await locator.press(action.key, { timeout: timeoutMs });\n");
    script.push_str("      } else {\n");
    script.push_str("        await rf.page.keyboard.press(action.key);\n");
    script.push_str("      }\n");
    script.push_str("      return { type: action.type, key: action.key };\n");
    script.push_str("    }\n");
    script.push_str("    case 'wait_for_selector': {\n");
    script.push_str("      const locator = rf.page.locator(action.selector).first();\n");
    script.push_str(
        "      await locator.waitFor({ state: action.state ?? 'visible', timeout: timeoutMs });\n",
    );
    script.push_str("      return { type: action.type, selector: action.selector };\n");
    script.push_str("    }\n");
    script.push_str("    case 'extract_text': {\n");
    script.push_str("      if (action.all) {\n");
    script.push_str(
        "        const values = await rf.page.locator(action.selector).allTextContents();\n",
    );
    script.push_str(
        "        return { type: action.type, selector: action.selector, value: values };\n",
    );
    script.push_str("      }\n");
    script.push_str(
        "      const value = await rf.page.locator(action.selector).first().textContent();\n",
    );
    script.push_str("      return { type: action.type, selector: action.selector, value };\n");
    script.push_str("    }\n");
    script.push_str("    case 'screenshot': {\n");
    script.push_str("      const target = path.isAbsolute(action.path) ? action.path : path.join(session.artifactsDir, action.path);\n");
    script.push_str("      await fs.promises.mkdir(path.dirname(target), { recursive: true });\n");
    script.push_str(
        "      await rf.page.screenshot({ path: target, fullPage: action.full_page ?? false });\n",
    );
    script.push_str("      return { type: action.type, path: target };\n");
    script.push_str("    }\n");
    script.push_str("    case 'evaluate': {\n");
    script.push_str(
        "      const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;\n",
    );
    script.push_str("      let value;\n");
    script.push_str("      try {\n");
    script.push_str("        const exprFn = new AsyncFunction('page', 'context', 'browser', `return (${action.expression});`);\n");
    script.push_str("        value = await exprFn(rf.page, rf.context, rf.browser);\n");
    script.push_str("      } catch (_) {\n");
    script.push_str("        const stmtFn = new AsyncFunction('page', 'context', 'browser', action.expression);\n");
    script.push_str("        value = await stmtFn(rf.page, rf.context, rf.browser);\n");
    script.push_str("      }\n");
    script.push_str("      return { type: action.type, value };\n");
    script.push_str("    }\n");
    script.push_str("    default:\n");
    script.push_str("      throw new Error(`Unsupported action type: ${action.type}`);\n");
    script.push_str("  }\n");
    script.push_str("}\n\n");

    script.push_str("const outputs = [];\n");
    script.push_str("try {\n");
    script.push_str("  for (const action of actions) {\n");
    script.push_str("    const value = await executeAction(action);\n");
    script.push_str("    outputs.push(value);\n");
    script.push_str("  }\n");
    script.push_str("  await rf.saveState();\n");
    script.push_str("  process.stdout.write(`${RESULT_MARKER}${JSON.stringify({ success: true, result: outputs })}\\n`);\n");
    script.push_str("} catch (error) {\n");
    script.push_str("  const message = error && error.stack ? error.stack : String(error);\n");
    script.push_str("  process.stderr.write(message + '\\n');\n");
    script.push_str("  process.stdout.write(`${RESULT_MARKER}${JSON.stringify({ success: false, error: message })}\\n`);\n");
    script.push_str("  process.exitCode = 1;\n");
    script.push_str("} finally {\n");
    script.push_str("  await context.close().catch(() => {});\n");
    script.push_str("  await browser.close().catch(() => {});\n");
    script.push_str("}\n");

    Ok(script)
}

async fn run_node_job(
    script_content: String,
    language: ScriptLanguage,
    runtime: ScriptRuntime,
    timeout_secs: u64,
    cwd: Option<&str>,
    probe: &RuntimeProbe,
) -> Result<BrowserExecutionResult> {
    if runtime != ScriptRuntime::Auto && runtime != ScriptRuntime::Node {
        bail!("Only node runtime is supported");
    }

    let timeout_secs = timeout_secs.max(1);
    let mut args = Vec::new();

    if language == ScriptLanguage::Ts {
        if !probe.node_typescript_available {
            bail!("TypeScript execution requires Node.js support for --experimental-strip-types");
        }
        args.push("--experimental-strip-types".to_string());
    }

    let temp_dir = tempfile::Builder::new()
        .prefix("restflow-browser-script-")
        .tempdir()?;

    let extension = if language == ScriptLanguage::Ts {
        "ts"
    } else {
        "mjs"
    };
    let script_path = temp_dir.path().join(format!("runner.{}", extension));
    std::fs::write(&script_path, script_content)?;

    args.push(script_path.display().to_string());

    let cwd_path = match cwd {
        Some(path) => {
            let parsed = PathBuf::from(path);
            if !parsed.exists() || !parsed.is_dir() {
                bail!("Invalid working directory: {}", parsed.display());
            }
            Some(parsed)
        }
        None => None,
    };

    let started = Instant::now();
    let output = run_command_capture("node", &args, cwd_path.as_deref(), timeout_secs).await?;
    let duration_ms = started.elapsed().as_millis() as u64;
    let (stdout, payload) = extract_result_payload(&output.stdout);

    Ok(BrowserExecutionResult {
        runtime: "node".to_string(),
        exit_code: output.exit_code,
        duration_ms,
        stdout,
        stderr: output.stderr,
        payload,
    })
}

fn indent_code(code: &str, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    if code.is_empty() {
        return prefix;
    }

    code.lines()
        .map(|line| format!("{}{}", prefix, line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_result_payload(stdout: &str) -> (String, Option<Value>) {
    let mut payload: Option<Value> = None;
    let mut clean_lines = Vec::new();

    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix(RESULT_MARKER) {
            if let Ok(value) = serde_json::from_str::<Value>(rest.trim()) {
                payload = Some(value);
            }
            continue;
        }
        clean_lines.push(line.to_string());
    }

    (clean_lines.join("\n"), payload)
}

struct CommandCapture {
    exit_code: i32,
    stdout: String,
    stderr: String,
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

fn detect_chromium_cache() -> bool {
    if let Ok(path) = std::env::var("PLAYWRIGHT_BROWSERS_PATH") {
        let parsed = PathBuf::from(path);
        if parsed.exists() {
            return true;
        }
    }

    let mut candidates = Vec::new();

    if let Ok(home) = std::env::var("HOME") {
        candidates.push(PathBuf::from(&home).join(".cache/ms-playwright"));
        candidates.push(PathBuf::from(&home).join("Library/Caches/ms-playwright"));
    }

    if let Ok(user_profile) = std::env::var("USERPROFILE") {
        candidates.push(PathBuf::from(user_profile).join("AppData/Local/ms-playwright"));
    }

    candidates.into_iter().any(|path| path.exists())
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
    fn extract_payload_marker_parses_json() {
        let stdout = "line1\n__RESTFLOW_BROWSER_RESULT__={\"success\":true,\"result\":123}\nline2";
        let (cleaned, payload) = extract_result_payload(stdout);
        assert_eq!(cleaned, "line1\nline2");
        assert_eq!(payload.unwrap()["result"], json!(123));
    }

    #[test]
    fn build_action_runner_contains_switch_cases() {
        let session = BrowserSession {
            id: "s1".to_string(),
            browser: BrowserKind::Chromium,
            headless: true,
            created_at_ms: 0,
            session_dir: "/tmp/s1".to_string(),
            profile_dir: "/tmp/s1/profile".to_string(),
            artifacts_dir: "/tmp/s1/artifacts".to_string(),
        };

        let script = build_action_runner(
            &session,
            &RunActionsRequest {
                session_id: "s1".to_string(),
                actions: vec![BrowserAction::Navigate {
                    url: "https://example.com".to_string(),
                    wait_until: None,
                }],
                runtime: ScriptRuntime::Auto,
                timeout_secs: 60,
                cwd: None,
            },
        )
        .unwrap();

        assert!(script.contains("case 'navigate'"));
        assert!(script.contains("case 'screenshot'"));
    }
}
