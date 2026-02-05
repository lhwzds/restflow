//! LSP manager for multiple language servers.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use dashmap::DashMap;
use lsp_types::{Diagnostic, Uri};
use tokio::sync::Mutex;
use tracing::warn;
use url::Url;

use restflow_ai::DiagnosticsProvider;

use super::client::{LspClient, LspClientConfig};
use super::watcher::LspWatcher;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LanguageId {
    Rust,
    Go,
    TypeScript,
    Python,
}

impl LanguageId {
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("rs") => Some(Self::Rust),
            Some("go") => Some(Self::Go),
            Some("ts") | Some("tsx") | Some("js") | Some("jsx") => Some(Self::TypeScript),
            Some("py") => Some(Self::Python),
            _ => None,
        }
    }

    pub fn language_id(&self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Go => "go",
            Self::TypeScript => "typescript",
            Self::Python => "python",
        }
    }

    pub fn command(&self) -> (&'static str, Vec<&'static str>) {
        match self {
            Self::Rust => ("rust-analyzer", vec![]),
            Self::Go => ("gopls", vec![]),
            Self::TypeScript => ("typescript-language-server", vec!["--stdio"]),
            Self::Python => ("pyright-langserver", vec!["--stdio"]),
        }
    }
}

#[derive(Clone)]
pub struct LspManager {
    root: PathBuf,
    clients: Arc<DashMap<LanguageId, Arc<LspClient>>>,
    init_lock: Arc<Mutex<()>>,
    watcher: Arc<Mutex<Option<LspWatcher>>>,
}

impl LspManager {
    pub fn new(root: PathBuf) -> Self {
        let manager = Self {
            root,
            clients: Arc::new(DashMap::new()),
            init_lock: Arc::new(Mutex::new(())),
            watcher: Arc::new(Mutex::new(None)),
        };

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let cloned = manager.clone();
            handle.spawn(async move {
                if let Err(err) = cloned.ensure_watcher().await {
                    warn!(error = %err, "Failed to start LSP watcher");
                }
            });
        } else {
            warn!("Skipping LSP watcher startup: no Tokio runtime available");
        }

        manager
    }

    pub async fn get_client_for_file(&self, path: &Path) -> anyhow::Result<Option<Arc<LspClient>>> {
        let Some(language) = LanguageId::from_path(path) else {
            return Ok(None);
        };

        if let Some(client) = self.clients.get(&language) {
            return Ok(Some(client.clone()));
        }

        let _guard = self.init_lock.lock().await;
        if let Some(client) = self.clients.get(&language) {
            return Ok(Some(client.clone()));
        }

        let (command, args) = language.command();
        let root_uri = Url::from_directory_path(&self.root)
            .ok()
            .and_then(|url| Uri::from_str(url.as_str()).ok());
        let config = LspClientConfig {
            command: command.to_string(),
            args: args.into_iter().map(|v| v.to_string()).collect(),
            language_id: language.language_id().to_string(),
            root_uri,
        };

        let client = Arc::new(LspClient::new(config).await?);
        client.initialize().await?;
        self.clients.insert(language, client.clone());

        Ok(Some(client))
    }

    pub async fn on_fs_change(&self, path: &Path) -> anyhow::Result<()> {
        let Some(client) = self.get_client_for_file(path).await? else {
            return Ok(());
        };

        let content = tokio::fs::read_to_string(path).await?;
        client.did_open(path, &content).await?;
        client.did_change(path, &content).await?;
        Ok(())
    }

    async fn ensure_watcher(&self) -> anyhow::Result<()> {
        let mut guard = self.watcher.lock().await;
        if guard.is_some() {
            return Ok(());
        }

        let watcher = LspWatcher::spawn(self.root.clone(), self.clone())?;
        *guard = Some(watcher);
        Ok(())
    }

    async fn ensure_open(&self, path: &Path) -> anyhow::Result<()> {
        let Some(client) = self.get_client_for_file(path).await? else {
            return Ok(());
        };

        let content = tokio::fs::read_to_string(path)
            .await
            .context("Failed to read file for LSP")?;
        client.did_open(path, &content).await?;
        Ok(())
    }

    async fn did_change(&self, path: &Path, content: &str) -> anyhow::Result<()> {
        let Some(client) = self.get_client_for_file(path).await? else {
            return Ok(());
        };

        client.did_open(path, content).await?;
        client.did_change(path, content).await?;
        Ok(())
    }

    async fn wait_for_diagnostics(
        &self,
        path: &Path,
        timeout: Duration,
    ) -> anyhow::Result<Vec<Diagnostic>> {
        let Some(client) = self.get_client_for_file(path).await? else {
            return Ok(Vec::new());
        };

        client.wait_for_diagnostics(path, timeout).await
    }

    async fn get_diagnostics(&self, path: &Path) -> anyhow::Result<Vec<Diagnostic>> {
        let Some(client) = self.get_client_for_file(path).await? else {
            return Ok(Vec::new());
        };

        Ok(client
            .get_diagnostics(path)
            .await?
            .unwrap_or_default())
    }
}

#[async_trait::async_trait]
impl DiagnosticsProvider for LspManager {
    async fn ensure_open(&self, path: &Path) -> restflow_ai::Result<()> {
        self.ensure_open(path)
            .await
            .map_err(|e| restflow_ai::AiError::Tool(e.to_string()))
    }

    async fn did_change(&self, path: &Path, content: &str) -> restflow_ai::Result<()> {
        self.did_change(path, content)
            .await
            .map_err(|e| restflow_ai::AiError::Tool(e.to_string()))
    }

    async fn wait_for_diagnostics(
        &self,
        path: &Path,
        timeout: Duration,
    ) -> restflow_ai::Result<Vec<Diagnostic>> {
        self.wait_for_diagnostics(path, timeout)
            .await
            .map_err(|e| restflow_ai::AiError::Tool(e.to_string()))
    }

    async fn get_diagnostics(&self, path: &Path) -> restflow_ai::Result<Vec<Diagnostic>> {
        self.get_diagnostics(path)
            .await
            .map_err(|e| restflow_ai::AiError::Tool(e.to_string()))
    }
}
