//! File system watcher for LSP diagnostics updates.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::Mutex;

use super::manager::LspManager;

pub struct LspWatcher {
    _watcher: RecommendedWatcher,
}

impl LspWatcher {
    pub fn spawn(root: PathBuf, manager: LspManager) -> anyhow::Result<Self> {
        let (tx, mut rx) = tokio::sync::mpsc::channel(256);
        let pending = Arc::new(Mutex::new(HashMap::<PathBuf, Instant>::new()));

        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.blocking_send(res);
        })?;

        watcher
            .watch(&root, RecursiveMode::Recursive)
            .context("Failed to start LSP watcher")?;

        tokio::spawn({
            let pending = pending.clone();
            async move {
                while let Some(event) = rx.recv().await {
                    let Ok(event) = event else { continue };
                    for path in event.paths {
                        if !is_supported(&path) {
                            continue;
                        }
                        let now = Instant::now();
                        {
                            let mut guard = pending.lock().await;
                            guard.insert(path.clone(), now);
                        }

                        let pending = pending.clone();
                        let manager = manager.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_millis(300)).await;
                            let should_process = {
                                let mut guard = pending.lock().await;
                                match guard.get(&path) {
                                    Some(seen) if *seen == now => {
                                        guard.remove(&path);
                                        true
                                    }
                                    _ => false,
                                }
                            };

                            if should_process {
                                let _ = manager.on_fs_change(&path).await;
                            }
                        });
                    }
                }
            }
        });

        Ok(Self { _watcher: watcher })
    }
}

fn is_supported(path: &Path) -> bool {
    if path.is_dir() {
        return false;
    }

    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("rs") | Some("go") | Some("ts") | Some("tsx") | Some("js") | Some("jsx") | Some("py")
    )
}
