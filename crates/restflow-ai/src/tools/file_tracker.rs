//! File read/write tracking for external modification detection.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::SystemTime;

use tokio::fs;

#[derive(Debug, Default)]
pub struct FileTracker {
    records: RwLock<HashMap<PathBuf, FileRecord>>,
}

#[derive(Debug, Clone)]
struct FileRecord {
    last_read: SystemTime,
    last_write: Option<SystemTime>,
}

impl FileTracker {
    pub fn new() -> Self {
        Self {
            records: RwLock::new(HashMap::new()),
        }
    }

    /// Record that we read a file.
    pub fn record_read(&self, path: &Path) {
        let mut records = self.records.write().unwrap();
        let entry = records.entry(path.to_path_buf()).or_insert(FileRecord {
            last_read: SystemTime::UNIX_EPOCH,
            last_write: None,
        });
        entry.last_read = SystemTime::now();
    }

    /// Record that we wrote a file.
    pub fn record_write(&self, path: &Path) {
        let mut records = self.records.write().unwrap();
        let entry = records.entry(path.to_path_buf()).or_insert(FileRecord {
            last_read: SystemTime::UNIX_EPOCH,
            last_write: None,
        });
        entry.last_write = Some(SystemTime::now());
    }

    /// Check if file was modified externally since last read.
    pub async fn check_external_modification(&self, path: &Path) -> io::Result<bool> {
        let (last_read, last_write) = {
            let records = self.records.read().unwrap();
            let Some(record) = records.get(path) else {
                return Ok(false);
            };
            (record.last_read, record.last_write)
        };

        let metadata = match fs::metadata(path).await {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(err) => return Err(err),
        };

        let modified = metadata.modified()?;
        if modified <= last_read {
            return Ok(false);
        }

        if let Some(last_write) = last_write {
            Ok(modified > last_write)
        } else {
            Ok(true)
        }
    }

    /// Get last read time for a file.
    pub fn last_read(&self, path: &Path) -> Option<SystemTime> {
        let records = self.records.read().unwrap();
        records.get(path).map(|record| record.last_read)
    }
}
