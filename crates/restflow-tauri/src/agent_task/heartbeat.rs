//! Heartbeat Runner - Periodic health monitoring for agent task system.
//!
//! The HeartbeatRunner provides:
//! - Periodic heartbeat events to the frontend for connection monitoring
//! - Runner health status tracking
//! - Active task monitoring
//! - System resource reporting
//!
//! # Architecture
//!
//! The heartbeat runner operates independently from the main task runner,
//! sending periodic status updates to connected frontends via Tauri events.
//!
//! # Usage
//!
//! ```ignore
//! use restflow_tauri::agent_task::{HeartbeatRunner, HeartbeatConfig};
//! use tauri::Manager;
//!
//! let heartbeat = HeartbeatRunner::new(HeartbeatConfig::default());
//! let handle = heartbeat.start(app_handle);
//!
//! // Later, to stop:
//! handle.stop().await?;
//! ```

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration, Instant};
use tracing::{debug, info, warn};
use ts_rs::TS;

/// Tauri event name for heartbeat events
pub const HEARTBEAT_EVENT: &str = "agent-task:heartbeat";

/// Configuration for the HeartbeatRunner
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// Heartbeat interval in milliseconds
    pub interval_ms: u64,
    /// Include system stats in heartbeat
    pub include_stats: bool,
    /// Maximum missed heartbeats before warning
    pub max_missed_heartbeats: u32,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval_ms: 5000,        // 5 seconds
            include_stats: true,
            max_missed_heartbeats: 3, // Warn after 15 seconds of no acks
        }
    }
}

/// Heartbeat event sent to the frontend
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HeartbeatEvent {
    /// Regular heartbeat pulse
    Pulse(HeartbeatPulse),
    /// Runner status changed
    StatusChange(RunnerStatusEvent),
    /// Warning about missed heartbeats or other issues
    Warning(HeartbeatWarning),
}

/// Regular heartbeat pulse data
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HeartbeatPulse {
    /// Sequence number for this heartbeat
    #[ts(type = "number")]
    pub sequence: u64,
    /// Timestamp of this heartbeat (milliseconds since epoch)
    #[ts(type = "number")]
    pub timestamp: i64,
    /// Number of active (running) tasks
    pub active_tasks: u32,
    /// Number of pending tasks (scheduled but not yet run)
    pub pending_tasks: u32,
    /// Runner uptime in milliseconds
    #[ts(type = "number")]
    pub uptime_ms: u64,
    /// Optional system stats
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<SystemStats>,
}

/// System statistics included in heartbeat
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SystemStats {
    /// Memory usage in bytes (if available)
    #[ts(type = "number | null")]
    pub memory_bytes: Option<u64>,
    /// Number of tokio tasks (if available)
    pub tokio_tasks: Option<u32>,
}

/// Runner status change event
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunnerStatusEvent {
    /// Current runner status
    pub status: RunnerStatus,
    /// Timestamp of the status change
    #[ts(type = "number")]
    pub timestamp: i64,
    /// Optional message about the status change
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Runner status enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum RunnerStatus {
    /// Runner is starting up
    Starting,
    /// Runner is running normally
    Running,
    /// Runner is paused
    Paused,
    /// Runner is stopping
    Stopping,
    /// Runner has stopped
    Stopped,
    /// Runner encountered an error
    Error,
}

/// Warning event for issues detected by heartbeat
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HeartbeatWarning {
    /// Warning code for categorization
    pub code: String,
    /// Human-readable warning message
    pub message: String,
    /// Timestamp of the warning
    #[ts(type = "number")]
    pub timestamp: i64,
}

/// Commands for controlling the heartbeat runner
#[derive(Debug)]
pub enum HeartbeatCommand {
    /// Stop the heartbeat runner
    Stop,
    /// Update the task counts
    UpdateCounts { active: u32, pending: u32 },
    /// Acknowledge a heartbeat (from frontend)
    Ack { sequence: u64 },
    /// Force emit a status change
    EmitStatus { status: RunnerStatus, message: Option<String> },
}

/// Handle to control a running HeartbeatRunner
pub struct HeartbeatHandle {
    command_tx: mpsc::Sender<HeartbeatCommand>,
}

impl HeartbeatHandle {
    /// Stop the heartbeat runner
    pub async fn stop(&self) -> Result<()> {
        self.command_tx
            .send(HeartbeatCommand::Stop)
            .await
            .map_err(|e| anyhow!("Failed to send stop command: {}", e))
    }

    /// Update task counts
    pub async fn update_counts(&self, active: u32, pending: u32) -> Result<()> {
        self.command_tx
            .send(HeartbeatCommand::UpdateCounts { active, pending })
            .await
            .map_err(|e| anyhow!("Failed to send update counts command: {}", e))
    }

    /// Acknowledge a heartbeat (call from frontend event handler)
    pub async fn ack(&self, sequence: u64) -> Result<()> {
        self.command_tx
            .send(HeartbeatCommand::Ack { sequence })
            .await
            .map_err(|e| anyhow!("Failed to send ack command: {}", e))
    }

    /// Emit a status change event
    pub async fn emit_status(&self, status: RunnerStatus, message: Option<String>) -> Result<()> {
        self.command_tx
            .send(HeartbeatCommand::EmitStatus { status, message })
            .await
            .map_err(|e| anyhow!("Failed to send emit status command: {}", e))
    }
}

/// Trait for emitting heartbeat events (allows dependency injection)
#[async_trait::async_trait]
pub trait HeartbeatEmitter: Send + Sync {
    /// Emit a heartbeat event
    async fn emit(&self, event: HeartbeatEvent);
}

/// Tauri-based heartbeat emitter
#[derive(Clone)]
pub struct TauriHeartbeatEmitter {
    app_handle: tauri::AppHandle,
}

impl TauriHeartbeatEmitter {
    pub fn new(app_handle: tauri::AppHandle) -> Self {
        Self { app_handle }
    }
}

#[async_trait::async_trait]
impl HeartbeatEmitter for TauriHeartbeatEmitter {
    async fn emit(&self, event: HeartbeatEvent) {
        use tauri::Emitter;
        if let Err(e) = self.app_handle.emit(HEARTBEAT_EVENT, &event) {
            warn!("Failed to emit heartbeat event: {}", e);
        }
    }
}

/// Channel-based heartbeat emitter for testing
pub struct ChannelHeartbeatEmitter {
    sender: mpsc::Sender<HeartbeatEvent>,
}

impl ChannelHeartbeatEmitter {
    pub fn new(sender: mpsc::Sender<HeartbeatEvent>) -> Self {
        Self { sender }
    }
}

#[async_trait::async_trait]
impl HeartbeatEmitter for ChannelHeartbeatEmitter {
    async fn emit(&self, event: HeartbeatEvent) {
        let _ = self.sender.send(event).await;
    }
}

/// No-op heartbeat emitter for when heartbeats are disabled
pub struct NoopHeartbeatEmitter;

#[async_trait::async_trait]
impl HeartbeatEmitter for NoopHeartbeatEmitter {
    async fn emit(&self, _event: HeartbeatEvent) {
        // No-op
    }
}

/// Internal state for the heartbeat runner
struct HeartbeatState {
    active_tasks: u32,
    pending_tasks: u32,
    last_ack_sequence: u64,
    missed_heartbeats: u32,
}

impl Default for HeartbeatState {
    fn default() -> Self {
        Self {
            active_tasks: 0,
            pending_tasks: 0,
            last_ack_sequence: 0,
            missed_heartbeats: 0,
        }
    }
}

/// The HeartbeatRunner that sends periodic heartbeat events
pub struct HeartbeatRunner {
    config: HeartbeatConfig,
    sequence: AtomicU64,
    state: Arc<RwLock<HeartbeatState>>,
    start_time: Instant,
}

impl HeartbeatRunner {
    /// Create a new HeartbeatRunner with the given configuration
    pub fn new(config: HeartbeatConfig) -> Self {
        Self {
            config,
            sequence: AtomicU64::new(0),
            state: Arc::new(RwLock::new(HeartbeatState::default())),
            start_time: Instant::now(),
        }
    }

    /// Start the heartbeat runner with a Tauri app handle
    pub fn start_with_tauri(self: Arc<Self>, app_handle: tauri::AppHandle) -> HeartbeatHandle {
        let emitter = Arc::new(TauriHeartbeatEmitter::new(app_handle));
        self.start(emitter)
    }

    /// Start the heartbeat runner with a custom emitter
    pub fn start(self: Arc<Self>, emitter: Arc<dyn HeartbeatEmitter>) -> HeartbeatHandle {
        let (command_tx, command_rx) = mpsc::channel(32);
        let runner = self.clone();

        tokio::spawn(async move {
            runner.run_loop(command_rx, emitter).await;
        });

        HeartbeatHandle { command_tx }
    }

    /// Main run loop
    async fn run_loop(
        self: Arc<Self>,
        mut command_rx: mpsc::Receiver<HeartbeatCommand>,
        emitter: Arc<dyn HeartbeatEmitter>,
    ) {
        let mut heartbeat_interval = interval(Duration::from_millis(self.config.interval_ms));

        info!(
            "HeartbeatRunner started (interval={}ms)",
            self.config.interval_ms
        );

        // Emit initial status
        emitter
            .emit(HeartbeatEvent::StatusChange(RunnerStatusEvent {
                status: RunnerStatus::Running,
                timestamp: chrono::Utc::now().timestamp_millis(),
                message: Some("Heartbeat runner started".to_string()),
            }))
            .await;

        loop {
            tokio::select! {
                _ = heartbeat_interval.tick() => {
                    self.emit_heartbeat(&emitter).await;
                }
                cmd = command_rx.recv() => {
                    match cmd {
                        Some(HeartbeatCommand::Stop) => {
                            info!("HeartbeatRunner stopping...");
                            emitter.emit(HeartbeatEvent::StatusChange(RunnerStatusEvent {
                                status: RunnerStatus::Stopped,
                                timestamp: chrono::Utc::now().timestamp_millis(),
                                message: Some("Heartbeat runner stopped".to_string()),
                            })).await;
                            break;
                        }
                        Some(HeartbeatCommand::UpdateCounts { active, pending }) => {
                            let mut state = self.state.write().await;
                            state.active_tasks = active;
                            state.pending_tasks = pending;
                            debug!("Updated task counts: active={}, pending={}", active, pending);
                        }
                        Some(HeartbeatCommand::Ack { sequence }) => {
                            let mut state = self.state.write().await;
                            if sequence > state.last_ack_sequence {
                                state.last_ack_sequence = sequence;
                                state.missed_heartbeats = 0;
                                debug!("Heartbeat ack received: sequence={}", sequence);
                            }
                        }
                        Some(HeartbeatCommand::EmitStatus { status, message }) => {
                            emitter.emit(HeartbeatEvent::StatusChange(RunnerStatusEvent {
                                status,
                                timestamp: chrono::Utc::now().timestamp_millis(),
                                message,
                            })).await;
                        }
                        None => {
                            info!("Command channel closed, stopping heartbeat runner");
                            break;
                        }
                    }
                }
            }
        }

        info!("HeartbeatRunner stopped");
    }

    /// Emit a heartbeat pulse
    async fn emit_heartbeat(&self, emitter: &Arc<dyn HeartbeatEmitter>) {
        let sequence = self.sequence.fetch_add(1, Ordering::SeqCst) + 1;
        let state = self.state.read().await;
        let uptime_ms = self.start_time.elapsed().as_millis() as u64;

        // Check for missed heartbeats
        let current_sequence = sequence;
        let last_ack = state.last_ack_sequence;
        
        // We allow some lag - only count as missed if more than max_missed behind
        let missed = if current_sequence > last_ack + 1 {
            (current_sequence - last_ack - 1).min(u32::MAX as u64) as u32
        } else {
            0
        };

        // Emit warning if too many missed
        if missed >= self.config.max_missed_heartbeats {
            emitter
                .emit(HeartbeatEvent::Warning(HeartbeatWarning {
                    code: "HEARTBEAT_MISSED".to_string(),
                    message: format!(
                        "Frontend has not acknowledged {} heartbeats. Connection may be lost.",
                        missed
                    ),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                }))
                .await;
        }

        let stats = if self.config.include_stats {
            Some(SystemStats {
                memory_bytes: None, // Could add actual memory stats here
                tokio_tasks: None,  // Could add tokio runtime stats
            })
        } else {
            None
        };

        let pulse = HeartbeatPulse {
            sequence,
            timestamp: chrono::Utc::now().timestamp_millis(),
            active_tasks: state.active_tasks,
            pending_tasks: state.pending_tasks,
            uptime_ms,
            stats,
        };

        debug!(
            "Emitting heartbeat: seq={}, active={}, pending={}",
            sequence, state.active_tasks, state.pending_tasks
        );

        emitter.emit(HeartbeatEvent::Pulse(pulse)).await;
    }

    /// Get the current sequence number
    pub fn current_sequence(&self) -> u64 {
        self.sequence.load(Ordering::SeqCst)
    }

    /// Get the uptime in milliseconds
    pub fn uptime_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_heartbeat_runner_start_stop() {
        let config = HeartbeatConfig {
            interval_ms: 100,
            ..Default::default()
        };

        let (tx, mut rx) = mpsc::channel(100);
        let emitter = Arc::new(ChannelHeartbeatEmitter::new(tx));
        let runner = Arc::new(HeartbeatRunner::new(config));
        
        let handle = runner.start(emitter);

        // Wait for initial status event
        let event = timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("Should receive event")
            .expect("Channel should not be closed");

        match event {
            HeartbeatEvent::StatusChange(status) => {
                assert_eq!(status.status, RunnerStatus::Running);
            }
            _ => panic!("Expected StatusChange event"),
        }

        // Stop the runner
        handle.stop().await.unwrap();

        // Give it time to stop
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    #[tokio::test]
    async fn test_heartbeat_emits_pulses() {
        let config = HeartbeatConfig {
            interval_ms: 50,
            include_stats: true,
            ..Default::default()
        };

        let (tx, mut rx) = mpsc::channel(100);
        let emitter = Arc::new(ChannelHeartbeatEmitter::new(tx));
        let runner = Arc::new(HeartbeatRunner::new(config));

        let handle = runner.start(emitter);

        // Skip initial status event
        let _ = timeout(Duration::from_millis(200), rx.recv()).await;

        // Wait for a pulse
        let event = timeout(Duration::from_millis(200), rx.recv())
            .await
            .expect("Should receive event")
            .expect("Channel should not be closed");

        match event {
            HeartbeatEvent::Pulse(pulse) => {
                assert!(pulse.sequence >= 1);
                assert!(pulse.timestamp > 0);
                assert!(pulse.stats.is_some());
            }
            _ => panic!("Expected Pulse event, got {:?}", event),
        }

        handle.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_heartbeat_update_counts() {
        let config = HeartbeatConfig {
            interval_ms: 50,
            ..Default::default()
        };

        let (tx, mut rx) = mpsc::channel(100);
        let emitter = Arc::new(ChannelHeartbeatEmitter::new(tx));
        let runner = Arc::new(HeartbeatRunner::new(config));

        let handle = runner.start(emitter);

        // Update counts
        handle.update_counts(5, 10).await.unwrap();

        // Skip initial status event
        let _ = timeout(Duration::from_millis(100), rx.recv()).await;

        // Wait for a pulse with updated counts
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Drain events until we find a pulse with our counts
        let mut found = false;
        for _ in 0..10 {
            if let Ok(Some(event)) = timeout(Duration::from_millis(100), rx.recv()).await {
                if let HeartbeatEvent::Pulse(pulse) = event {
                    if pulse.active_tasks == 5 && pulse.pending_tasks == 10 {
                        found = true;
                        break;
                    }
                }
            }
        }

        assert!(found, "Should have received pulse with updated counts");

        handle.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_heartbeat_ack() {
        let config = HeartbeatConfig {
            interval_ms: 100,
            max_missed_heartbeats: 10, // High threshold to avoid warnings
            ..Default::default()
        };

        let (tx, mut rx) = mpsc::channel(100);
        let emitter = Arc::new(ChannelHeartbeatEmitter::new(tx));
        let runner = Arc::new(HeartbeatRunner::new(config));

        let handle = runner.clone().start(emitter);

        // Wait for first pulse and get its sequence
        let mut first_sequence = 0u64;
        for _ in 0..5 {
            if let Ok(Some(event)) = timeout(Duration::from_millis(200), rx.recv()).await {
                if let HeartbeatEvent::Pulse(pulse) = event {
                    first_sequence = pulse.sequence;
                    break;
                }
            }
        }

        assert!(first_sequence > 0, "Should have received at least one pulse");

        // Ack the pulse - this should work without error
        handle.ack(first_sequence).await.unwrap();

        // Verify we can still receive more pulses after acking
        let mut received_more = false;
        for _ in 0..5 {
            if let Ok(Some(event)) = timeout(Duration::from_millis(200), rx.recv()).await {
                if let HeartbeatEvent::Pulse(pulse) = event {
                    if pulse.sequence > first_sequence {
                        received_more = true;
                        break;
                    }
                }
            }
        }

        assert!(received_more, "Should receive more pulses after acking");

        handle.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_heartbeat_emit_status() {
        let config = HeartbeatConfig {
            interval_ms: 1000, // Long interval so we don't get pulses
            ..Default::default()
        };

        let (tx, mut rx) = mpsc::channel(100);
        let emitter = Arc::new(ChannelHeartbeatEmitter::new(tx));
        let runner = Arc::new(HeartbeatRunner::new(config));

        let handle = runner.start(emitter);

        // Skip initial status event
        let _ = timeout(Duration::from_millis(100), rx.recv()).await;

        // Emit a custom status
        handle
            .emit_status(RunnerStatus::Paused, Some("User paused".to_string()))
            .await
            .unwrap();

        // Wait for status event
        let event = timeout(Duration::from_millis(200), rx.recv())
            .await
            .expect("Should receive event")
            .expect("Channel should not be closed");

        match event {
            HeartbeatEvent::StatusChange(status) => {
                assert_eq!(status.status, RunnerStatus::Paused);
                assert_eq!(status.message, Some("User paused".to_string()));
            }
            _ => panic!("Expected StatusChange event"),
        }

        handle.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_heartbeat_pulse_serialization() {
        let pulse = HeartbeatPulse {
            sequence: 42,
            timestamp: 1704067200000,
            active_tasks: 3,
            pending_tasks: 7,
            uptime_ms: 60000,
            stats: Some(SystemStats {
                memory_bytes: Some(1024 * 1024 * 100),
                tokio_tasks: Some(15),
            }),
        };

        let event = HeartbeatEvent::Pulse(pulse);
        let json = serde_json::to_string(&event).unwrap();

        assert!(json.contains("\"kind\":\"pulse\""));
        assert!(json.contains("\"sequence\":42"));
        assert!(json.contains("\"active_tasks\":3"));
    }

    #[tokio::test]
    async fn test_runner_status_serialization() {
        let status = RunnerStatusEvent {
            status: RunnerStatus::Running,
            timestamp: 1704067200000,
            message: Some("All systems go".to_string()),
        };

        let event = HeartbeatEvent::StatusChange(status);
        let json = serde_json::to_string(&event).unwrap();

        assert!(json.contains("\"kind\":\"status_change\""));
        assert!(json.contains("\"status\":\"running\""));
    }

    #[tokio::test]
    async fn test_warning_serialization() {
        let warning = HeartbeatWarning {
            code: "TEST_WARNING".to_string(),
            message: "This is a test".to_string(),
            timestamp: 1704067200000,
        };

        let event = HeartbeatEvent::Warning(warning);
        let json = serde_json::to_string(&event).unwrap();

        assert!(json.contains("\"kind\":\"warning\""));
        assert!(json.contains("\"code\":\"TEST_WARNING\""));
    }

    #[tokio::test]
    async fn test_heartbeat_sequence_increments() {
        let config = HeartbeatConfig {
            interval_ms: 30,
            ..Default::default()
        };

        let (tx, mut rx) = mpsc::channel(100);
        let emitter = Arc::new(ChannelHeartbeatEmitter::new(tx));
        let runner = Arc::new(HeartbeatRunner::new(config));

        let handle = runner.clone().start(emitter);

        // Skip initial status event
        let _ = timeout(Duration::from_millis(100), rx.recv()).await;

        // Collect several pulses
        let mut sequences = Vec::new();
        for _ in 0..3 {
            if let Ok(Some(event)) = timeout(Duration::from_millis(100), rx.recv()).await {
                if let HeartbeatEvent::Pulse(pulse) = event {
                    sequences.push(pulse.sequence);
                }
            }
        }

        handle.stop().await.unwrap();

        // Verify sequences are incrementing
        assert!(sequences.len() >= 2, "Should have received at least 2 pulses");
        for i in 1..sequences.len() {
            assert!(
                sequences[i] > sequences[i - 1],
                "Sequence should increment: {} should be > {}",
                sequences[i],
                sequences[i - 1]
            );
        }
    }

    #[tokio::test]
    async fn test_heartbeat_uptime_increases() {
        let config = HeartbeatConfig {
            interval_ms: 50,
            ..Default::default()
        };

        let (tx, mut rx) = mpsc::channel(100);
        let emitter = Arc::new(ChannelHeartbeatEmitter::new(tx));
        let runner = Arc::new(HeartbeatRunner::new(config));

        let handle = runner.start(emitter);

        // Skip initial status event
        let _ = timeout(Duration::from_millis(100), rx.recv()).await;

        // Get first pulse
        let first_uptime = loop {
            if let Ok(Some(event)) = timeout(Duration::from_millis(200), rx.recv()).await {
                if let HeartbeatEvent::Pulse(pulse) = event {
                    break pulse.uptime_ms;
                }
            }
        };

        // Wait a bit and get another pulse
        tokio::time::sleep(Duration::from_millis(100)).await;

        let second_uptime = loop {
            if let Ok(Some(event)) = timeout(Duration::from_millis(200), rx.recv()).await {
                if let HeartbeatEvent::Pulse(pulse) = event {
                    break pulse.uptime_ms;
                }
            }
        };

        handle.stop().await.unwrap();

        assert!(
            second_uptime > first_uptime,
            "Uptime should increase: {} should be > {}",
            second_uptime,
            first_uptime
        );
    }

    #[tokio::test]
    async fn test_noop_emitter() {
        let emitter = NoopHeartbeatEmitter;
        
        // Should not panic or error
        emitter.emit(HeartbeatEvent::Pulse(HeartbeatPulse {
            sequence: 1,
            timestamp: 0,
            active_tasks: 0,
            pending_tasks: 0,
            uptime_ms: 0,
            stats: None,
        })).await;
    }

    #[test]
    fn test_heartbeat_config_default() {
        let config = HeartbeatConfig::default();
        
        assert_eq!(config.interval_ms, 5000);
        assert!(config.include_stats);
        assert_eq!(config.max_missed_heartbeats, 3);
    }

    #[test]
    fn test_runner_status_variants() {
        let statuses = vec![
            RunnerStatus::Starting,
            RunnerStatus::Running,
            RunnerStatus::Paused,
            RunnerStatus::Stopping,
            RunnerStatus::Stopped,
            RunnerStatus::Error,
        ];

        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let deserialized: RunnerStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, deserialized);
        }
    }
}
