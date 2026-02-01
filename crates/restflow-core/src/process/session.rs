use portable_pty::{Child, ChildKiller, ExitStatus};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
pub struct SessionOutput {
    pub pending: String,
    pub aggregated: String,
}

pub struct ProcessSession {
    pub id: String,
    pub command: String,
    pub cwd: Option<String>,
    pub started_at: i64,
    pub writer: Mutex<Box<dyn Write + Send>>,
    pub output: Arc<Mutex<SessionOutput>>,
    pub child: Mutex<Box<dyn Child + Send + Sync>>,
    pub killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    exit_status: Mutex<Option<ExitStatus>>,
    read_closed: AtomicBool,
}

impl std::fmt::Debug for ProcessSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessSession")
            .field("id", &self.id)
            .field("command", &self.command)
            .field("cwd", &self.cwd)
            .field("started_at", &self.started_at)
            .finish_non_exhaustive()
    }
}

impl ProcessSession {
    pub fn new(
        id: String,
        command: String,
        cwd: Option<String>,
        started_at: i64,
        writer: Box<dyn Write + Send>,
        output: Arc<Mutex<SessionOutput>>,
        child: Box<dyn Child + Send + Sync>,
    ) -> Self {
        let killer = child.clone_killer();
        Self {
            id,
            command,
            cwd,
            started_at,
            writer: Mutex::new(writer),
            output,
            child: Mutex::new(child),
            killer: Mutex::new(killer),
            exit_status: Mutex::new(None),
            read_closed: AtomicBool::new(false),
        }
    }

    /// Kill the process
    pub fn kill(&self) -> anyhow::Result<()> {
        let mut killer = self
            .killer
            .lock()
            .map_err(|_| anyhow::anyhow!("Process session lock poisoned"))?;
        killer.kill()?;
        Ok(())
    }

    pub fn mark_read_closed(&self) {
        self.read_closed.store(true, Ordering::Release);
    }

    pub fn read_closed(&self) -> bool {
        self.read_closed.load(Ordering::Acquire)
    }

    pub fn exit_status(&self) -> Option<ExitStatus> {
        self.exit_status
            .lock()
            .ok()
            .and_then(|status| status.clone())
    }

    pub fn set_exit_status(&self, status: ExitStatus) {
        if let Ok(mut guard) = self.exit_status.lock() {
            *guard = Some(status);
        }
    }

    pub fn try_update_exit_status(&self) -> anyhow::Result<Option<ExitStatus>> {
        if self.exit_status().is_some() {
            return Ok(self.exit_status());
        }

        let mut child = self
            .child
            .lock()
            .map_err(|_| anyhow::anyhow!("Process session lock poisoned"))?;
        if let Some(status) = child.try_wait()? {
            self.set_exit_status(status.clone());
            return Ok(Some(status));
        }
        Ok(None)
    }
}

#[derive(Debug, Clone)]
pub struct FinishedSession {
    pub id: String,
    pub command: String,
    pub cwd: Option<String>,
    pub started_at: i64,
    pub finished_at: i64,
    pub exit_code: Option<i32>,
    pub output: String,
}
