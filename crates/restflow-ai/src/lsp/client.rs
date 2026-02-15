//! LSP client implementation backed by stdio JSON-RPC.

use std::path::Path;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

use lsp_types::{
    ClientCapabilities, Diagnostic, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    DocumentDiagnosticParams, DocumentDiagnosticReport, InitializeParams, InitializedParams,
    TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem, Uri,
    VersionedTextDocumentIdentifier, WorkspaceFolder,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use url::Url;

use crate::error::{AiError, Result};

use super::LspServerConfig;
use super::types::{JsonRpcMessage, JsonRpcRequest, JsonRpcResponse};

/// LSP client for a single language server.
#[derive(Debug)]
pub struct LspClient {
    process: Child,
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<BufReader<ChildStdout>>,
    request_id: AtomicU64,
    language: String,
}

impl LspClient {
    /// Create and initialize a new LSP client.
    pub async fn new(config: &LspServerConfig) -> Result<Self> {
        let mut process = Command::new(&config.command)
            .args(&config.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(AiError::Io)?;

        let stdin = process
            .stdin
            .take()
            .ok_or_else(|| AiError::Tool("Failed to open LSP stdin".to_string()))?;
        let stdout = process
            .stdout
            .take()
            .ok_or_else(|| AiError::Tool("Failed to open LSP stdout".to_string()))?;

        let client = Self {
            process,
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(BufReader::new(stdout)),
            request_id: AtomicU64::new(1),
            language: config.language.clone(),
        };

        client.initialize().await?;
        Ok(client)
    }

    /// Initialize the LSP session.
    async fn initialize(&self) -> Result<()> {
        let root_uri = std::env::current_dir()
            .ok()
            .and_then(|path| Url::from_file_path(path).ok())
            .and_then(|url| Uri::from_str(url.as_str()).ok());

        let workspace_folders = root_uri.as_ref().map(|uri| {
            vec![WorkspaceFolder {
                uri: uri.clone(),
                name: "workspace".to_string(),
            }]
        });

        let params = InitializeParams {
            process_id: Some(std::process::id()),
            workspace_folders,
            capabilities: ClientCapabilities::default(),
            ..Default::default()
        };

        let _: serde_json::Value = self.send_request("initialize", params).await?;
        self.send_notification("initialized", InitializedParams {})
            .await?;
        Ok(())
    }

    /// Notify server that a document has been opened.
    pub async fn did_open(&self, path: &Path, content: &str, version: i32) -> Result<()> {
        let url = Url::from_file_path(path)
            .map_err(|_| AiError::Tool(format!("Invalid file path: {}", path.display())))?;
        let uri = Uri::from_str(url.as_str())
            .map_err(|err| AiError::Tool(format!("Invalid file uri: {err}")))?;

        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri,
                language_id: self.detect_language(path),
                version,
                text: content.to_string(),
            },
        };

        self.send_notification("textDocument/didOpen", params).await
    }

    /// Notify server that a document has changed.
    pub async fn did_change(&self, path: &Path, content: &str, version: i32) -> Result<()> {
        let url = Url::from_file_path(path)
            .map_err(|_| AiError::Tool(format!("Invalid file path: {}", path.display())))?;
        let uri = Uri::from_str(url.as_str())
            .map_err(|err| AiError::Tool(format!("Invalid file uri: {err}")))?;

        let params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier { uri, version },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: content.to_string(),
            }],
        };

        self.send_notification("textDocument/didChange", params)
            .await
    }

    /// Request diagnostics for a document.
    pub async fn get_diagnostics(&self, path: &Path) -> Result<Vec<Diagnostic>> {
        let url = Url::from_file_path(path)
            .map_err(|_| AiError::Tool(format!("Invalid file path: {}", path.display())))?;
        let uri = Uri::from_str(url.as_str())
            .map_err(|err| AiError::Tool(format!("Invalid file uri: {err}")))?;

        let params = DocumentDiagnosticParams {
            text_document: TextDocumentIdentifier { uri },
            identifier: None,
            previous_result_id: None,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let report: DocumentDiagnosticReport =
            self.send_request("textDocument/diagnostic", params).await?;

        let diagnostics = match report {
            DocumentDiagnosticReport::Full(full) => full.full_document_diagnostic_report.items,
            DocumentDiagnosticReport::Unchanged(_) => Vec::new(),
        };

        Ok(diagnostics)
    }

    /// Shutdown the LSP session.
    pub async fn shutdown(&mut self) -> Result<()> {
        let _: serde_json::Value = self.send_request("shutdown", serde_json::json!({})).await?;
        self.send_notification("exit", serde_json::json!({}))
            .await?;
        let _ = self.process.wait().await;
        Ok(())
    }

    async fn send_request<P, R>(&self, method: &str, params: P) -> Result<R>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let request = JsonRpcRequest::new(id, method, params);
        self.write_message(&JsonRpcMessage::Request(request))
            .await?;

        let response = self.read_response(id).await?;
        response
            .into_result()
            .map_err(|err| AiError::Tool(format!("LSP error {}: {}", err.code, err.message)))
    }

    async fn send_notification<P>(&self, method: &str, params: P) -> Result<()>
    where
        P: Serialize,
    {
        let notification = JsonRpcRequest::notification(method, params);
        self.write_message(&JsonRpcMessage::Request(notification))
            .await
    }

    async fn write_message(&self, message: &JsonRpcMessage) -> Result<()> {
        let payload = serde_json::to_vec(message).map_err(AiError::Json)?;
        let header = format!("Content-Length: {}\r\n\r\n", payload.len());

        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(header.as_bytes())
            .await
            .map_err(AiError::Io)?;
        stdin.write_all(&payload).await.map_err(AiError::Io)?;
        stdin.flush().await.map_err(AiError::Io)?;
        Ok(())
    }

    async fn read_response(&self, expected_id: u64) -> Result<JsonRpcResponse> {
        loop {
            let message = self.read_message().await?;
            if let JsonRpcMessage::Response(response) = message
                && response.id == expected_id
            {
                return Ok(response);
            }
        }
    }

    async fn read_message(&self) -> Result<JsonRpcMessage> {
        let mut stdout = self.stdout.lock().await;
        let mut content_length: Option<usize> = None;

        loop {
            let mut line = String::new();
            let bytes = stdout.read_line(&mut line).await.map_err(AiError::Io)?;
            if bytes == 0 {
                return Err(AiError::Tool("LSP server closed stdout".to_string()));
            }

            if line == "\r\n" {
                break;
            }

            if let Some(value) = line.strip_prefix("Content-Length:") {
                let value = value.trim();
                content_length = value.parse::<usize>().ok();
            }
        }

        let length = content_length.ok_or_else(|| {
            AiError::Tool("Missing Content-Length header from LSP server".to_string())
        })?;

        let mut buffer = vec![0u8; length];
        stdout.read_exact(&mut buffer).await.map_err(AiError::Io)?;

        let message: JsonRpcMessage = serde_json::from_slice(&buffer).map_err(AiError::Json)?;
        Ok(message)
    }

    fn detect_language(&self, path: &Path) -> String {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_string())
            .unwrap_or_else(|| self.language.clone())
    }
}
