use anyhow::Result;
use dashmap::DashMap;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use restflow_ai::tools::{ProcessLog, ProcessManager, ProcessPollResult, ProcessSessionInfo};

mod session;

pub use session::{FinishedSession, ProcessSession, SessionOutput};

const DEFAULT_MAX_OUTPUT_BYTES: usize = 1_000_000;
const DEFAULT_TTL_SECONDS: u64 = 30 * 60;
const DEFAULT_PTY_SIZE: PtySize = PtySize {
    rows: 24,
    cols: 80,
    pixel_width: 0,
    pixel_height: 0,
};

#[derive(Debug, Clone)]
pub struct ProcessRegistry {
    sessions: Arc<DashMap<String, Arc<ProcessSession>>>,
    finished: Arc<DashMap<String, FinishedSession>>,
    max_output_bytes: usize,
    ttl: Duration,
}

impl Default for ProcessRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessRegistry {
    pub fn new() -> Self {
        let registry = Self {
            sessions: Arc::new(DashMap::new()),
            finished: Arc::new(DashMap::new()),
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            ttl: Duration::from_secs(DEFAULT_TTL_SECONDS),
        };

        registry.spawn_cleanup_task();
        registry
    }

    pub fn with_max_output(mut self, max_output_bytes: usize) -> Self {
        self.max_output_bytes = max_output_bytes;
        self
    }

    pub fn with_ttl_seconds(mut self, ttl_seconds: u64) -> Self {
        self.ttl = Duration::from_secs(ttl_seconds);
        self
    }

    fn spawn_cleanup_task(&self) {
        let finished = self.finished.clone();
        let ttl = self.ttl;
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    let now = current_timestamp_ms();
                    let expired: Vec<String> = finished
                        .iter()
                        .filter_map(|entry| {
                            if now.saturating_sub(entry.finished_at) > ttl.as_millis() as i64 {
                                Some(entry.key().clone())
                            } else {
                                None
                            }
                        })
                        .collect();

                    for session_id in expired {
                        finished.remove(&session_id);
                    }
                }
            });
        } else {
            tracing::warn!("No Tokio runtime found for process cleanup task");
        }
    }

    fn build_shell_command(command: &str) -> CommandBuilder {
        #[cfg(target_os = "windows")]
        {
            let mut cmd = CommandBuilder::new("cmd.exe");
            cmd.args(["/C", command]);
            cmd
        }
        #[cfg(not(target_os = "windows"))]
        {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
            let mut cmd = CommandBuilder::new(shell);
            cmd.args(["-c", command]);
            cmd
        }
    }

    fn append_output(output: &mut SessionOutput, data: &str, max_bytes: usize) {
        output.pending.push_str(data);
        output.aggregated.push_str(data);

        if output.pending.len() > max_bytes {
            let keep_from = output.pending.len() - (max_bytes * 9 / 10);
            output.pending = output.pending[keep_from..].to_string();
        }

        if output.aggregated.len() > max_bytes {
            let keep_from = output.aggregated.len() - (max_bytes * 9 / 10);
            output.aggregated = output.aggregated[keep_from..].to_string();
        }
    }

    fn slice_utf8(text: &str, offset: usize, limit: usize) -> String {
        if text.is_empty() {
            return String::new();
        }
        let mut start = offset.min(text.len());
        while start > 0 && !text.is_char_boundary(start) {
            start -= 1;
        }
        let mut end = start.saturating_add(limit).min(text.len());
        while end < text.len() && !text.is_char_boundary(end) {
            end += 1;
        }
        text[start..end].to_string()
    }

    fn take_pending(output: &Arc<Mutex<SessionOutput>>) -> String {
        if let Ok(mut guard) = output.lock() {
            let pending = guard.pending.clone();
            guard.pending.clear();
            return pending;
        }
        String::new()
    }

    fn session_status(exit_code: Option<i32>) -> String {
        match exit_code {
            None => "running".to_string(),
            Some(0) => "completed".to_string(),
            Some(_) => "failed".to_string(),
        }
    }

    fn finalize_session(&self, session: Arc<ProcessSession>, exit_code: Option<i32>) {
        let output = session
            .output
            .lock()
            .map(|o| o.aggregated.clone())
            .unwrap_or_default();
        let finished = FinishedSession {
            id: session.id.clone(),
            command: session.command.clone(),
            cwd: session.cwd.clone(),
            started_at: session.started_at,
            finished_at: current_timestamp_ms(),
            exit_code,
            output,
        };
        self.sessions.remove(&session.id);
        self.finished.insert(session.id.clone(), finished);
    }

    fn create_reader_thread(
        session: Arc<ProcessSession>,
        mut reader: Box<dyn Read + Send>,
        max_output: usize,
    ) {
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            let mut incomplete_utf8: Vec<u8> = Vec::new();
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        if !incomplete_utf8.is_empty() {
                            let data = String::from_utf8_lossy(&incomplete_utf8).to_string();
                            if let Ok(mut output) = session.output.lock() {
                                Self::append_output(&mut output, &data, max_output);
                            }
                        }
                        session.mark_read_closed();
                        break;
                    }
                    Ok(n) => {
                        let mut bytes = std::mem::take(&mut incomplete_utf8);
                        bytes.extend_from_slice(&buf[..n]);
                        let valid_up_to = find_utf8_boundary(&bytes);
                        if valid_up_to > 0 {
                            let data = String::from_utf8_lossy(&bytes[..valid_up_to]).to_string();
                            if let Ok(mut output) = session.output.lock() {
                                Self::append_output(&mut output, &data, max_output);
                            }
                        }
                        if valid_up_to < bytes.len() {
                            incomplete_utf8 = bytes[valid_up_to..].to_vec();
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Process output read error");
                        session.mark_read_closed();
                        break;
                    }
                }
            }
        });
    }

    pub fn spawn(&self, command: &str, cwd: Option<String>) -> Result<String> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(DEFAULT_PTY_SIZE)?;

        let mut cmd = Self::build_shell_command(command);
        if let Some(cwd) = cwd.as_ref() {
            cmd.cwd(cwd);
        }
        cmd.env("TERM", "xterm-256color");

        let child = pair.slave.spawn_command(cmd)?;
        let writer = pair.master.take_writer()?;
        let reader = pair.master.try_clone_reader()?;

        let session_id = Uuid::new_v4().to_string();
        let output = Arc::new(Mutex::new(SessionOutput::default()));
        let session = Arc::new(ProcessSession::new(
            session_id.clone(),
            command.to_string(),
            cwd,
            current_timestamp_ms(),
            writer,
            output.clone(),
            child,
        ));

        Self::create_reader_thread(session.clone(), reader, self.max_output_bytes);

        self.sessions.insert(session_id.clone(), session);
        Ok(session_id)
    }

    pub fn poll(&self, session_id: &str) -> Result<ProcessPollResult> {
        if let Some(session) = self.sessions.get(session_id) {
            let session = session.value().clone();
            let _ = session.try_update_exit_status();
            let pending = Self::take_pending(&session.output);
            let exit_code = session
                .exit_status()
                .map(|status| status.exit_code() as i32);
            let status = Self::session_status(exit_code);

            if exit_code.is_some() && session.read_closed() {
                self.finalize_session(session, exit_code);
            }

            return Ok(ProcessPollResult {
                session_id: session_id.to_string(),
                output: pending,
                status,
                exit_code,
            });
        }

        if let Some(finished) = self.finished.get(session_id) {
            let exit_code = finished.exit_code;
            return Ok(ProcessPollResult {
                session_id: session_id.to_string(),
                output: String::new(),
                status: Self::session_status(exit_code),
                exit_code,
            });
        }

        anyhow::bail!("Session not found: {}", session_id)
    }

    pub fn write(&self, session_id: &str, data: &str) -> Result<()> {
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;
        let mut writer = session
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("Process session lock poisoned"))?;
        writer.write_all(data.as_bytes())?;
        writer.flush()?;
        Ok(())
    }

    pub fn kill(&self, session_id: &str) -> Result<()> {
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;
        session.kill()?;
        Ok(())
    }

    pub fn list(&self) -> Vec<ProcessSessionInfo> {
        let mut items: Vec<ProcessSessionInfo> = self
            .sessions
            .iter()
            .map(|entry| {
                let session = entry.value();
                let exit_code = session
                    .exit_status()
                    .map(|status| status.exit_code() as i32);
                ProcessSessionInfo {
                    session_id: session.id.clone(),
                    command: session.command.clone(),
                    cwd: session.cwd.clone(),
                    started_at: session.started_at,
                    status: Self::session_status(exit_code),
                    exit_code,
                }
            })
            .collect();

        for entry in self.finished.iter() {
            items.push(ProcessSessionInfo {
                session_id: entry.id.clone(),
                command: entry.command.clone(),
                cwd: entry.cwd.clone(),
                started_at: entry.started_at,
                status: Self::session_status(entry.exit_code),
                exit_code: entry.exit_code,
            });
        }

        items
    }

    pub fn get_log(&self, session_id: &str, offset: usize, limit: usize) -> Result<ProcessLog> {
        if let Some(session) = self.sessions.get(session_id) {
            let output = session
                .output
                .lock()
                .map(|o| o.aggregated.clone())
                .unwrap_or_default();
            let total = output.len();
            let slice = Self::slice_utf8(&output, offset, limit);
            return Ok(ProcessLog {
                session_id: session_id.to_string(),
                output: slice,
                offset,
                limit,
                total,
                truncated: offset + limit < total,
            });
        }

        if let Some(finished) = self.finished.get(session_id) {
            let total = finished.output.len();
            let slice = Self::slice_utf8(&finished.output, offset, limit);
            return Ok(ProcessLog {
                session_id: session_id.to_string(),
                output: slice,
                offset,
                limit,
                total,
                truncated: offset + limit < total,
            });
        }

        anyhow::bail!("Session not found: {}", session_id)
    }
}

impl ProcessManager for ProcessRegistry {
    fn spawn(&self, command: String, cwd: Option<String>) -> Result<String> {
        Self::spawn(self, &command, cwd)
    }

    fn poll(&self, session_id: &str) -> Result<ProcessPollResult> {
        Self::poll(self, session_id)
    }

    fn write(&self, session_id: &str, data: &str) -> Result<()> {
        Self::write(self, session_id, data)
    }

    fn kill(&self, session_id: &str) -> Result<()> {
        Self::kill(self, session_id)
    }

    fn list(&self) -> Result<Vec<ProcessSessionInfo>> {
        Ok(Self::list(self))
    }

    fn log(&self, session_id: &str, offset: usize, limit: usize) -> Result<ProcessLog> {
        Self::get_log(self, session_id, offset, limit)
    }
}

fn current_timestamp_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn find_utf8_boundary(bytes: &[u8]) -> usize {
    match std::str::from_utf8(bytes) {
        Ok(_) => bytes.len(),
        Err(e) => e.valid_up_to(),
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[cfg(unix)]
    #[tokio::test]
    async fn test_spawn_and_poll() {
        let registry = ProcessRegistry::new();
        let session_id = registry.spawn("echo hello", None).unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        let result = registry.poll(&session_id).unwrap();
        assert!(result.output.contains("hello"));
        assert!(result.status == "completed" || result.status == "running");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_interactive_process() {
        let registry = ProcessRegistry::new();
        let session_id = registry.spawn("cat", None).unwrap();
        registry.write(&session_id, "ping\n").unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        let result = registry.poll(&session_id).unwrap();
        assert!(result.output.contains("ping"));
        registry.kill(&session_id).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_kill_session() {
        let registry = ProcessRegistry::new();
        let session_id = registry.spawn("sleep 5", None).unwrap();
        registry.kill(&session_id).unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        let result = registry.poll(&session_id).unwrap();
        assert!(result.status == "failed" || result.status == "completed");
    }
}
