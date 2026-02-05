//! LSP manager for multiple language servers.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lsp_types::Diagnostic;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::error::{AiError, Result};

use super::LspClient;

/// Configuration for a language server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspServerConfig {
    pub language: String,
    pub command: String,
    pub args: Vec<String>,
    pub file_extensions: Vec<String>,
}

/// Manages LSP clients for multiple languages.
#[derive(Debug)]
pub struct LspManager {
    clients: HashMap<String, Arc<Mutex<LspClient>>>,
    configs: HashMap<String, LspServerConfig>,
    file_versions: HashMap<PathBuf, i32>,
}

impl LspManager {
    /// Create a manager with default server configurations.
    pub fn new() -> Self {
        let mut configs = HashMap::new();
        configs.insert(
            "go".to_string(),
            LspServerConfig {
                language: "go".to_string(),
                command: "gopls".to_string(),
                args: vec![],
                file_extensions: vec!["go".to_string()],
            },
        );
        configs.insert(
            "rust".to_string(),
            LspServerConfig {
                language: "rust".to_string(),
                command: "rust-analyzer".to_string(),
                args: vec![],
                file_extensions: vec!["rs".to_string()],
            },
        );
        configs.insert(
            "typescript".to_string(),
            LspServerConfig {
                language: "typescript".to_string(),
                command: "typescript-language-server".to_string(),
                args: vec!["--stdio".to_string()],
                file_extensions: vec![
                    "ts".to_string(),
                    "tsx".to_string(),
                    "js".to_string(),
                    "jsx".to_string(),
                ],
            },
        );
        configs.insert(
            "python".to_string(),
            LspServerConfig {
                language: "python".to_string(),
                command: "pyright-langserver".to_string(),
                args: vec!["--stdio".to_string()],
                file_extensions: vec!["py".to_string()],
            },
        );

        Self {
            clients: HashMap::new(),
            configs,
            file_versions: HashMap::new(),
        }
    }

    /// Override or add a server configuration.
    pub fn set_config(&mut self, config: LspServerConfig) {
        self.configs.insert(config.language.clone(), config);
    }

    /// Get diagnostics for a file.
    pub async fn get_diagnostics(&mut self, path: &Path, content: &str) -> Result<Vec<Diagnostic>> {
        let client = self.get_client_for_file(path).await?;
        let version = self.next_version(path, true);

        {
            let client = client.lock().await;
            client.did_open(path, content, version).await?;
        }

        let diagnostics = {
            let client = client.lock().await;
            client.get_diagnostics(path).await?
        };

        Ok(diagnostics)
    }

    /// Notify file change and request updated diagnostics.
    pub async fn notify_change(
        &mut self,
        path: &Path,
        content: &str,
    ) -> Result<Vec<Diagnostic>> {
        let client = self.get_client_for_file(path).await?;
        let version = self.next_version(path, false);

        {
            let client = client.lock().await;
            client.did_change(path, content, version).await?;
        }

        let diagnostics = {
            let client = client.lock().await;
            client.get_diagnostics(path).await?
        };

        Ok(diagnostics)
    }

    /// Shutdown all LSP clients.
    pub async fn shutdown(&mut self) {
        let mut clients = Vec::new();
        for (_, client) in self.clients.drain() {
            clients.push(client);
        }

        for client in clients {
            if let Ok(mut client) = client.try_lock() {
                let _ = client.shutdown().await;
            }
        }
    }

    async fn get_client_for_file(&mut self, path: &Path) -> Result<Arc<Mutex<LspClient>>> {
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .ok_or_else(|| AiError::Tool("File extension missing".to_string()))?;

        let language = self
            .detect_language_by_extension(extension)
            .ok_or_else(|| AiError::Tool("Unsupported file extension".to_string()))?;

        if let Some(client) = self.clients.get(&language) {
            return Ok(client.clone());
        }

        let config = self
            .configs
            .get(&language)
            .ok_or_else(|| AiError::Tool("Missing LSP configuration".to_string()))?
            .clone();

        let client = LspClient::new(&config).await?;
        let client = Arc::new(Mutex::new(client));
        self.clients.insert(language, client.clone());

        Ok(client)
    }

    fn detect_language_by_extension(&self, ext: &str) -> Option<String> {
        for (language, config) in &self.configs {
            if config
                .file_extensions
                .iter()
                .any(|candidate| candidate == ext)
            {
                return Some(language.clone());
            }
        }
        None
    }

    fn next_version(&mut self, path: &Path, is_open: bool) -> i32 {
        use std::collections::hash_map::Entry;

        let entry = self.file_versions.entry(path.to_path_buf());
        if is_open {
            return match entry {
                Entry::Occupied(entry) => *entry.get(),
                Entry::Vacant(entry) => {
                    entry.insert(1);
                    1
                }
            };
        }

        match entry {
            Entry::Occupied(mut entry) => {
                let value = entry.get_mut();
                *value += 1;
                *value
            }
            Entry::Vacant(entry) => {
                entry.insert(1);
                1
            }
        }
    }
}

impl Default for LspManager {
    fn default() -> Self {
        Self::new()
    }
}
