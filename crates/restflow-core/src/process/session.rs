use portable_pty::{Child, ChildKiller, ExitStatus, MasterPty, PtySize};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessSessionSource {
    User,
    Agent,
}

impl Default for ProcessSessionSource {
    fn default() -> Self {
        Self::Agent
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProcessSessionMetadata {
    pub agent_id: Option<String>,
    pub task_id: Option<String>,
}

pub trait ProcessOutputListener: Send + Sync {
    fn on_output(&self, session_id: &str, data: &str);
    fn on_closed(&self, session_id: &str);
}

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
    pub source: ProcessSessionSource,
    pub metadata: ProcessSessionMetadata,
    pub writer: Mutex<Box<dyn Write + Send>>,
    pub master: Mutex<Box<dyn MasterPty + Send>>,
    pub output: Arc<Mutex<SessionOutput>>,
    pub output_listener: Option<Arc<dyn ProcessOutputListener>>,
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
        source: ProcessSessionSource,
        metadata: ProcessSessionMetadata,
        writer: Box<dyn Write + Send>,
        master: Box<dyn MasterPty + Send>,
        output: Arc<Mutex<SessionOutput>>,
        output_listener: Option<Arc<dyn ProcessOutputListener>>,
        child: Box<dyn Child + Send + Sync>,
    ) -> Self {
        let killer = child.clone_killer();
        Self {
            id,
            command,
            cwd,
            started_at,
            source,
            metadata,
            writer: Mutex::new(writer),
            master: Mutex::new(master),
            output,
            output_listener,
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

    pub fn resize(&self, size: PtySize) -> anyhow::Result<()> {
        let master = self
            .master
            .lock()
            .map_err(|_| anyhow::anyhow!("Process session lock poisoned"))?;
        master.resize(size)?;
        Ok(())
    }

    pub fn emit_output(&self, data: &str) {
        if let Some(listener) = self.output_listener.as_ref() {
            listener.on_output(&self.id, data);
        }
    }

    pub fn emit_closed(&self) {
        if let Some(listener) = self.output_listener.as_ref() {
            listener.on_closed(&self.id);
        }
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
