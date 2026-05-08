use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use redb::{Database, DatabaseError};

#[derive(Debug, Clone)]
pub struct RedbLeaseProvider {
    path: Arc<PathBuf>,
    timeout: Duration,
    initial_delay: Duration,
    max_delay: Duration,
}

impl RedbLeaseProvider {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Arc::new(path.into()),
            timeout: Duration::from_secs(5),
            initial_delay: Duration::from_millis(50),
            max_delay: Duration::from_millis(250),
        }
    }

    #[cfg(test)]
    pub fn with_timing(
        path: impl Into<PathBuf>,
        timeout: Duration,
        initial_delay: Duration,
        max_delay: Duration,
    ) -> Self {
        Self {
            path: Arc::new(path.into()),
            timeout,
            initial_delay,
            max_delay,
        }
    }

    pub fn path(&self) -> &Path {
        self.path.as_ref()
    }

    pub fn with_database<T>(&self, operation: impl FnOnce(&Database) -> Result<T>) -> Result<T> {
        let db = self.open_database()?;
        operation(&db)
    }

    fn open_database(&self) -> Result<Database> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create database directory {}", parent.display())
            })?;
        }

        let started_at = Instant::now();
        let mut delay = self.initial_delay;
        loop {
            match Database::create(self.path.as_ref()) {
                Ok(db) => return Ok(db),
                Err(DatabaseError::DatabaseAlreadyOpen) if started_at.elapsed() < self.timeout => {
                    thread::sleep(delay);
                    delay = (delay * 2).min(self.max_delay);
                }
                Err(DatabaseError::DatabaseAlreadyOpen) => {
                    anyhow::bail!(
                        "Timed out waiting for redb lease on {}",
                        self.path.display()
                    );
                }
                Err(err) => return Err(err).context("Failed to open redb database"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lease_waits_until_existing_database_handle_is_released() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("lease.db");
        let held = Database::create(&db_path).unwrap();
        let provider = RedbLeaseProvider::with_timing(
            db_path.clone(),
            Duration::from_secs(2),
            Duration::from_millis(10),
            Duration::from_millis(25),
        );

        let handle = thread::spawn(move || provider.with_database(|_| Ok(())));
        thread::sleep(Duration::from_millis(100));
        drop(held);

        handle.join().unwrap().unwrap();
    }

    #[test]
    fn lease_times_out_when_database_stays_open() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("lease-timeout.db");
        let _held = Database::create(&db_path).unwrap();
        let provider = RedbLeaseProvider::with_timing(
            db_path,
            Duration::from_millis(50),
            Duration::from_millis(10),
            Duration::from_millis(10),
        );

        let error = provider.with_database(|_| Ok(())).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Timed out waiting for redb lease")
        );
    }
}
