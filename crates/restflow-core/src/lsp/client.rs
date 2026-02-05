//! LSP client implementation over stdio JSON-RPC.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use anyhow::Context;
use dashmap::DashMap;
use lsp_types::{
    ClientCapabilities, Diagnostic, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    InitializeParams, InitializedParams, TextDocumentContentChangeEvent, TextDocumentItem,
    Url,
};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{mpsc, oneshot, Mutex, Notify, RwLock};
use tokio::time::Duration;
use tracing::{debug, warn};

use super::protocol::{JsonRpcNotification, JsonRpcRequest};

#[derive(Debug, Clone)]
pub struct LspClientConfig {
    pub command: String,
    pub args: Vec<String>,
    pub language_id: String,
    pub root_uri: Option<Url>,
}

pub struct LspClient {
    sender: mpsc::Sender<Value>,
    pending: Arc<Mutex<HashMap<i64, oneshot::Sender<Value>>>>,
    diagnostics: Arc<RwLock<HashMap<PathBuf, Vec<Diagnostic>>>>,
    diagnostics_notify: Arc<Notify>,
    open_versions: Arc<DashMap<PathBuf, i32>>,
    language_id: String,
    root_uri: Option<Url>,
    next_id: AtomicI64,
    _child: Arc<Mutex<Child>>,
}

impl LspClient {
    pub async fn new(config: LspClientConfig) -> anyhow::Result<Self> {
        let mut command = Command::new(&config.command);
        command.args(&config.args);
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::null());

        let mut child = command.spawn().context("Failed to spawn LSP server")?;
        let stdin = child.stdin.take().context("Failed to open stdin")?;
        let stdout = child.stdout.take().context("Failed to open stdout")?;

        let (sender, mut receiver) = mpsc::channel::<Value>(128);
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let diagnostics = Arc::new(RwLock::new(HashMap::new()));
        let diagnostics_notify = Arc::new(Notify::new());
        let open_versions = Arc::new(DashMap::new());
        let language_id = config.language_id.clone();
        let root_uri = config.root_uri.clone();

        let pending_reader = pending.clone();
        let diagnostics_reader = diagnostics.clone();
        let diagnostics_notify_reader = diagnostics_notify.clone();

        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            loop {
                match read_message(&mut reader).await {
                    Ok(Some(message)) => {
                        handle_incoming(
                            message,
                            &pending_reader,
                            &diagnostics_reader,
                            &diagnostics_notify_reader,
                        )
                        .await;
                    }
                    Ok(None) => break,
                    Err(err) => {
                        warn!(error = %err, "LSP reader error");
                        break;
                    }
                }
            }
        });

        tokio::spawn(async move {
            let mut writer = stdin;
            while let Some(message) = receiver.recv().await {
                if let Err(err) = write_message(&mut writer, &message).await {
                    warn!(error = %err, "LSP writer error");
                    break;
                }
            }
        });

        Ok(Self {
            sender,
            pending,
            diagnostics,
            diagnostics_notify,
            open_versions,
            language_id,
            root_uri,
            next_id: AtomicI64::new(1),
            _child: Arc::new(Mutex::new(child)),
        })
    }

    pub async fn initialize(&self) -> anyhow::Result<()> {
        let params = InitializeParams {
            process_id: None,
            root_uri: self.root_uri.clone(),
            capabilities: ClientCapabilities::default(),
            initialization_options: None,
            trace: None,
            workspace_folders: None,
            root_path: None,
            client_info: None,
            locale: None,
            ..InitializeParams::default()
        };

        let _ = self
            .request("initialize", serde_json::to_value(params)?)
            .await?;

        self.notify("initialized", serde_json::to_value(InitializedParams {})?)
            .await?;

        Ok(())
    }

    pub async fn did_open(&self, path: &Path, content: &str) -> anyhow::Result<()> {
        if self.open_versions.contains_key(path) {
            return Ok(());
        }

        let uri = Url::from_file_path(path)
            .map_err(|_| anyhow::anyhow!("Invalid file path"))?;
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri,
                language_id: self.language_id.clone(),
                version: 1,
                text: content.to_string(),
            },
        };

        self.open_versions.insert(path.to_path_buf(), 1);
        self.notify("textDocument/didOpen", serde_json::to_value(params)?)
            .await?;

        Ok(())
    }

    pub async fn did_change(&self, path: &Path, content: &str) -> anyhow::Result<()> {
        let uri = Url::from_file_path(path)
            .map_err(|_| anyhow::anyhow!("Invalid file path"))?;

        let version = self
            .open_versions
            .get(path)
            .map(|v| v.value().to_owned())
            .unwrap_or(0)
            + 1;

        self.open_versions.insert(path.to_path_buf(), version);

        let params = DidChangeTextDocumentParams {
            text_document: lsp_types::VersionedTextDocumentIdentifier { uri, version },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: content.to_string(),
            }],
        };

        self.notify("textDocument/didChange", serde_json::to_value(params)?)
            .await?;
        Ok(())
    }

    pub async fn wait_for_diagnostics(
        &self,
        path: &Path,
        timeout: Duration,
    ) -> anyhow::Result<Vec<Diagnostic>> {
        let start = tokio::time::Instant::now();
        loop {
            if let Some(diags) = self.get_diagnostics(path).await? {
                return Ok(diags);
            }

            let remaining = timeout
                .checked_sub(start.elapsed())
                .unwrap_or_else(|| Duration::from_millis(0));
            if remaining.is_zero() {
                return Ok(Vec::new());
            }

            let notified = tokio::time::timeout(remaining, self.diagnostics_notify.notified()).await;
            if notified.is_err() {
                return Ok(Vec::new());
            }
        }
    }

    pub async fn get_diagnostics(&self, path: &Path) -> anyhow::Result<Option<Vec<Diagnostic>>> {
        let diagnostics = self.diagnostics.read().await;
        Ok(diagnostics.get(path).cloned())
    }

    async fn request(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
            params: Some(params),
        };

        self.sender
            .send(serde_json::to_value(request)?)
            .await
            .context("Failed to send request")?;

        let response = rx.await.context("LSP request canceled")?;
        Ok(response)
    }

    async fn notify(&self, method: &str, params: Value) -> anyhow::Result<()> {
        let notification = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params: Some(params),
        };

        self.sender
            .send(serde_json::to_value(notification)?)
            .await
            .context("Failed to send notification")?;

        Ok(())
    }
}

async fn handle_incoming(
    message: Value,
    pending: &Arc<Mutex<HashMap<i64, oneshot::Sender<Value>>>>,
    diagnostics: &Arc<RwLock<HashMap<PathBuf, Vec<Diagnostic>>>>,
    diagnostics_notify: &Arc<Notify>,
) {
    if let Some(id) = message.get("id").and_then(|v| v.as_i64())
        && message.get("method").is_none()
    {
        let sender = pending.lock().await.remove(&id);
        if let Some(sender) = sender {
            let _ = sender.send(message);
        }
        return;
    }

    let method = message.get("method").and_then(|v| v.as_str());
    if method == Some("textDocument/publishDiagnostics") {
        if let Some(params) = message.get("params") {
            match serde_json::from_value::<lsp_types::PublishDiagnosticsParams>(params.clone()) {
                Ok(payload) => {
                    if let Ok(path) = payload.uri.to_file_path() {
                        diagnostics
                            .write()
                            .await
                            .insert(path, payload.diagnostics);
                        diagnostics_notify.notify_waiters();
                    }
                }
                Err(err) => {
                    debug!(error = %err, "Failed to parse diagnostics");
                }
            }
        }
    }
}

async fn read_message<R: AsyncBufReadExt + Unpin>(reader: &mut R) -> anyhow::Result<Option<Value>> {
    let mut content_length: Option<usize> = None;

    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line).await?;
        if bytes == 0 {
            return Ok(None);
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            content_length = Some(value.trim().parse()?);
        }
    }

    let length = match content_length {
        Some(length) => length,
        None => return Ok(None),
    };

    let mut buffer = vec![0u8; length];
    reader.read_exact(&mut buffer).await?;
    let message: Value = serde_json::from_slice(&buffer)?;
    Ok(Some(message))
}

async fn write_message(writer: &mut ChildStdin, message: &Value) -> anyhow::Result<()> {
    let payload = serde_json::to_vec(message)?;
    let header = format!("Content-Length: {}\r\n\r\n", payload.len());
    writer.write_all(header.as_bytes()).await?;
    writer.write_all(&payload).await?;
    writer.flush().await?;
    Ok(())
}
