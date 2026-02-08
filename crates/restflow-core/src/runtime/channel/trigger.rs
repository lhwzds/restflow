//! Task trigger interface for channel handlers
//!
//! This module defines the BackgroundAgentTrigger trait that bridges the channel message
//! handlers with the task execution system.

use crate::models::BackgroundAgent;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// System status for /status command
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SystemStatus {
    /// Whether the task runner is active
    pub runner_active: bool,
    /// Number of currently running tasks
    pub active_count: usize,
    /// Number of pending tasks
    pub pending_count: usize,
    /// Number of tasks completed today
    pub completed_today: usize,
}

/// Interface for triggering task operations from channel handlers
///
/// This trait is implemented by the application state to allow the channel
/// handlers to interact with the task execution system without tight coupling.
#[async_trait]
pub trait BackgroundAgentTrigger: Send + Sync {
    /// List all tasks
    async fn list_background_agents(&self) -> Result<Vec<BackgroundAgent>>;

    /// Find task by name or ID and run it
    async fn find_and_run_background_agent(&self, name_or_id: &str) -> Result<BackgroundAgent>;

    /// Stop a running task
    async fn stop_background_agent(&self, task_id: &str) -> Result<()>;

    /// Get system status
    async fn get_status(&self) -> Result<SystemStatus>;

    /// Send input to a running task
    async fn send_message_to_background_agent(&self, task_id: &str, input: &str) -> Result<()>;

    /// Handle approval response for a task
    ///
    /// Returns true if there was a pending approval to handle
    async fn handle_background_agent_approval(&self, task_id: &str, approved: bool)
    -> Result<bool>;
}

#[cfg(test)]
pub mod mock {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tokio::sync::Mutex;

    /// Mock task trigger for testing
    pub struct MockBackgroundAgentTrigger {
        tasks: Arc<Mutex<Vec<BackgroundAgent>>>,
        runner_active: AtomicBool,
        active_count: AtomicUsize,
        pending_count: AtomicUsize,
        completed_today: AtomicUsize,
        pub last_input: Arc<Mutex<Option<(String, String)>>>,
        pub last_approval: Arc<Mutex<Option<(String, bool)>>>,
    }

    impl MockBackgroundAgentTrigger {
        pub fn new() -> Self {
            Self {
                tasks: Arc::new(Mutex::new(Vec::new())),
                runner_active: AtomicBool::new(true),
                active_count: AtomicUsize::new(0),
                pending_count: AtomicUsize::new(0),
                completed_today: AtomicUsize::new(0),
                last_input: Arc::new(Mutex::new(None)),
                last_approval: Arc::new(Mutex::new(None)),
            }
        }

        pub async fn add_task(&self, task: BackgroundAgent) {
            self.tasks.lock().await.push(task);
        }

        pub fn set_runner_active(&self, active: bool) {
            self.runner_active.store(active, Ordering::SeqCst);
        }

        pub fn set_active_count(&self, count: usize) {
            self.active_count.store(count, Ordering::SeqCst);
        }
    }

    impl Default for MockBackgroundAgentTrigger {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait]
    impl BackgroundAgentTrigger for MockBackgroundAgentTrigger {
        async fn list_background_agents(&self) -> Result<Vec<BackgroundAgent>> {
            Ok(self.tasks.lock().await.clone())
        }

        async fn find_and_run_background_agent(&self, name_or_id: &str) -> Result<BackgroundAgent> {
            let tasks = self.tasks.lock().await;
            tasks
                .iter()
                .find(|t| t.id == name_or_id || t.name == name_or_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Background agent not found: {}", name_or_id))
        }

        async fn stop_background_agent(&self, _task_id: &str) -> Result<()> {
            Ok(())
        }

        async fn get_status(&self) -> Result<SystemStatus> {
            Ok(SystemStatus {
                runner_active: self.runner_active.load(Ordering::SeqCst),
                active_count: self.active_count.load(Ordering::SeqCst),
                pending_count: self.pending_count.load(Ordering::SeqCst),
                completed_today: self.completed_today.load(Ordering::SeqCst),
            })
        }

        async fn send_message_to_background_agent(&self, task_id: &str, input: &str) -> Result<()> {
            *self.last_input.lock().await = Some((task_id.to_string(), input.to_string()));
            Ok(())
        }

        async fn handle_background_agent_approval(
            &self,
            task_id: &str,
            approved: bool,
        ) -> Result<bool> {
            *self.last_approval.lock().await = Some((task_id.to_string(), approved));
            Ok(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_status_serialize() {
        let status = SystemStatus {
            runner_active: true,
            active_count: 2,
            pending_count: 5,
            completed_today: 10,
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("runner_active"));
        assert!(json.contains("true"));
    }
}
