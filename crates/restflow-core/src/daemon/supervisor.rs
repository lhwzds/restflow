use super::health::HealthChecker;
use super::process::{DaemonConfig, ProcessManager};
use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tracing::{error, info, warn};

#[derive(Clone)]
pub struct SupervisorConfig {
    pub check_interval: Duration,
    pub max_restarts: u32,
    pub restart_window: Duration,
    pub daemon_config: DaemonConfig,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(5),
            max_restarts: 5,
            restart_window: Duration::from_secs(60),
            daemon_config: DaemonConfig::default(),
        }
    }
}

pub struct Supervisor {
    process_manager: Arc<ProcessManager>,
    health_checker: Arc<HealthChecker>,
    config: SupervisorConfig,
}

impl Supervisor {
    pub fn new(
        process_manager: Arc<ProcessManager>,
        health_checker: Arc<HealthChecker>,
        config: SupervisorConfig,
    ) -> Self {
        Self {
            process_manager,
            health_checker,
            config,
        }
    }

    pub async fn run(&self, mut shutdown: broadcast::Receiver<()>) -> Result<()> {
        let mut restart_count = 0u32;
        let mut last_restart = Instant::now();

        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    info!("Supervisor shutting down");
                    break;
                }
                _ = tokio::time::sleep(self.config.check_interval) => {
                    let health = self.health_checker.check().await;
                    if !health.healthy {
                        warn!("Daemon unhealthy, attempting restart");
                        if last_restart.elapsed() > self.config.restart_window {
                            restart_count = 0;
                        }

                        if restart_count >= self.config.max_restarts {
                            error!("Max restart attempts reached, giving up");
                            break;
                        }

                        if let Err(err) = self.restart_daemon().await {
                            error!(error = %err, "Failed to restart daemon");
                        }

                        restart_count += 1;
                        last_restart = Instant::now();
                    }
                }
            }
        }

        Ok(())
    }

    async fn restart_daemon(&self) -> Result<()> {
        self.process_manager.stop()?;
        tokio::time::sleep(Duration::from_secs(1)).await;
        self.process_manager
            .start(self.config.daemon_config.clone())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supervisor_config_defaults() {
        let config = SupervisorConfig::default();

        assert_eq!(config.check_interval, Duration::from_secs(5));
        assert_eq!(config.max_restarts, 5);
        assert_eq!(config.restart_window, Duration::from_secs(60));

        // Verify nested DaemonConfig defaults.
        assert!(config.daemon_config.mcp);
        assert_eq!(config.daemon_config.mcp_port, Some(8787));
    }

    #[tokio::test]
    async fn shutdown_signal_stops_run() {
        // Create a ProcessManager (uses ~/.restflow/ paths but we never call
        // start/stop, so no actual daemon interaction occurs).
        let process_manager =
            Arc::new(ProcessManager::new().expect("ProcessManager::new should succeed in tests"));

        // HealthChecker pointed at a non-existent socket; we expect the
        // supervisor to exit via shutdown before any health check fires.
        let health_checker = Arc::new(HealthChecker::new(
            std::path::PathBuf::from("/tmp/restflow-test-nonexistent.sock"),
            None,
        ));

        let config = SupervisorConfig {
            // Use a very long interval so the health check branch never fires
            // before the shutdown signal.
            check_interval: Duration::from_secs(3600),
            ..Default::default()
        };

        let supervisor = Supervisor::new(process_manager, health_checker, config);

        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

        // Send shutdown before run() even starts its select loop iteration.
        let _ = shutdown_tx.send(());

        // run() must return promptly (within 2 seconds).
        let result = tokio::time::timeout(Duration::from_secs(2), supervisor.run(shutdown_rx))
            .await
            .expect("supervisor.run() should exit within timeout");

        assert!(result.is_ok(), "supervisor.run() should return Ok(())");
    }
}
